use axum::{Router, routing::get};
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_scalar::{Scalar, Servable};

use crate::routes::ApiDoc;

/// Create the main application router with all sub-routes mounted.
pub fn create_app(pool: PgPool) -> Router {
    let app = Router::new()
        .route("/v1/health", get(health_check))
        .route("/v1/version", get(version_check))
        .nest("/v1/agents", ac_registry::handler::routes(pool.clone()))
        .nest("/v1/discovery", ac_discovery::handler::routes(pool.clone()))
        .nest("/v1/messages", ac_mailbox::handler::routes(pool.clone()))
        .nest("/v1/tasks", ac_tasks::handler::routes(pool.clone()))
        .nest("/v1/validations", ac_validation::handler::routes(pool.clone()))
        .nest("/v1/agents", ac_reputation::handler::routes(pool.clone()))
        .layer(TraceLayer::new_for_http());

    // Merge OpenAPI spec and Scalar docs
    let doc = ApiDoc::openapi();
    app.route("/api/openapi.json", get(crate::routes::openapi))
        .merge(Scalar::with_url("/docs", doc))
}

/// GET /v1/health — liveness probe.
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
