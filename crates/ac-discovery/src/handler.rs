use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::IntoResponse,
    Router,
    routing::get,
};
use serde::Deserialize;
use std::sync::Arc;

use crate::service::{DiscoveryService, SearchQuery};

/// Shared application state for handlers.
pub struct AppState {
    pub discovery: DiscoveryService,
}

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
    State(state): State<Arc<AppState>>,
    Query(params): Query<SearchParams>,
) -> impl IntoResponse {
    let query = SearchQuery {
        capability: params.capability,
        min_reliability: params.min_reliability,
        protocol: params.protocol,
        status: params.status,
        limit: params.limit.unwrap_or(20),
    };

    match state.discovery.search_agents(&query).await {
        Ok(results) => {
            let body = serde_json::json!({
                "results": results,
                "count": results.len(),
                "protocol": "acp/1",
            });
            (StatusCode::OK, axum::Json(body)).into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "discovery search failed");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                axum::Json(serde_json::json!({ "error": e.to_string() })),
            )
                .into_response()
        }
    }
}

/// Mount discovery routes.
pub fn routes(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/search", get(search))
        .with_state(state)
}
