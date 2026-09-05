use axum::{Router, routing::get};
use sqlx::PgPool;
use tower_http::trace::TraceLayer;

/// Create the main application router with all sub-routes mounted.
pub fn create_app(pool: PgPool) -> Router {
    let app = Router::new()
        .route("/v1/health", get(health_check))
        .route("/v1/version", get(version_check))
        .nest("/v1/agents", ac_registry::handler::routes())
        .nest("/v1/discovery", ac_discovery::handler::routes())
        .nest("/v1/messages", ac_mailbox::handler::routes())
        .nest("/v1/tasks", ac_tasks::handler::routes())
        .layer(TraceLayer::new_for_http());

    app
}

async fn health_check() -> &'static str {
    "ok"
}

async fn version_check() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "version": "0.1.0",
        "protocol": "acp/1"
    }))
}
