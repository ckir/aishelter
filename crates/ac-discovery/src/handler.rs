use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::Json,
    Router,
    routing::get,
};
use serde::Deserialize;
use sqlx::PgPool;

use crate::service::{DiscoveryService, SearchQuery};

/// Query parameters for GET /search.
#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub capability: Option<String>,
    pub min_reliability: Option<f64>,
    pub protocol: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
}

/// GET /search — search for agents by capability.
pub async fn search(
    State(pool): State<PgPool>,
    Query(params): Query<SearchParams>,
) -> Json<serde_json::Value> {
    let service = DiscoveryService::new(pool);
    let query = SearchQuery {
        capability: params.capability,
        min_reliability: params.min_reliability,
        protocol: params.protocol,
        status: params.status,
        limit: params.limit.unwrap_or(20),
    };

    match service.search_agents(&query).await {
        Ok(results) => Json(serde_json::json!({
            "results": results,
            "count": results.len(),
            "protocol": "acp/1",
        })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

pub fn routes(pool: PgPool) -> Router {
    Router::new()
        .route("/search", get(search))
        .with_state(pool)
}
