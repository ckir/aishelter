//! Axum HTTP server and route registration.
//!
//! This module assembles the top-level [`axum::Router`] that serves all
//! Agent Commons API endpoints.  Each feature crate (registry, discovery,
//! mailbox, tasks, validation, reputation) contributes its own nested
//! sub-router via `pub fn routes(pool) -> Router`.
//!
//! The server also hosts the Scalar interactive API documentation at
//! `/docs` and the raw OpenAPI specification at `/api/openapi.json`.

use crate::middleware::rate_limit::{RateLimiter, rate_limit as rate_limit_mw};
use ac_db::pool::SharedPool;
use ac_metrics::middleware::record_metrics;
use ac_metrics::readiness;
use ac_metrics::registry::Metrics;
use axum::{Router, extract::Extension, middleware::from_fn, routing::get};
use http::header::HeaderName;
use tower_http::request_id::{MakeRequestUuid, SetRequestIdLayer};
use tower_http::trace::{DefaultMakeSpan, TraceLayer};
use tracing::Level;
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};

use crate::routes::ApiDoc;

/// Create the main application router with all sub-routes mounted.
///
/// The returned router includes:
/// - System endpoints (`/v1/health`, `/v1/version`, `/metrics`)
/// - Feature sub-routes under `/v1/{feature}`
/// - HTTP request tracing via `tower-http`
/// - Metrics middleware recording request counts and durations
/// - Rate limiting with global and per-agent token buckets
/// - OpenAPI JSON at `/api/openapi.json`
/// - Interactive Scalar UI at `/docs`
pub fn create_app(pool: SharedPool, metrics: Metrics, rate_limiter: RateLimiter) -> Router {
    let app = Router::new()
        .route("/v1/health", get(health_check))
        .route("/v1/version", get(version_check))
        .route("/metrics", get(metrics_handler))
        .route(
            "/v1/ready",
            get({
                let pool = pool.clone();
                move || async move {
                    let pg_pool = pool.load();
                    if readiness::check(&pg_pool).await {
                        (axum::http::StatusCode::OK, "ready")
                    } else {
                        (axum::http::StatusCode::SERVICE_UNAVAILABLE, "not ready")
                    }
                }
            }),
        )
        .nest("/v1/agents", ac_registry::handler::routes(pool.clone()))
        .nest("/", ac_directory::well_known::routes(pool.clone()))
        .nest("/", ac_directory::handler::routes(pool.clone()))
        .nest("/v1/discovery", ac_discovery::handler::routes(pool.clone()))
        .nest("/v1/messages", ac_mailbox::handler::routes(pool.clone()))
        .nest("/v1/tasks", ac_tasks::handler::routes(pool.clone()))
        .nest("/v1/validations", ac_validation::handler::routes(pool.clone()))
        .nest("/v1/agents", ac_reputation::handler::routes(pool.clone()))
        .layer(from_fn(rate_limit_mw))
        .layer(Extension(rate_limiter))
        .layer(from_fn(record_metrics))
        .layer(Extension(metrics.clone()))
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().include_headers(true).level(Level::INFO)),
        )
        .layer(SetRequestIdLayer::new(HeaderName::from_static("x-request-id"), MakeRequestUuid));

    // Merge OpenAPI spec and Scalar docs
    let doc = ApiDoc::openapi();
    app.route("/api/openapi.json", get(crate::routes::openapi))
        .merge(Scalar::with_url("/docs", doc))
}

/// GET /v1/health — liveness probe.
///
/// Returns `"ok"` with HTTP 200 if the server is running.
/// Used by container orchestrators and load balancers.
#[utoipa::path(
    get,
    path = "/v1/health",
    tag = "system",
    responses(
        (status = 200, description = "Service is healthy", body = String),
    ),
)]
async fn health_check() -> &'static str {
    "ok"
}

/// GET /v1/version — version and protocol info.
///
/// Returns the server version and the ACP protocol version it implements.
/// Clients can use this to verify compatibility before making requests.
#[utoipa::path(
    get,
    path = "/v1/version",
    tag = "system",
    responses(
        (status = 200, description = "Version info", body = serde_json::Value,
         example = json!({"version": "0.1.0", "protocol": "acp/1"})),
    ),
)]
async fn version_check() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "version": "0.1.0",
        "protocol": "acp/1"
    }))
}

/// GET /metrics — Prometheus metrics endpoint.
///
/// Returns all registered metrics in Prometheus text exposition format.
/// Consumed by Prometheus scrapers for observability dashboards.
async fn metrics_handler(
    Extension(metrics): Extension<Metrics>,
) -> (axum::http::StatusCode, [(http::HeaderName, &'static str); 1], String) {
    (
        axum::http::StatusCode::OK,
        [(http::header::CONTENT_TYPE, "text/plain; version=0.0.4; charset=utf-8")],
        metrics.encode(),
    )
}
