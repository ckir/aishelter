use crate::service::ReputationService;
use ac_types::error::AcError;
use axum::{
    Router,
    extract::{Path, State},
    http::StatusCode,
    response::Json,
    routing::get,
};
use serde::Serialize;
use ac_db::pool::SharedPool;

/// Application state for reputation routes.
#[derive(Clone)]
pub struct ReputationState {
    pub pool: SharedPool,
}

/// Reputation scores response.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ReputationResponse {
    pub agent_id: String,
    pub reliability: f64,
    pub reliability_confidence: f64,
    pub task_success: f64,
    pub task_success_confidence: f64,
    pub verification_accuracy: f64,
    pub verification_accuracy_confidence: f64,
    pub responsiveness: f64,
    pub responsiveness_confidence: f64,
    pub vwu_total: u64,
}

/// Contribution stats response.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ContributionsResponse {
    pub vwu_total: i64,
    pub verified_tasks: i64,
    pub validation_tasks: i64,
    pub validation_accuracy: f64,
}

/// GET /{id}/reputation — Get reputation scores for an agent.
#[utoipa::path(
    get,
    path = "/v1/agents/{id}/reputation",
    tag = "reputation",
    params(
        ("id" = String, Path, description = "Agent ID"),
    ),
    responses(
        (status = 200, description = "Reputation scores", body = ReputationResponse),
        (status = 500, description = "Internal error"),
    ),
)]
pub async fn get_reputation(
    Path(agent_id): Path<String>,
    State(state): State<ReputationState>,
) -> Result<Json<ReputationResponse>, (StatusCode, String)> {
    let pool = state.pool.load();
    let service = ReputationService::new(pool);
    let snapshot = service.get_reputation(&agent_id).await.map_err(|e| match e {
        AcError::AgentNotFound(_) => (StatusCode::NOT_FOUND, e.to_string()),
        AcError::Database(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        other => (StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
    })?;

    Ok(Json(ReputationResponse {
        agent_id: snapshot.agent_id.0,
        reliability: snapshot.reliability,
        reliability_confidence: snapshot.reliability_confidence,
        task_success: snapshot.task_success,
        task_success_confidence: snapshot.task_success_confidence,
        verification_accuracy: snapshot.verification_accuracy,
        verification_accuracy_confidence: snapshot.verification_accuracy_confidence,
        responsiveness: snapshot.responsiveness,
        responsiveness_confidence: snapshot.responsiveness_confidence,
        vwu_total: snapshot.vwu_total,
    }))
}

/// GET /{id}/contributions — Get contribution stats for an agent.
#[utoipa::path(
    get,
    path = "/v1/agents/{id}/contributions",
    tag = "reputation",
    params(
        ("id" = String, Path, description = "Agent ID"),
    ),
    responses(
        (status = 200, description = "Contribution stats", body = ContributionsResponse),
        (status = 500, description = "Internal error"),
    ),
)]
pub async fn get_contributions(
    Path(agent_id): Path<String>,
    State(state): State<ReputationState>,
) -> Result<Json<ContributionsResponse>, (StatusCode, String)> {
    let pool = state.pool.load();
    let service = ReputationService::new(pool);
    let stats = service
        .get_contributions(&agent_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(ContributionsResponse {
        vwu_total: stats.vwu_total,
        verified_tasks: stats.verified_tasks,
        validation_tasks: stats.validation_tasks,
        validation_accuracy: stats.validation_accuracy,
    }))
}

/// Build the reputation router.
pub fn routes(pool: SharedPool) -> Router {
    let state = ReputationState { pool };
    Router::new()
        .route("/{id}/reputation", get(get_reputation))
        .route("/{id}/contributions", get(get_contributions))
        .with_state(state)
}
