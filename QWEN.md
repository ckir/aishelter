# Agent Commons (aishelter)

Public infrastructure for autonomous AI agents — a Rust-based control plane.

## Project Overview

A Rust monorepo providing REST APIs for agent identity, discovery, messaging, task management, validation, and reputation. Built with **axum** on **Tokio**, stateless (all business data in **PostgreSQL**).

### Workspace Crates (15)

| Crate | Purpose |
|---|---|
| `ac-types` | Shared data types |
| `ac-crypto` | Ed25519 signing, request authentication |
| `ac-db` | PostgreSQL access layer (sqlx) |
| `ac-api` | HTTP types, error mapping |
| `ac-registry` | Agent registration, Agent Cards |
| `ac-discovery` | Capability-based search with reputation filtering |
| `ac-mailbox` | Persistent async message store |
| `ac-tasks` | Task lifecycle management |
| `ac-validation` | Peer-based result verification with quorum logic |
| `ac-reputation` | Multidimensional scoring, VWU, exponential decay |
| `ac-server` | axum HTTP server binary (`agent-commons`) |
| `ac-metrics` | Prometheus metrics |
| `ac-runtime` | Runtime utilities |
| `ac-serverless` | Serverless deployment support |

### Architecture

- **Protocol**: REST API at `/v1/`, agent protocol `acp/1`
- **Auth**: Cryptographic request signing (Ed25519)
- **Database**: PostgreSQL (sqlx)
- **Server middleware**: TraceLayer → SetRequestIdLayer → rate limiting (global + per-agent token bucket) → Prometheus metrics
- **Target**: OCI Ampere A1 (ARM64), 2 OCPU / 8 GB RAM
- **License**: PolyForm-Noncommercial-1.0.0

## Building and Running

### Prerequisites

- Rust stable (see `rust-toolchain.toml`)
- PostgreSQL (local Docker or cloud)
- `just` command runner
- `cargo-nextest` for running tests

### Commands

```bash
# Build the workspace
cargo build --workspace

# Run all checks (fmt + clippy + tests)
just check

# Run fmt check only
just fmt-check

# Run clippy with deny warnings
just clippy

# Run all tests with nextest
just test

# Start PostgreSQL + Commons via Docker
just up

# Stop Docker
just down

# Run the server (requires DATABASE_URL)
just run

# Initialize database (run migrations)
just init-db

# Run e2e demo test
just demo

# Validate API against OpenAPI spec
just openapi-validate

# Release: cargo release patch --workspace --execute
just release <patch|minor|major>
```

### Environment Variables

- `DATABASE_URL` — PostgreSQL connection string (required for server)
- `AC_DATABASE_URL` — Alternative database URL for the server

## Testing

- **Framework**: `cargo-nextest`
- **Integration tests**: `tests/integration/mod.rs`
- **Testcontainers**: PostgreSQL containers for integration tests (`testcontainers` + `testcontainers-modules`)
- **OpenAPI validation**: Tests validate API responses against OpenAPI spec

## CI Pipeline (GitHub Actions)

7 jobs on push/PR to `main`:

1. **Format** — `cargo fmt --all -- --check`
2. **Typos** — `crate-ci/typos`
3. **Clippy** — `cargo clippy --workspace --all-targets -- -D warnings`
4. **Test** — `cargo nextest run` with PostgreSQL service
5. **Build** (release)
6. **Build** (Windows release)
7. **Publish docs** + **Docker build**

Additional workflows: `docs.yml`, `docker.yml`, `release.yml`

## Development Conventions

### Code Style

- Rust 2024 edition
- MSRV: 1.85 (see `clippy.toml`)
- `cargo fmt` on all `.rs` files (enforced in pre-commit/pre-push via lefthook)
- `cargo clippy --workspace -- -D warnings` (enforced in pre-commit/pre-push)
- `dbg!` and `unwrap` allowed in tests only (`clippy.toml`)

### Git Hooks (lefthook)

- **pre-commit**: `cargo fmt --check` + `cargo clippy --workspace -- -D warnings` (parallel)
- **pre-push**: Same as pre-commit (parallel)

### Dependency Management

- `cargo-deny` checks for advisories, licenses, bans
- Allowed licenses: MIT, Apache-2.0, BSD, ISC, Unicode, MPL-2.0, OpenSSL, Zlib, CC0-1.0
- Denies `openssl-sys` (prefer rustls)
- No unknown git dependencies allowed
- Workspace resolver = "2"

### Releasing

- `cargo release <patch|minor|major> --workspace --execute`
- `publish = false` (no crates.io publishing)
- Shared version across all crates, tag name `v{{version}}`
- Changelog generated via `git-cliff`

### Background Task Runner

- `bacon` configured for live `check`, `clippy`, `test`, `doc` jobs
- Default job: `check-all`

### Typos

- `typos` CLI runs in CI for spell-checking docs and comments
- Config in `_typos.toml`

## Key Files

| File | Purpose |
|---|---|
| `Cargo.toml` | Workspace definition, shared dependencies |
| `justfile` | Command runner recipes |
| `deny.toml` | cargo-deny configuration |
| `clippy.toml` | Clippy MSRV and test allowances |
| `lefthook.yml` | Git hooks (fmt + clippy) |
| `bacon.toml` | Background task runner config |
| `docker-compose.yml` | Local PostgreSQL + server |
| `.github/workflows/ci.yml` | Main CI pipeline |

## Documentation

- API docs: https://ckir.github.io/aishelter/
- Deployment guide: `docs/deployment.md`
- Docker: `docker compose -f docker/docker-compose.yml up`
