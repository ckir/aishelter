---
name: Agent Commons v0.2 Production Readiness Design
description: Design for v0.2 deliverables: integration tests with testcontainers, Prometheus metrics + readiness + trace IDs, rate limiting, and OpenAPI validation
type: project
---

# Agent Commons v0.2 — Production Readiness Design

**Date:** 2026-09-06
**Status:** Approved for implementation
**Source:** ROADMAP.md — Q4 2026 v0.2

## Goal

Make Agent Commons deployable in production environments with automated testing, monitoring, security hardening, and API contract validation.

## Architecture Overview

v0.2 adds four deliverables on top of the existing v0.1 codebase. The workspace grows from 11 to 12 crates with the addition of `ac-metrics`.

```
crates/
├── ac-types/       — shared data types
├── ac-crypto/      — Ed25519 signing, request auth
├── ac-db/          — PostgreSQL access layer
├── ac-api/         — HTTP types, error mapping
├── ac-registry/    — agent registration, Agent Cards
├── ac-discovery/   — capability-based search
├── ac-mailbox/     — persistent message store
├── ac-tasks/       — task lifecycle
├── ac-validation/  — peer validation, quorum
├── ac-reputation/  — reputation events, VWU, scoring
├── ac-metrics/     — Prometheus registry, readiness, metrics middleware  [NEW]
└── ac-server/      — axum HTTP server binary + rate limiting middleware
```

## Deliverable 1: Integration Test Suite

### Problem

Existing `tests/e2e.rs` requires a manually running server (`TEST_BASE_URL`) and PostgreSQL instance. All 11 tests are `#[ignore]` — they don't run in CI automatically.

### Approach

Add `testcontainers-rs` to spin up PostgreSQL per-test, start an in-process axum server via `tower::ServiceExt::oneshot`. No external server needed. Tests run in CI automatically.

### Architecture

```
tests/integration/
├── harness.rs          — testcontainers + axum Router setup
├── registry.rs         — per-endpoint tests for /v1/agents
├── discovery.rs        — per-endpoint tests for /v1/discovery
├── mailbox.rs          — per-endpoint tests for /v1/messages
├── tasks.rs            — per-endpoint tests for /v1/tasks
├── validation.rs       — per-endpoint tests for /v1/validations
├── reputation.rs       — per-endpoint tests for /v1/reputation
├── openapi.rs          — OpenAPI schema conformance tests
└── adversarial.rs      — expanded adversarial tests
```

### Harness Design

`harness.rs` provides:
1. `async fn setup() -> TestApp` — starts PostgreSQL container, creates pool, runs migrations, builds axum Router
2. `TestApp` wraps a `Router` and provides `async fn request(&self, req: Request) -> Response` via `oneshot`
3. `async fn teardown(self)` — drops the container
4. Helper functions mirroring the existing `tests/e2e.rs` API helpers but working against a `Router` directly

### Existing e2e.rs

`tests/e2e.rs` remains as-is for manual/dev use against a running server. New `tests/integration/` covers the automated CI path.

### Test Coverage

Each endpoint gets:
- **Happy path** — valid request, 200/201 response, body matches expected
- **Validation errors** — missing fields, wrong types, 400 response
- **Not found** — nonexistent resource, 404
- **Conflict** — duplicate creation, 409
- **Server error** — internal failure, 500 (should not happen, but tested)

The three demonstration scenarios (§63-65) are covered:
- §63: Full lifecycle (already in e2e.rs, also in integration)
- §64: Async mailbox (already in e2e.rs, also in integration)
- §65: Adversarial (expanded in integration/adversarial.rs)

### Dependencies Added

| Crate | Scope | Purpose |
|---|---|---|
| `testcontainers` | dev-dependency | PostgreSQL container |
| `testcontainers-modules` | dev-dependency | Postgres module |
| `tower` | dev-dependency (workspace) | ServiceExt::oneshot |

### Justfile

New target: `just e2e` — runs `cargo nextest run --test integration` (testcontainer-based tests that need no external server).

## Deliverable 2: Metrics & Observability

### Problem

No Prometheus metrics, no readiness probe, no trace IDs in structured logs. The server is a black box in production.

### New Crate: ac-metrics

A dedicated crate for Prometheus metrics, isolated from the server crate. This keeps the Prometheus dependency contained and makes it reusable if other crates need metrics later.

#### Contents

```
crates/ac-metrics/
├── Cargo.toml
└── src/
    ├── lib.rs          — pub use of registry, middleware, readiness
    ├── registry.rs     — Prometheus Registry + metric definitions
    ├── readiness.rs    — readiness check (DB connectivity)
    └── middleware.rs   — tower middleware for request metrics
```

#### Metrics Defined

| Metric | Type | Labels | Description |
|---|---|---|---|
| `http_requests_total` | Counter | `method`, `path`, `status` | Total HTTP requests |
| `http_request_duration_seconds` | Histogram | `method`, `path` | Request latency |
| `db_pool_connections_active` | Gauge | — | Active DB pool connections |
| `db_query_duration_seconds` | Histogram | — | SQL query latency (via sqlx interceptor) |
| `active_tasks` | Gauge | `status` | Tasks by status |
| `messages_pending` | Gauge | — | Unacknowledged messages |

#### Endpoints

| Endpoint | Method | Description |
|---|---|---|
| `/metrics` | GET | Prometheus metrics (text/plain) |
| `/v1/ready` | GET | Readiness probe — returns 200 only if DB is reachable |

#### Readiness Check Implementation

`ac-metrics::readiness::check(pool)` — attempts `pool.acquire_timeout(1s)`. Returns 200 on success, 503 on timeout. Called on every `/v1/ready` request.

#### Trace IDs

Add `tower_http::request_id::SetRequestIdLayer` with header name `X-Request-Id`. The existing `TraceLayer` already logs request/response — configure it to include the request ID in the log output via `DefaultMakeSpan::with_level(Level::INFO).include_headers(true)`.

### Wiring in ac-server

- `main.rs`: Create Prometheus registry, wrap DB pool with metrics interceptor, pass both to `create_app`
- `server.rs`: Add `/metrics` and `/v1/ready` routes, add `SetRequestIdLayer` + metrics middleware to router
- `config.rs`: No new config keys for metrics (defaults are fine)

### docker-compose.yml

Add optional Prometheus container for local development:

```yaml
  prometheus:
    image: prom/prometheus:latest
    ports:
      - "9090:9090"
    volumes:
      - ./docker/prometheus.yml:/etc/prometheus/prometheus.yml
    depends_on:
      - commons
```

### Dependencies Added

| Crate | Scope | Purpose |
|---|---|---|
| `prometheus-client` | workspace | Prometheus metrics collection (Rust-native crate) |

### Why prometheus-client, not prometheus

`prometheus-client` is the newer, actively maintained Rust-native Prometheus client. The older `prometheus` crate wraps the Go client and has more dependencies.

## Deliverable 3: Rate Limiting

### Problem

No request throttling. Any agent can flood the server with unlimited requests.

### Approach

Two-tier rate limiting using `tower-http` (already declared with `limit` feature):

1. **Global limit** — 1000 requests per 60 seconds across all clients
2. **Per-agent limit** — 100 requests per 60 seconds per agent ID

### Implementation

New module: `crates/ac-server/src/middleware/rate_limit.rs`

```rust
pub struct AgentRateLimit {
    global: RateLimitLayer,
    per_agent: Arc<Mutex<HashMap<String, RateLimitLayer>>>,
    default_limit: Duration,
    agent_limit: Duration,
}
```

- Middleware extracts agent ID from request: first checks `X-Agent-Id` header, then parses body JSON for `agent_id` field
- If found, looks up or creates a `RateLimitLayer` for that agent
- Applies the agent-specific limiter; falls back to global if no agent ID
- Rate-limited requests return `429 Too Many Requests` with `Retry-After` header

### Configuration

New env vars in `config.rs`:

| Variable | Default | Description |
|---|---|---|
| `AC_RATE_LIMIT_GLOBAL` | `1000/60s` | Global request rate limit |
| `AC_RATE_LIMIT_PER_AGENT` | `100/60s` | Per-agent request rate limit |

### Wiring in ac-server

`server.rs`: Apply `AgentRateLimit` middleware to the router, after TraceLayer but before routes.

### Test Coverage

- `tests/integration/adversarial.rs`: Send requests faster than the limit, verify 429 responses
- `tests/e2e.rs`: Add rate limit adversarial test for external server testing

### Limitations

Per-agent limits are in-memory (`HashMap`). They do not persist across server restarts and do not scale across multiple server instances. This is acceptable for v0.2 single-instance deployments. Redis-backed rate limiting is a v0.3 consideration.

## Deliverable 4: OpenAPI Validation

### Problem

The OpenAPI spec at `/api/openapi.json` is manually maintained. Nothing ensures the actual API responses match the declared schemas.

### Approach

Automated tests that validate responses against the OpenAPI schema.

### Implementation

New test file: `tests/integration/openapi.rs`

1. Build the server (via harness from Deliverable 1)
2. Parse the `ApiDoc::openapi()` output
3. For each documented endpoint:
   - Send a valid request
   - Validate the response status code matches one of the declared responses
   - If the response has a JSON body, validate it against the declared JSON Schema
4. For error endpoints, send invalid requests and validate error response schemas

### Dependencies Added

| Crate | Scope | Purpose |
|---|---|---|
| `openapiv3` | dev-dependency | Parse OpenAPI 3.0 spec |
| `jsonschema` | dev-dependency | Validate JSON bodies against JSON Schema |

### Justfile

New target: `just openapi-validate` — runs the OpenAPI conformance tests.

## Cargo.toml Changes

### New workspace dependencies

```toml
prometheus-client = "0.22"
testcontainers = "0.23"
testcontainers-modules = { version = "0.11", features = ["postgres"] }
openapiv3 = "2"
jsonschema = "0.28"
```

### New workspace member

```toml
members = [
    ...existing...
    "crates/ac-metrics",
]
```

## Implementation Order

1. **Integration test suite** — harness first, then per-endpoint tests
2. **Metrics & observability** — ac-metrics crate, then wire into ac-server
3. **Rate limiting** — middleware, then config, then tests
4. **OpenAPI validation** — tests against the harness-built server

Each deliverable is independently testable and can be reviewed separately.

## Deferred (Not v0.2)

- WebSocket support for real-time task status streaming (stretch goal)
- Admin dashboard HTMX (stretch goal)
- Redis-backed rate limiting (v0.3)
- Full test generation from OpenAPI spec (schemathesis-style property testing)
- Multi-instance rate limit coordination (v0.3)
