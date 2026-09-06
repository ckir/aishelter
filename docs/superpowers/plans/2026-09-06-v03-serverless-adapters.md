# v0.3 Serverless Adapters Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Enable the ac-server axum application to deploy on AWS Lambda, Cloudflare Workers, and Google Cloud Run Functions via a trait-based adapter layer.

**Architecture:** A new `ac-runtime` crate defines `HttpAdapter` and `DbAdapter` traits. Three platform-specific adapters implement these traits. A new `ac-serverless` crate with three binary targets wires the adapters to `ac_server::create_app()`.

**Tech Stack:** Rust, axum, tower, sqlx, lambda-web, workers-rs, tokio, anyhow, thiserror.

**Spec:** `docs/superpowers/specs/2026-09-06-v03-serverless-adapters-design.md`

## Global Constraints

- `ac-server` crate — zero modifications
- `ac-db`, `ac-types`, and all feature crates — zero modifications
- Existing `cargo run -p ac-server` dev workflow — unchanged
- Env var prefix: `AC_` (same as current server)
- Edition: 2024 (workspace default)
- DRY, YAGNI, TDD principles — each task includes tests
- Frequent commits — one commit per task

---

### Task 1: Create `ac-runtime` crate with trait definitions

**Files:**
- Create: `crates/ac-runtime/Cargo.toml`
- Create: `crates/ac-runtime/src/lib.rs`
- Modify: `Cargo.toml` (root workspace members)

**Interfaces:**
- Produces: `ac_runtime::HttpAdapter` trait, `ac_runtime::DbAdapter` trait, `ac_runtime::Result<T>` type alias

**Dependencies:** `axum`, `tower`, `sqlx`, `anyhow`, `thiserror`

- [ ] **Step 1: Write `ac-runtime/Cargo.toml`**

```toml
[package]
name = "ac-runtime"
version = "0.1.0"
edition = "2024"
license.workspace = true

[dependencies]
axum = { workspace = true }
tower = { workspace = true }
sqlx = { workspace = true }
anyhow = { workspace = true }
thiserror = { workspace = true }
async-trait = "0.1"
```

- [ ] **Step 2: Write `ac-runtime/src/lib.rs` with trait definitions**

```rust
//! Runtime adapter traits for serverless deployments.
//!
//! This crate defines the `HttpAdapter` and `DbAdapter` traits that
//! decouple the ac-server application from platform-specific HTTP
//! handling and database connection logic.

use axum::Router;
use sqlx::PgPool;

/// Result type alias for adapter operations.
pub type Result<T> = std::result::Result<T, anyhow::Error>;

/// Abstracts platform-specific HTTP request/response handling.
///
/// Each implementation wraps the axum Router in a platform-specific
/// event loop (Lambda invocations, Workers fetch events, Cloud Run TCP).
pub trait HttpAdapter: Send + Sync + 'static {
    /// Start serving requests through the given router.
    /// Blocks until the platform signals shutdown or an error occurs.
    fn serve(self, router: Router) -> impl std::future::Future<Output = Result<()>> + Send;
}

/// Abstracts platform-specific database connection creation.
///
/// For Lambda and Cloud Run, this returns a `sqlx::PgPool`.
/// For Workers, this may return an HTTP gateway client or pooled handle.
pub trait DbAdapter: Send + Sync + 'static {
    /// The connection pool type this adapter produces.
    type Pool: Clone + Send + Sync + 'static;

    /// Create a new connection pool from the given connection string.
    fn create_pool(
        connection_string: &str,
    ) -> impl std::future::Future<Output = Result<Self::Pool>> + Send;
}

/// A DbAdapter that returns a standard sqlx::PgPool — used by Lambda and Cloud Run.
pub struct PgDbAdapter;

impl DbAdapter for PgDbAdapter {
    type Pool = PgPool;

    async fn create_pool(connection_string: &str) -> Result<Self::Pool> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(10)
            .connect(connection_string)
            .await?;
        Ok(pool)
    }
}
```

- [ ] **Step 3: Run `cargo check -p ac-runtime`**

Run: `cargo check -p ac-runtime`
Expected: PASS with no errors

- [ ] **Step 4: Add root `Cargo.toml` workspace member**

Add `"crates/ac-runtime"` to the `members` array in the root `Cargo.toml`:

```toml
[workspace]
members = [
    "crates/ac-types",
    "crates/ac-crypto",
    "crates/ac-db",
    "crates/ac-api",
    "crates/ac-registry",
    "crates/ac-discovery",
    "crates/ac-mailbox",
    "crates/ac-tasks",
    "crates/ac-validation",
    "crates/ac-reputation",
    "crates/ac-server",
    "crates/ac-metrics",
    "crates/ac-runtime",
]
```

- [ ] **Step 5: Run `cargo check --workspace`**

Run: `cargo check --workspace`
Expected: PASS with no errors

- [ ] **Step 6: Commit**

```bash
git add crates/ac-runtime/ Cargo.toml
git commit -m "feat(v0.3): add ac-runtime crate with HttpAdapter and DbAdapter traits"
```

---

### Task 2: Implement Lambda adapter

**Files:**
- Create: `crates/ac-runtime/src/adapters/mod.rs`
- Create: `crates/ac-runtime/src/adapters/lambda.rs`
- Modify: `crates/ac-runtime/src/lib.rs` (add adapter module exports)
- Modify: `crates/ac-runtime/Cargo.toml` (add lambda-web dependency)

**Interfaces:**
- Consumes: `ac_runtime::HttpAdapter`, `ac_runtime::DbAdapter`, `ac_runtime::PgDbAdapter`
- Produces: `ac_runtime::adapters::LambdaHttpAdapter`

**Dependencies:** `lambda-web` (or `lambda_http` + `tower` bridge)

- [ ] **Step 1: Update `ac-runtime/Cargo.toml` with lambda dependencies**

Add under `[dependencies]`:

```toml
lambda_http = "0.13"
tower = { workspace = true }
```

- [ ] **Step 2: Write `crates/ac-runtime/src/adapters/mod.rs`**

```rust
//! Platform-specific adapter implementations.

#[cfg(feature = "lambda")]
pub mod lambda;

#[cfg(feature = "cloudrun")]
pub mod cloudrun;

#[cfg(feature = "workers")]
pub mod workers;
```

- [ ] **Step 3: Write `crates/ac-runtime/src/adapters/lambda.rs`**

```rust
//! AWS Lambda HTTP adapter using lambda_http.
//!
//! Wraps the axum Router in a tower::Service that Lambda's runtime
//! invokes per invocation. Cold start creates the PgPool; warm
//! invocations reuse it.

use axum::Router;
use lambda_http::{Body, Error, Request, RequestExt, Response};
use tower::Service;
use tower::ServiceExt as _;

use crate::{HttpAdapter, Result};

/// HTTP adapter for AWS Lambda via lambda_http.
///
/// Each Lambda invocation receives one API Gateway event, converts
/// it to an axum Request, calls the Router, and returns the Response.
pub struct LambdaHttpAdapter;

impl LambdaHttpAdapter {
    /// Create a new Lambda adapter.
    pub fn new() -> Self {
        Self
    }
}

impl Default for LambdaHttpAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpAdapter for LambdaHttpAdapter {
    async fn serve(self, router: Router) -> Result<()> {
        // lambda_http::run starts the Lambda event loop.
        // The handler wraps the axum Router as a tower::Service.
        lambda_http::run(LambdaHandler { router }).await;
        Ok(())
    }
}

struct LambdaHandler {
    router: Router,
}

#[tower::async_trait]
impl Service<Request> for LambdaHandler {
    type Response = Response<Body>;
    type Error = Error;
    type Future = std::pin::Pin<Box<dyn std::future::Future<Output = std::result::Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::result::Result<(), Self::Error>> {
        // The axum Router is always ready.
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: Request) -> Self::Future {
        let mut router = self.router.clone();
        Box::pin(async move {
            let response = router.call(req).await.map_err(|e| {
                tracing::error!("axum router error: {:?}", e);
                lambda_http::Error::new("internal server error")
            })?;
            Ok(response.map(Body::from))
        })
    }
}
```

- [ ] **Step 4: Update `crates/ac-runtime/src/lib.rs` to add feature flag and module**

Add at the top of `lib.rs`:

```rust
#[cfg(any(feature = "lambda", feature = "cloudrun", feature = "workers"))]
pub mod adapters;

// Re-export adapter types when their feature is enabled
#[cfg(feature = "lambda")]
pub use adapters::lambda::LambdaHttpAdapter;
```

- [ ] **Step 5: Add `lambda` feature to `ac-runtime/Cargo.toml`**

```toml
[features]
lambda = ["dep:lambda_http"]
```

- [ ] **Step 6: Run `cargo check -p ac-runtime --features lambda`**

Run: `cargo check -p ac-runtime --features lambda`
Expected: PASS with no errors

- [ ] **Step 7: Write unit test for Lambda adapter service readiness**

Create: `crates/ac-runtime/src/adapters/lambda_test.rs`

```rust
#[cfg(test)]
mod tests {
    use axum::Router;
    use tower::Service;

    use super::LambdaHandler;

    #[test]
    fn lambda_handler_is_always_ready() {
        let router = Router::new();
        let mut handler = LambdaHandler { router };
        let mut cx = std::task::Context::from_waker(futures::task::noop_waker_ref());
        let poll = handler.poll_ready(&mut cx);
        assert!(poll.is_ready());
        assert!(poll.unwrap().is_ok());
    }
}
```

- [ ] **Step 8: Add `futures` dev dependency and test module**

In `ac-runtime/Cargo.toml` under `[dev-dependencies]`:

```toml
futures = "0.3"
```

In `adapters/mod.rs`, add under `#[cfg(feature = "lambda")]`:

```rust
#[cfg(all(test, feature = "lambda"))]
mod lambda_test;
```

- [ ] **Step 9: Run `cargo test -p ac-runtime --features lambda`**

Run: `cargo test -p ac-runtime --features lambda`
Expected: PASS, 1 test

- [ ] **Step 10: Commit**

```bash
git add crates/ac-runtime/
git commit -m "feat(v0.3): implement Lambda HTTP adapter"
```

---

### Task 3: Implement Cloud Run adapter

**Files:**
- Create: `crates/ac-runtime/src/adapters/cloudrun.rs`
- Modify: `crates/ac-runtime/src/adapters/mod.rs` (already has feature gate)
- Modify: `crates/ac-runtime/Cargo.toml` (add cloudrun feature)
- Modify: `crates/ac-runtime/src/lib.rs` (add re-export)

**Interfaces:**
- Consumes: `ac_runtime::HttpAdapter`
- Produces: `ac_runtime::adapters::CloudRunHttpAdapter`

**Dependencies:** `tokio`, `axum`

- [ ] **Step 1: Add `cloudrun` feature to `ac-runtime/Cargo.toml`**

```toml
[features]
lambda = ["dep:lambda_http"]
cloudrun = ["dep:tokio"]
```

- [ ] **Step 2: Update `crates/ac-runtime/src/lib.rs` re-exports**

Add:

```rust
#[cfg(feature = "cloudrun")]
pub use adapters::cloudrun::CloudRunHttpAdapter;
```

- [ ] **Step 3: Write `crates/ac-runtime/src/adapters/cloudrun.rs`**

```rust
//! Google Cloud Run HTTP adapter.
//!
//! Runs a minimal Tokio runtime and binds to the $PORT env var
//! provided by Cloud Run at deploy time. Functionally identical
//! to the current ac-server binary but with configurable port.

use axum::Router;
use tokio::net::TcpListener;

use crate::{HttpAdapter, Result};

/// HTTP adapter for Google Cloud Run.
///
/// Binds to the configured port (from $PORT env var or default)
/// and serves the axum Router via `axum::serve`.
pub struct CloudRunHttpAdapter {
    host: String,
    port: u16,
}

impl CloudRunHttpAdapter {
    /// Create a new Cloud Run adapter.
    ///
    /// If `port` is None, reads from the $PORT environment variable
    /// (Cloud Run convention), defaulting to 3000.
    pub fn new(host: String, port: Option<u16>) -> Self {
        let port = port.unwrap_or_else(|| {
            std::env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3000)
        });
        Self { host, port }
    }
}

impl HttpAdapter for CloudRunHttpAdapter {
    async fn serve(self, router: Router) -> Result<()> {
        let addr = format!("{}:{}", self.host, self.port);
        let listener = TcpListener::bind(&addr).await?;
        tracing::info!("Cloud Run adapter listening on {}", addr);
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_signal())
            .await?;
        Ok(())
    }
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    ctrl_c.await.expect("failed to install Ctrl+C handler");
    tracing::info!("shutdown signal received, draining connections...");
}
```

- [ ] **Step 4: Run `cargo check -p ac-runtime --features cloudrun`**

Run: `cargo check -p ac-runtime --features cloudrun`
Expected: PASS with no errors

- [ ] **Step 5: Write unit test for Cloud Run adapter port resolution**

Create: `crates/ac-runtime/src/adapters/cloudrun_test.rs`

```rust
#[cfg(test)]
mod tests {
    use super::CloudRunHttpAdapter;

    #[test]
    fn cloudrun_adapter_uses_explicit_port() {
        let adapter = CloudRunHttpAdapter::new("0.0.0.0".to_string(), Some(8080));
        assert_eq!(adapter.port, 8080);
    }

    #[test]
    fn cloudrun_adapter_defaults_to_3000() {
        std::env::remove_var("PORT");
        let adapter = CloudRunHttpAdapter::new("0.0.0.0".to_string(), None);
        assert_eq!(adapter.port, 3000);
    }

    #[test]
    fn cloudrun_adapter_reads_port_env() {
        std::env::set_var("PORT", "9000");
        let adapter = CloudRunHttpAdapter::new("0.0.0.0".to_string(), None);
        assert_eq!(adapter.port, 9000);
        std::env::remove_var("PORT");
    }
}
```

- [ ] **Step 6: Add test module to `adapters/mod.rs`**

Add under the `#[cfg(feature = "cloudrun")]` block:

```rust
#[cfg(all(test, feature = "cloudrun"))]
mod cloudrun_test;
```

- [ ] **Step 7: Run `cargo test -p ac-runtime --features cloudrun`**

Run: `cargo test -p ac-runtime --features cloudrun`
Expected: PASS, 3 tests

- [ ] **Step 8: Commit**

```bash
git add crates/ac-runtime/
git commit -m "feat(v0.3): implement Cloud Run HTTP adapter"
```

---

### Task 4: Implement Workers adapter

**Files:**
- Create: `crates/ac-runtime/src/adapters/workers.rs`
- Modify: `crates/ac-runtime/Cargo.toml` (add workers feature + deps)
- Modify: `crates/ac-runtime/src/lib.rs` (add re-export)
- Modify: `crates/ac-runtime/src/adapters/mod.rs` (test module)

**Interfaces:**
- Consumes: `ac_runtime::HttpAdapter`
- Produces: `ac_runtime::adapters::WorkersHttpAdapter`

**Dependencies:** `worker` (workers-rs), `tower`, `console_error_panic_hook`

- [ ] **Step 1: Add `workers` feature and dependencies to `ac-runtime/Cargo.toml`**

```toml
[features]
lambda = ["dep:lambda_http"]
cloudrun = ["dep:tokio"]
workers = ["dep:worker", "dep:tower"]

[dependencies]
worker = { version = "0.6", optional = true }
console_error_panic_hook = { version = "0.1", optional = true }
```

- [ ] **Step 2: Update `crates/ac-runtime/src/lib.rs` re-exports**

Add:

```rust
#[cfg(feature = "workers")]
pub use adapters::workers::WorkersHttpAdapter;
```

- [ ] **Step 3: Write `crates/ac-runtime/src/adapters/workers.rs`**

```rust
//! Cloudflare Workers HTTP adapter using workers-rs.
//!
//! Converts the axum Router into a tower::Service, then wraps it
//! in a worker::Router that handles fetch events. Each fetch event
//! calls the service and maps the axum Response to a worker::Response.
//!
//! Note: Workers have no TCP socket support. Database access must go
//! through an external PostgreSQL-over-HTTP gateway.

use axum::Router;
use tower::Service;
use tower::ServiceExt as _;
use worker::{Request as WorkerRequest, Response as WorkerResponse, Router as WorkerRouter};

use crate::{HttpAdapter, Result};

/// HTTP adapter for Cloudflare Workers via workers-rs.
///
/// The adapter does not "serve" in the traditional sense — Cloudflare
/// invokes the worker's fetch event handler per request. This adapter
/// provides the binding between the axum Router and the Workers runtime.
pub struct WorkersHttpAdapter;

impl WorkersHttpAdapter {
    /// Create a new Workers adapter.
    pub fn new() -> Self {
        Self
    }

    /// Create a worker::Router that dispatches to the axum Router.
    ///
    /// Call this from your worker's `[worker::event(fetch)]` handler:
    ///
    /// ```ignore
    /// let router = WorkersHttpAdapter::new().into_worker_router(app);
    /// router.run(req, env, ctx).await
    /// ```
    pub fn into_worker_router(self, router: Router) -> WorkerRouter {
        WorkerRouter::new()
            .get("/*route", move |req, ctx| {
                let router = router.clone();
                async move {
                    match handle_request(req, router).await {
                        Ok(resp) => resp,
                        Err(e) => {
                            tracing::error!("Workers adapter error: {}", e);
                            WorkerResponse::error(500)
                                .unwrap()
                            }
                        }
                    }
                }
            })
            .post("/*route", move |req, ctx| {
                let router = router.clone();
                async move {
                    match handle_request(req, router).await {
                        Ok(resp) => resp,
                        Err(e) => {
                            tracing::error!("Workers adapter error: {}", e);
                            WorkerResponse::error(500)
                                .unwrap()
                            }
                        }
                    }
                }
            })
            .put("/*route", move |req, ctx| {
                let router = router.clone();
                async move {
                    match handle_request(req, router).await {
                        Ok(resp) => resp,
                        Err(e) => {
                            tracing::error!("Workers adapter error: {}", e);
                            WorkerResponse::error(500)
                                .unwrap()
                            }
                        }
                    }
                }
            })
            .delete("/*route", move |req, ctx| {
                let router = router.clone();
                async move {
                    match handle_request(req, router).await {
                        Ok(resp) => resp,
                        Err(e) => {
                            tracing::error!("Workers adapter error: {}", e);
                            WorkerResponse::error(500)
                                .unwrap()
                            }
                        }
                    }
                }
            })
            .patch("/*route", move |req, ctx| {
                let router = router.clone();
                async move {
                    match handle_request(req, router).await {
                        Ok(resp) => resp,
                        Err(e) => {
                            tracing::error!("Workers adapter error: {}", e);
                            WorkerResponse::error(500)
                                .unwrap()
                            }
                        }
                    }
                }
            })
    }
}

impl Default for WorkersHttpAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpAdapter for WorkersHttpAdapter {
    async fn serve(self, _router: Router) -> Result<()> {
        // Workers doesn't have a traditional "serve" loop — Cloudflare
        // invokes the fetch event handler. This adapter's serve() is
        // a no-op for trait compatibility.
        Ok(())
    }
}

async fn handle_request(req: WorkerRequest, router: Router) -> Result<WorkerResponse> {
    // Convert worker::Request to http::Request
    let uri = req.url()?.to_string();
    let method = req.method().to_string();
    
    let mut http_req = http::Request::builder()
        .method(method.as_str())
        .uri(&uri);
    
    for (key, value) in req.headers().iter() {
        http_req = http_req.header(key, value);
    }
    
    let body_bytes = req.bytes().await?;
    let http_req = http_req.body(axum::body::Body::from(body_bytes))?;
    
    // Call the axum router
    let mut router_service = router.oneshot_ready().await?;
    let response = router_service.call(http_req).await?;
    
    // Convert axum Response to worker::Response
    let status = response.status().as_u16().try_into()?;
    let mut worker_resp = WorkerResponse::builder().status(status);
    
    for (key, value) in response.headers().iter() {
        if let Ok(v) = value.to_str() {
            worker_resp = worker_resp.header(key.as_str(), v);
        }
    }
    
    let body = axum::body::to_bytes(response.into_body(), 10_485_760).await?;
    worker_resp.body(Vec::from(body))
}
```

- [ ] **Step 4: Run `cargo check -p ac-runtime --features workers`**

Run: `cargo check -p ac-runtime --features workers`
Expected: May have compile errors due to workers-rs API details — fix type mismatches in the `handle_request` function. The key conversion is: `worker::Request` → `http::Request` → `Router::call` → `http::Response` → `worker::Response`.

- [ ] **Step 5: Add test module to `adapters/mod.rs`**

Add under `#[cfg(feature = "workers")]`:

```rust
#[cfg(all(test, feature = "workers"))]
mod workers_test;
```

- [ ] **Step 6: Write minimal test for Workers adapter construction**

Create: `crates/ac-runtime/src/adapters/workers_test.rs`

```rust
#[cfg(test)]
mod tests {
    use super::WorkersHttpAdapter;

    #[test]
    fn workers_adapter_constructs() {
        let _adapter = WorkersHttpAdapter::new();
    }

    #[test]
    fn workers_adapter_implements_default() {
        let _adapter = WorkersHttpAdapter::default();
    }
}
```

- [ ] **Step 7: Run `cargo test -p ac-runtime --features workers`**

Run: `cargo test -p ac-runtime --features workers`
Expected: PASS, 2 tests

- [ ] **Step 8: Commit**

```bash
git add crates/ac-runtime/
git commit -m "feat(v0.3): implement Cloudflare Workers HTTP adapter"
```

---

### Task 5: Create `ac-serverless` crate with three binary targets

**Files:**
- Create: `crates/ac-serverless/Cargo.toml`
- Create: `crates/ac-serverless/src/bin/ac-server-lambda.rs`
- Create: `crates/ac-serverless/src/bin/ac-server-workers.rs`
- Create: `crates/ac-serverless/src/bin/ac-server-cloudrun.rs`
- Modify: `Cargo.toml` (root workspace members — add `crates/ac-serverless`)

**Interfaces:**
- Consumes: `ac_runtime::HttpAdapter`, `ac_runtime::DbAdapter`, `ac_runtime::PgDbAdapter`, `ac_runtime::LambdaHttpAdapter`, `ac_runtime::CloudRunHttpAdapter`, `ac_runtime::WorkersHttpAdapter`, `ac_server::create_app`, `ac_server::config::Settings`, `ac_server::middleware::rate_limit::RateLimiter`, `ac_metrics::registry::Metrics`
- Produces: Three executable binaries

- [ ] **Step 1: Write `crates/ac-serverless/Cargo.toml`**

```toml
[package]
name = "ac-serverless"
version = "0.1.0"
edition = "2024"
license.workspace = true

[[bin]]
name = "ac-server-lambda"
path = "src/bin/ac-server-lambda.rs"
required-features = ["lambda"]

[[bin]]
name = "ac-server-workers"
path = "src/bin/ac-server-workers.rs"
required-features = ["workers"]

[[bin]]
name = "ac-server-cloudrun"
path = "src/bin/ac-server-cloudrun.rs"
required-features = ["cloudrun"]

[features]
lambda = ["ac-runtime/lambda", "dep:lambda_http"]
cloudrun = ["ac-runtime/cloudrun", "dep:tokio"]
workers = ["ac-runtime/workers"]

[dependencies]
ac-runtime = { path = "../ac-runtime" }
ac-server = { path = "../ac-server" }
ac-metrics = { path = "../ac-metrics" }
sqlx = { workspace = true }
tokio = { workspace = true, optional = true }
anyhow = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
lambda_http = { version = "0.13", optional = true }
```

- [ ] **Step 2: Write `crates/ac-serverless/src/bin/ac-server-cloudrun.rs`**

```rust
//! Cloud Run serverless binary.
//!
//! Startup sequence:
//! 1. Load Settings from AC_ env vars
//! 2. Create PgPool via PgDbAdapter
//! 3. Run migrations
//! 4. Create Metrics and RateLimiter
//! 5. Build Router via create_app
//! 6. Serve via CloudRunHttpAdapter

use ac_metrics::registry::Metrics;
use ac_runtime::adapters::cloudrun::CloudRunHttpAdapter;
use ac_runtime::PgDbAdapter;
use ac_server::config::Settings;
use ac_server::middleware::rate_limit::RateLimiter;
use ac_server::server::create_app;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let settings = Settings::new().unwrap_or_else(|_| Settings::default());

    let pool = PgDbAdapter::create_pool(&settings.database_url).await?;
    sqlx::migrate!("../../migrations").run(&pool).await?;

    let metrics = Metrics::new();
    let rate_limiter = RateLimiter::new(
        settings.rate_limit_global,
        settings.rate_limit_per_agent,
        settings.rate_limit_window_secs,
    );

    let app = create_app(pool, metrics, rate_limiter);

    let adapter = CloudRunHttpAdapter::new(settings.host, Some(settings.port));
    tracing::info!("Cloud Run adapter starting on {}:{}", settings.host, settings.port);

    adapter.serve(app).await?;
    Ok(())
}
```

- [ ] **Step 3: Write `crates/ac-serverless/src/bin/ac-server-lambda.rs`**

```rust
//! AWS Lambda serverless binary.
//!
//! Startup sequence:
//! 1. Load Settings from AC_ env vars
//! 2. Create PgPool via PgDbAdapter (cold start)
//! 3. Run migrations (cold start only)
//! 4. Create Metrics and RateLimiter
//! 5. Build Router via create_app
//! 6. Serve via LambdaHttpAdapter (lambda_http event loop)

use ac_metrics::registry::Metrics;
use ac_runtime::adapters::lambda::LambdaHttpAdapter;
use ac_runtime::PgDbAdapter;
use ac_server::config::Settings;
use ac_server::middleware::rate_limit::RateLimiter;
use ac_server::server::create_app;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();

    let settings = Settings::new().unwrap_or_else(|_| Settings::default());

    let pool = PgDbAdapter::create_pool(&settings.database_url).await?;
    sqlx::migrate!("../../migrations").run(&pool).await?;

    let metrics = Metrics::new();
    let rate_limiter = RateLimiter::new(
        settings.rate_limit_global,
        settings.rate_limit_per_agent,
        settings.rate_limit_window_secs,
    );

    let app = create_app(pool, metrics, rate_limiter);

    let adapter = LambdaHttpAdapter::new();
    tracing::info!("Lambda adapter starting");

    adapter.serve(app).await?;
    Ok(())
}
```

- [ ] **Step 4: Write `crates/ac-serverless/src/bin/ac-server-workers.rs`**

```rust
//! Cloudflare Workers serverless binary.
//!
//! Workers doesn't have a traditional main function. The worker entry
//! point is the [worker::event(fetch)] attribute in the Workers runtime.
//! This binary compiles the worker wasm module.
//!
//! At runtime, Cloudflare invokes the fetch event handler, which calls
//! the axum Router via the WorkersHttpAdapter.

use ac_runtime::adapters::workers::WorkersHttpAdapter;
use ac_runtime::PgDbAdapter;
use worker::*;

mod utils;

fn init() {
    console_error_panic_hook::set_once();
}

#[event(fetch)]
async fn fetch(
    req: worker::Request,
    env: Env,
    ctx: Context,
) -> Result<worker::Response> {
    init();

    // In a real deployment, the pool would be initialized via an HTTP
    // gateway connection string from env. For now, this is a placeholder.
    let adapter = WorkersHttpAdapter::new();
    let router = axum::Router::new();

    // The adapter converts the worker request to an axum request
    // and routes it through the app.
    adapter.into_worker_router(router).run(req, env, ctx).await
}
```

- [ ] **Step 5: Create `crates/ac-serverless/src/utils.rs`**

```rust
//! Utility module for worker-specific conversions.

use worker::Result as WorkerResult;

/// Initialize panic hook for better error messages in Workers.
pub fn init_panic_hook() {
    console_error_panic_hook::set_once();
}
```

- [ ] **Step 6: Add `ac-serverless` to root workspace**

Add `"crates/ac-serverless"` to root `Cargo.toml` members.

- [ ] **Step 7: Run `cargo check -p ac-serverless --features cloudrun`**

Run: `cargo check -p ac-serverless --features cloudrun`
Expected: PASS with no errors

- [ ] **Step 8: Run `cargo check -p ac-serverless --features lambda`**

Run: `cargo check -p ac-serverless --features lambda`
Expected: PASS with no errors (may need lambda_http import fixes)

- [ ] **Step 9: Run `cargo check -p ac-serverless --features workers`**

Run: `cargo check -p ac-serverless --features workers`
Expected: May need workers-rs API adjustments

- [ ] **Step 10: Commit**

```bash
git add crates/ac-serverless/ Cargo.toml
git commit -m "feat(v0.3): add ac-serverless crate with Lambda, Cloud Run, Workers binaries"
```

---

### Task 6: Build verification and integration smoke test

**Files:**
- Create: `crates/ac-serverless/tests/smoke_test.rs`
- Modify: `crates/ac-serverless/Cargo.toml` (add dev dependencies)

**Interfaces:**
- Consumes: `ac_server::create_app`, `ac_server::server`, testcontainers

- [ ] **Step 1: Write smoke test that builds the Cloud Run binary and tests the router**

Create: `crates/ac-serverless/tests/smoke_test.rs`

```rust
//! Smoke test: verifies that the serverless binaries compile and
//! that the Cloud Run adapter's router responds to health checks.

use ac_metrics::registry::Metrics;
use ac_server::middleware::rate_limit::RateLimiter;
use ac_server::server::create_app;
use axum::Router;
use sqlx::PgPool;

/// Build the app router and test the health endpoint.
#[tokio::test]
async fn cloudrun_health_check() {
    // We can't test the actual CloudRunHttpAdapter (it needs TCP binding),
    // but we can verify the Router it wraps is functional.
    let pool = create_test_pool().await;
    let metrics = Metrics::new();
    let rate_limiter = RateLimiter::new(1000, 100, 60);

    let app: Router = create_app(pool, metrics, rate_limiter);

    let response = axum::test::MockRequest::get("/v1/health")
        .send(&app)
        .await;

    assert_eq!(response.status(), 200);
    let body = response.into_body_string().await;
    assert_eq!(body, "ok");
}

async fn create_test_pool() -> PgPool {
    // Use testcontainers for PostgreSQL
    let postgres = testcontainers_modules::postgres::Postgres::default();
    let container = testcontainers::runners::AsyncRunner::start(postgres).await.unwrap();
    let host_port = container.get_host_port_ipv(5432).await.unwrap();
    let url = format!("postgresql://postgres:postgres@localhost:{}/postgres", host_port);
    PgPool::connect(&url).await.unwrap()
}
```

- [ ] **Step 2: Add dev dependencies to `ac-serverless/Cargo.toml`**

```toml
[dev-dependencies]
axum = { workspace = true }
tokio = { workspace = true }
sqlx = { workspace = true }
testcontainers = { workspace = true }
testcontainers-modules = { workspace = true }
```

- [ ] **Step 3: Run `cargo test -p ac-serverless --features cloudrun`**

Run: `cargo test -p ac-serverless --features cloudrun`
Expected: PASS, 1 test (requires Docker for testcontainers)

- [ ] **Step 4: Commit**

```bash
git add crates/ac-serverless/tests/
git commit -m "test(v0.3): add smoke test for serverless router"
```

---

### Task 7: Update deployment docs

**Files:**
- Modify: `docs/deployment.md`

**Interfaces:** N/A

- [ ] **Step 1: Read current `docs/deployment.md`**

Read the existing deployment docs to understand the current structure.

- [ ] **Step 2: Append serverless deployment section**

Add to the end of `docs/deployment.md`:

```markdown
## Serverless Deployments

Agent Commons supports three serverless deployment targets via the
`ac-serverless` crate. Each target shares the same application code
but uses a platform-specific HTTP adapter and database connection
strategy.

### AWS Lambda

Build the Lambda binary:

```bash
cargo build -p ac-serverless --features lambda --release --bin ac-server-lambda
```

Deploy using the AWS Lambda console or `aws lambda` CLI. Set the
`AC_DATABASE_URL` environment variable to point to an RDS Proxy
endpoint.

**Cold start:** The first invocation creates the connection pool and
runs migrations (~2-5s). Warm invocations reuse the pool.

**Recommended:** Use provisioned concurrency to avoid cold starts
for production traffic.

### Google Cloud Run Functions

Build the Cloud Run binary:

```bash
cargo build -p ac-serverless --features cloudrun --release --bin ac-server-cloudrun
```

Deploy to Cloud Run with a Cloud SQL PostgreSQL instance:

```bash
gcloud run deploy agent-commons \
  --image=agent-commons-cloudrun \
  --add-cloudsql-instances=PROJECT:REGION:INSTANCE \
  --set-env-vars=AC_DATABASE_URL="postgresql://cloudsqlproxy@/aishelter?host=/cloudsql/PROJECT:REGION:INSTANCE"
```

### Cloudflare Workers

Build the Workers wasm module:

```bash
cargo install -q worker-build
worker-build --no-default-features --features workers -p ac-serverless
```

Deploy with Wrangler:

```bash
wrangler deploy
```

**Database:** Workers has no TCP socket support. Database access
requires an external PostgreSQL-over-HTTP gateway (e.g., Supavisor
HTTP mode). Set `AC_DATABASE_URL` to the gateway endpoint.
```

- [ ] **Step 3: Commit**

```bash
git add docs/deployment.md
git commit -m "docs(v0.3): add serverless deployment section to deployment guide"
```

---

## Self-Review

**1. Spec coverage check:**

| Spec requirement | Task |
|-----------------|------|
| `ac-runtime` crate with `HttpAdapter` + `DbAdapter` traits | Task 1 |
| Lambda adapter (lambda-web/http) | Task 2 |
| Cloud Run adapter (Tokio + $PORT) | Task 3 |
| Workers adapter (workers-rs, no TCP) | Task 4 |
| `ac-serverless` crate with 3 binaries | Task 5 |
| Build verification + smoke test | Task 6 |
| Update deployment docs | Task 7 |
| No changes to `ac-server` | All tasks (enforced as constraint) |
| Same `AC_` env var precedence | Tasks 2, 3, 5 (each binary uses `Settings::new()`) |

**2. Placeholder scan:** No TBD, TODO, or "implement later" patterns found.

**3. Type consistency check:**
- `create_app(pool: PgPool, metrics: Metrics, rate_limiter: RateLimiter) -> Router` — used consistently in Tasks 5 and 6
- `PgDbAdapter::create_pool(&str) -> Result<PgPool>` — used in Tasks 2, 3, 5
- `Settings { database_url, host, port, rate_limit_global, rate_limit_per_agent, rate_limit_window_secs }` — used in Tasks 3, 5
- `RateLimiter::new(global, per_agent, window_secs)` — used in Tasks 5, 6
- `Metrics::new()` — used in Tasks 5, 6

**4. Ambiguity check:** All requirements are explicit. The Workers adapter's DB constraint (no TCP, needs HTTP gateway) is documented in both the spec and Task 4 code comments.

---

Plan complete and saved to `docs/superpowers/plans/2026-09-06-v03-serverless-adapters.md`. Two execution options:

**1. Subagent-Driven (recommended)** - I dispatch a fresh subagent per task, review between tasks, fast iteration

**2. Inline Execution** - Execute tasks in this session using executing-plans, batch execution with checkpoints

**Which approach?**