# v0.3 Serverless Adapters Design

**Date**: 2026-09-06
**Status**: Draft — awaiting review

## Goal

Enable the ac-server axum application to deploy on AWS Lambda, Cloudflare Workers, and Google Cloud Run Functions via a trait-based adapter layer, without modifying the existing `ac-server` crate.

## Problem Statement

The current `ac-server` binary runs on a long-lived Tokio runtime with direct TCP access to PostgreSQL. Three target serverless platforms have fundamentally different constraints:

| Platform | Runtime | DB Access | HTTP Model |
|----------|---------|-----------|------------|
| AWS Lambda | Tokio per invocation | RDS Proxy (PostgreSQL) | Event → handler → response |
| Cloudflare Workers | V8 isolate, no Tokio | D1 (SQLite) or HTTP gateway | Fetch event → Response |
| Cloud Run Functions | Minimal Tokio | Cloud SQL Unix socket | HTTP request → response |

A single codebase with platform-specific adapters isolates these differences cleanly.

## Architecture

### Core abstraction: `ac-runtime` crate

Two traits decouple the server from the platform:

#### `HttpAdapter`

```rust
pub trait HttpAdapter: Send + Sync + 'static {
    async fn serve(self, router: Router) -> Result<()>;
}
```

Converts platform-specific HTTP request/response types into axum `Router` calls. Each implementation starts the platform's event loop and dispatches requests through the shared router.

#### `DbAdapter`

```rust
pub trait DbAdapter: Send + Sync + 'static {
    type Pool: Clone + Send + Sync + 'static;
    async fn create_pool(connection_string: &str) -> Result<Self::Pool>;
}
```

Creates a database connection handle appropriate to the platform. For Lambda and Cloud Run, this is `sqlx::PgPool` with platform-specific connection configuration. For Workers, this may be an HTTP gateway client or D1 handle.

### Adapters

#### Lambda (`ac-runtime-lambda`)

- **HTTP**: Uses `lambda-web` crate to wrap the axum Router as a Lambda handler. Each Lambda invocation receives one HTTP request, calls `router.call()`, and returns the response.
- **DB**: Standard `PgPool` via `sqlx`, connecting through RDS Proxy endpoint. Connection string includes RDS Proxy parameters.
- **Lifecycle**: Cold start creates the pool; warm reuses it across invocations. Pool survives between requests within the same Lambda instance.

#### Cloudflare Workers (`ac-runtime-workers`)

- **HTTP**: Uses `workers-rs` (`worker` crate). The axum Router is converted to a `tower::Service` via `tower::MakeService`. Each `fetch` event calls the service and maps the response.
- **DB**: No TCP sockets available. The adapter uses an external PostgreSQL-over-HTTP gateway (e.g., `supavisor` HTTP mode, or a custom relay running alongside the Workers script via `workers-kv`). D1 is NOT used — the schema uses PostgreSQL-specific features (JSONB, UUID extensions) that don't translate cleanly to SQLite.
- **Lifecycle**: Workers are stateless per-request. No connection pooling — each request opens a fresh connection or uses the gateway.

#### Cloud Run (`ac-runtime-cloudrun`)

- **HTTP**: Minimal Tokio runtime, binds to `$PORT` env var (provided by Cloud Run at deploy time). Essentially identical to the current `ac-server` binary, just with configurable port.
- **DB**: `PgPool` via Unix socket to Cloud SQL (`/cloudsql/<instance-connection-name>`).
- **Lifecycle**: Long-lived process, identical to current server. The only difference is the port binding and DB connection string format.

### `ac-serverless` crate

A new binary crate with three `[[bin]]` targets, one per platform:

```
src/bin/ac-server-lambda.rs
src/bin/ac-server-workers.rs
src/bin/ac-server-cloudrun.rs
```

Each binary follows the same startup sequence:
1. Load config from `AC_` env vars (same precedence as current server)
2. Create `Metrics` and `RateLimiter` (in-memory, identical to `ac-server`)
3. Call `DbAdapter::create_pool()` with the platform connection string
4. Call `ac_server::create_app(pool, metrics, rate_limiter)` → `Router`
5. Call `HttpAdapter::serve(router)` to start the platform event loop

## What Does NOT Change

- `ac-server` crate — zero modifications
- `ac-db`, `ac-types`, and all feature crates — zero modifications
- Existing `cargo run -p ac-server` dev workflow
- Docker Compose local PostgreSQL setup
- Integration tests for the current server
- Migrations — all platforms run the same `migrations/*.sql` files at startup

## Error Handling

| Platform | Error Translation |
|----------|------------------|
| Lambda | Rust errors → Lambda 500 response; timeouts handled by Lambda invocation timeout |
| Workers | Uncaught panics → 500 with X-Request-Id; all handler calls wrapped in guards |
| Cloud Run | Identical to current axum error handling |

Database errors surface as `500` with `X-Request-Id`. The `/v1/ready` endpoint returns `503` if the pool cannot ping the database — works identically on all platforms.

## Testing

| Layer | Method |
|-------|--------|
| Unit (ac-server) | Unchanged — tests `create_app` directly via axum test client |
| Lambda | `cargo lambda test` with mock Lambda API Gateway events |
| Workers | `miniflare` local emulator for Workers runtime |
| Cloud Run | Same integration tests as current server (axum + PostgreSQL) |
| CI | Three new job entries (or one combined job with three stages) |

## Migration Order

1. Create `ac-runtime` crate with trait definitions
2. Implement Lambda adapter (simplest — closest to existing server)
3. Implement Cloud Run adapter (nearly identical to current server)
4. Implement Workers adapter (hardest — no TCP, requires D1 or HTTP gateway)
5. Create `ac-serverless` crate wiring adapters to `create_app`
6. Add CI jobs for the three new binaries
7. Update `docs/deployment.md` with serverless deployment instructions

## File Structure

```
crates/
  ac-runtime/
    src/
      lib.rs              # Trait definitions (HttpAdapter, DbAdapter)
      adapters/
        mod.rs
        lambda.rs
        workers.rs
        cloudrun.rs
    Cargo.toml
  ac-serverless/
    src/
      bin/
        ac-server-lambda.rs
        ac-server-workers.rs
        ac-server-cloudrun.rs
    Cargo.toml

Modified:
  Cargo.toml              # Add ac-runtime and ac-serverless workspace members
  docs/deployment.md      # Add serverless deployment section
```

## Risks and Mitigations

| Risk | Mitigation |
|------|-----------|
| Workers D1 schema mismatch | Start with a subset of tables; validate D1 compatibility of each migration |
| Lambda cold start latency | Pool creation on cold start adds ~2-5s; acceptable for most use cases |
| Three binaries to maintain | All three share `ac-runtime` traits and `ac-server` router; per-platform code is thin |
| `ac-runtime` becomes a dumping ground | Keep traits minimal; platform-specific logic stays in the adapter, not the trait |
