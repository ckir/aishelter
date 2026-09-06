//! Smoke test: verifies that the serverless binaries compile and
//! that the Cloud Run adapter's router responds to health checks.

use ac_metrics::registry::Metrics;
use ac_server::middleware::rate_limit::RateLimiter;
use ac_server::server::create_app;
use axum::body::Body;
use axum::http::{Request, Uri};
use sqlx::PgPool;
use testcontainers::runners::AsyncRunner;
use testcontainers_modules::postgres::Postgres;
use tower::ServiceExt;

/// Build the app router and test the health endpoint.
#[tokio::test]
async fn cloudrun_health_check() {
    let pool = create_test_pool().await;
    let metrics = Metrics::new();
    let rate_limiter = RateLimiter::new(1000, 100, 60);

    let app = create_app(pool, metrics, rate_limiter);

    let uri: Uri = "/v1/health".parse().expect("invalid URI");
    let req = Request::builder().uri(uri).body(Body::empty()).expect("failed to build request");

    let response = app.oneshot(req).await.expect("service call failed");

    assert_eq!(response.status(), 200);
    let bytes =
        axum::body::to_bytes(response.into_body(), usize::MAX).await.expect("failed to read body");
    let body = String::from_utf8(bytes.to_vec()).expect("body not utf-8");
    assert_eq!(body, "ok");
}

async fn create_test_pool() -> PgPool {
    let container = Postgres::default().start().await.expect("failed to start postgres container");

    let host = container.get_host().await.expect("failed to get postgres host");
    let port = container.get_host_port_ipv4(5432).await.expect("failed to get postgres port");

    let dsn = format!("postgres://postgres:postgres@{host}:{port}/postgres");
    PgPool::connect(&dsn).await.expect("failed to connect to test postgres")
}
