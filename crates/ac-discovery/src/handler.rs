//! Discovery service HTTP route.
//!
//! Exposes a single `GET /search` endpoint (§13-14) that accepts query
//! parameters for capability-based agent search.  Results are ordered by
//! reliability score (descending) and filtered by optional minimum
//! reliability threshold.

use axum::{
    Router,
    extract::{Query, State},
    response::Json,
    routing::get,
};
use serde::Deserialize;
use sqlx::PgPool;

use crate::service::{DiscoveryService, SearchQuery};

/// Query parameters for `GET /search`.
///
/// All parameters are optional.  When `capability` is provided, only agents
/// with that capability are returned.  `min_reliability` filters out agents
/// below the given score threshold.  `limit` controls the maximum number of
/// results (default: 20).
#[derive(Debug, Deserialize, utoipa::IntoParams)]
pub struct SearchParams {
    /// Filter by capability name (e.g. `"fact_verification"`).
    pub capability: Option<String>,
    /// Minimum reliability score threshold (0.0..1.0).
    pub min_reliability: Option<f64>,
    /// Protocol filter (e.g. `"acp/1"`).
    pub protocol: Option<String>,
    /// Agent status filter (e.g. `"ACTIVE"`).
    pub status: Option<String>,
    /// Maximum number of results to return (default: 20).
    pub limit: Option<i64>,
}

/// GET /search — search for agents by capability.
///
/// Delegates to [`DiscoveryService::search_agents`] with the parsed query
/// parameters.  Returns a JSON object with `results`, `count`, and
/// `protocol` fields.
#[utoipa::path(
    get,
    path = "/v1/discovery/search",
    tag = "discovery",
    params(SearchParams),
    responses(
        (status = 200, description = "Search results", body = serde_json::Value),
    ),
)]
pub async fn search(
    State(pool): State<PgPool>,
    Query(params): Query<SearchParams>,
) -> Result<Json<serde_json::Value>, ac_types::error::AcError> {
    let service = DiscoveryService::new(pool);
    let query = SearchQuery {
        capability: params.capability,
        min_reliability: params.min_reliability,
        protocol: params.protocol,
        status: params.status,
        limit: params.limit.unwrap_or(20),
    };

    let results = service.search_agents(&query).await?;
    
    Ok(Json(serde_json::json!({
        "results": results,
        "count": results.len(),
        "protocol": "acp/1",
    })))
}

/// Build the discovery router with the search route mounted.
///
/// The router expects a [`PgPool`] state for database access.
pub fn routes(pool: PgPool) -> Router {
    Router::new().route("/search", get(search)).with_state(pool)
}
