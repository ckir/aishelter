//! Smoke test: verifies that the serverless binaries compile and
//! that the app router responds to health checks.

use ac_metrics::registry::Metrics;
use ac_server::middleware::rate_limit::RateLimiter;
use ac_server::server::create_app;
use axum::body::Body;
use axum::http::{Request, Uri};
use sqlx::PgPool;
use tower::ServiceExt;

/// Build the app router and test the health endpoint.
#[tokio::test]
async fn smoke_health_check() {
    let pool = create_test_pool().await;
    let metrics = Metrics::new();
    let rate_limiter = RateLimiter::new(1000, 100, 60);

    let app = create_app(pool, metrics, rate_limiter);

    let uri: Uri = "/v1/health".parse().expect("invalid URI");
    let req = Request::builder()
        .uri(uri)
        .header("X-Agent-Id", "test-agent")
        .body(Body::empty())
        .expect("failed to build request");

    let response = app.oneshot(req).await.expect("service call failed");

    assert_eq!(response.status(), 200);
    let bytes =
        axum::body::to_bytes(response.into_body(), usize::MAX).await.expect("failed to read body");
    let body = String::from_utf8(bytes.to_vec()).expect("body not utf-8");
    assert_eq!(body, "ok");
}

/// Connect to postgres using the CI-provided DATABASE_URL env var,
/// falling back to testcontainers for local dev.
async fn create_test_pool() -> PgPool {
    if let Ok(dsn) = std::env::var("DATABASE_URL") {
        // Use the CI-provided postgres service when available.
        // The migrations use CREATE TABLE IF NOT EXISTS, so they are idempotent.
        let pool = PgPool::connect(&dsn).await.expect("failed to connect to DATABASE_URL");
        run_migrations(&pool).await;
        return pool;
    }

    // Local dev: use testcontainers
    use testcontainers::runners::AsyncRunner;
    use testcontainers_modules::postgres::Postgres;

    let container = Postgres::default().start().await.expect("failed to start postgres");
    let host = container.get_host().await.expect("failed to get host");
    let port = container.get_host_port_ipv4(5432).await.expect("failed to get port");
    let dsn = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    let pool = PgPool::connect(&dsn).await.expect("failed to connect");
    run_migrations(&pool).await;
    pool
}

/// Run migrations from the workspace root.
async fn run_migrations(pool: &PgPool) {
    // Use CARGO_MANIFEST_DIR (always the crate root) to resolve the migrations path.
    // From crates/ac-serverless/, the workspace root is ../../.
    let crate_dir = env!("CARGO_MANIFEST_DIR");
    let migrations_path = format!("{}/../../migrations", crate_dir);
    let migrator = sqlx::migrate::Migrator::new(std::path::PathBuf::from(migrations_path))
        .await
        .expect("failed to create migrator");
    migrator.run(pool).await.expect("failed to run migrations");
}
