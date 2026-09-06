//! Axum route handlers for peer validation endpoints.
//!
//! Exposes two HTTP endpoints for the validation workflow (§20-22):
//! - `POST /{task_id}/validate` — submit a validation decision
//! - `GET /{task_id}` — retrieve all validation decisions for a task

use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::quorum::QuorumDecision;
use crate::service::ValidationService;

/// Build the validation router with the given database pool.
pub fn routes(pool: PgPool) -> Router {
    Router::new()
        .route("/{task_id}/validate", post(validate_task))
        .route("/{task_id}", get(get_validations))
        .with_state(pool)
}

/// Request body for submitting a validation decision.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct ValidateRequest {
    /// The validator agent's ID.
    pub validator_id: String,
    /// The decision: `"approve"` or `"reject"`.
    pub decision: String,
    /// Optional reasoning for the decision.
    pub reasoning: Option<String>,
}

/// Response body containing the quorum decision.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ValidateResponse {
    /// The quorum result: `"verified"`, `"rejected"`, `"disputed"`, or `"pending"`.
    pub decision: String,
}

/// Submit a validation decision for a task.
///
/// Records the validator's vote and computes the quorum decision.
/// If the quorum threshold is met, the task status is updated accordingly.
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
    // Create the validation service.
    let service = ValidationService::new(pool);
    // Submit the vote and get the quorum decision.
    let decision = service
        .validate_task(&task_id, &req.validator_id, &req.decision, req.reasoning.as_deref())
        .await;

    // Map the enum variant to a human-readable string.
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
///
/// Returns the full history of votes cast by validators, including
/// their reasoning.  Useful for auditing disputes (§22).
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
