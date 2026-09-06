//! Integration test harness.
//!
//! Provides [`TestApp`] which spins up a PostgreSQL container via testcontainers,
//! runs `sqlx` migrations, builds the axum application via `ac_server::server::create_app`,
//! and exposes convenience methods for issuing requests against the in-process server.

use axum::body::Body;
use axum::http::{Request, Response, StatusCode, Uri, header};
use sqlx::PgPool;
use sqlx::migrate::Migrator;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;

use ac_metrics::registry::Metrics;
use ac_server::middleware::rate_limit::RateLimiter;

/// In-process test application wrapping an axum [`Router`].
///
/// A `TestApp` owns a PostgreSQL test container and a connection pool.
/// It is created via [`TestApp::setup`] and provides HTTP convenience
/// methods that issue requests through `tower::ServiceExt::oneshot`.
pub struct TestApp {
    pool: PgPool,
    app: axum::Router,
}

impl TestApp {
    /// Start a PostgreSQL container, create a connection pool, run migrations,
    /// and build the application router.
    ///
    /// # Panics
    ///
    /// Panics if the test container cannot be started, the pool cannot be
    /// created, migrations fail, or the app router cannot be built.
    pub async fn setup() -> Self {
        let pool = if let Ok(dsn) = std::env::var("DATABASE_URL") {
            // Use the CI-provided postgres service when available.
            // The migrations use CREATE TABLE IF NOT EXISTS, so they are idempotent
            // and safe to run multiple times against the same database.
            PgPool::connect(&dsn).await.expect("failed to connect to DATABASE_URL")
        } else {
            // Fall back to testcontainers for local dev
            let container =
                Postgres::default().start().await.expect("failed to start postgres container");
            let host = container.get_host().await.expect("failed to get postgres host");
            let port =
                container.get_host_port_ipv4(5432).await.expect("failed to get postgres port");
            let dsn = format!("postgres://postgres:postgres@{host}:{port}/postgres");
            PgPool::connect(&dsn).await.expect("failed to connect to test postgres")
        };

        // Resolve migrations path relative to the workspace root.
        // For the [[test]] in the root Cargo.toml, CARGO_MANIFEST_DIR is the workspace root.
        let migrations_path =
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("migrations");
        let migrator = Migrator::new(migrations_path).await.expect("failed to create migrator");
        migrator.run(&pool).await.expect("failed to run migrations");

        let metrics = Metrics::new();
        let rate_limiter = RateLimiter::new(1000, 100, 60);
        let app = ac_server::server::create_app(pool.clone(), metrics, rate_limiter);

        Self { pool, app }
    }

    /// Return a reference to the connection pool.
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// Issue a raw request through the axum router via oneshot.
    ///
    /// # Panics
    ///
    /// Panics if the service call fails.
    pub async fn request(&self, req: Request<Body>) -> Response<Body> {
        self.app.clone().oneshot(req).await.expect("service call failed")
    }

    /// Issue a GET request to the given path.
    ///
    /// # Panics
    ///
    /// Panics if the URI cannot be parsed or the service call fails.
    pub async fn get(&self, path: &str) -> Response<Body> {
        let uri: Uri = path.parse().expect("invalid URI");
        let req = Request::builder().uri(uri).body(Body::empty()).expect("failed to build request");
        self.request(req).await
    }

    /// Issue a POST request with a JSON body.
    ///
    /// # Panics
    ///
    /// Panics if the body cannot be serialised, the URI cannot be parsed, or
    /// the service call fails.
    pub async fn post_json(&self, path: &str, body: serde_json::Value) -> Response<Body> {
        let json = serde_json::to_string(&body).expect("failed to serialise JSON");
        let uri: Uri = path.parse().expect("invalid URI");
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json))
            .expect("failed to build request");
        self.request(req).await
    }

    /// Issue a PUT request with a JSON body.
    ///
    /// # Panics
    ///
    /// Panics if the body cannot be serialised, the URI cannot be parsed, or
    /// the service call fails.
    pub async fn put_json(&self, path: &str, body: serde_json::Value) -> Response<Body> {
        let json = serde_json::to_string(&body).expect("failed to serialise JSON");
        let uri: Uri = path.parse().expect("invalid URI");
        let req = Request::builder()
            .method("PUT")
            .uri(uri)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json))
            .expect("failed to build request");
        self.request(req).await
    }

    /// Issue a POST request with no body to the given path.
    ///
    /// Used for acknowledgement endpoints that do not require a request body.
    ///
    /// # Panics
    ///
    /// Panics if the URI cannot be parsed or the service call fails.
    pub async fn post(&self, path: &str) -> Response<Body> {
        let uri: Uri = path.parse().expect("invalid URI");
        let req = Request::builder()
            .method("POST")
            .uri(uri)
            .body(Body::empty())
            .expect("failed to build request");
        self.request(req).await
    }

    /// Extract and deserialise the JSON body from a response.
    ///
    /// # Panics
    ///
    /// Panics if the body cannot be read or deserialised.
    pub async fn json_body<T: for<'de> serde::Deserialize<'de>>(resp: Response<Body>) -> T {
        let bytes =
            axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("failed to read body");
        serde_json::from_slice(&bytes).expect("failed to parse JSON body")
    }

    /// Extract the HTTP status code from a response.
    pub fn status(resp: &Response<Body>) -> StatusCode {
        resp.status()
    }
}
