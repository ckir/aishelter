use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::quorum::QuorumDecision;
use crate::service::ValidationService;

pub fn routes(pool: PgPool) -> Router {
    Router::new()
        .route("/{task_id}/validate", post(validate_task))
        .route("/{task_id}", get(get_validations))
        .with_state(pool)
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ValidateRequest {
    pub validator_id: String,
    pub decision: String,
    pub reasoning: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ValidateResponse {
    pub decision: String,
}

/// Submit a validation decision for a task.
#[utoipa::path(
    post,
    path = "/v1/validations/{task_id}/validate",
    tag = "validation",
    request_body = ValidateRequest,
    params(
        ("task_id" = String, Path, description = "Task ID"),
    ),
    responses(
        (status = 200, description = "Quorum decision", body = ValidateResponse,
         example = json!({"decision": "verified"})),
        (status = 400, description = "Invalid request"),
        (status = 404, description = "Task not found"),
    ),
)]
pub async fn validate_task(
    State(pool): State<PgPool>,
    Path(task_id): Path<String>,
    Json(req): Json<ValidateRequest>,
) -> Json<ValidateResponse> {
    let service = ValidationService::new(pool);
    let decision = service
        .validate_task(&task_id, &req.validator_id, &req.decision, req.reasoning.as_deref())
        .await;

    let decision_str = match decision {
        Ok(QuorumDecision::Verified) => "verified",
        Ok(QuorumDecision::Rejected) => "rejected",
        Ok(QuorumDecision::Disputed) => "disputed",
        Ok(QuorumDecision::Pending) => "pending",
        Err(e) => return Json(ValidateResponse { decision: format!("error: {}", e) }),
    };

    Json(ValidateResponse { decision: decision_str.to_string() })
}

/// Get all validation decisions for a task.
#[utoipa::path(
    get,
    path = "/v1/validations/{task_id}",
    tag = "validation",
    params(
        ("task_id" = String, Path, description = "Task ID"),
    ),
    responses(
        (status = 200, description = "Validation decisions", body = serde_json::Value),
        (status = 404, description = "Task not found"),
    ),
)]
pub async fn get_validations(
    State(pool): State<PgPool>,
    Path(task_id): Path<String>,
) -> Json<serde_json::Value> {
    let service = ValidationService::new(pool);
    match service.get_validations(&task_id).await {
        Ok(validations) => Json(serde_json::json!({ "data": validations })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}
