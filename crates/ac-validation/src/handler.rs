//! Axum route handlers for peer validation endpoints.
//!
//! Exposes two HTTP endpoints for the validation workflow (§20-22):
//! - `POST /{task_id}/validate` — submit a validation decision
//! - `GET /{task_id}` — retrieve all validation decisions for a task

use ac_db::pool::SharedPool;
use ac_types::auth::AuthenticatedAgent;
use ac_types::error::AcError;
use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use crate::quorum::QuorumDecision;
use crate::service::ValidationService;

/// Build the validation router with the given database pool.
pub fn routes(pool: SharedPool) -> Router {
    public_routes(pool.clone()).merge(protected_routes(pool))
}

/// Read-only validation history, open to anyone.
pub fn public_routes(pool: SharedPool) -> Router {
    Router::new().route("/{task_id}", get(get_validations)).with_state(pool)
}

/// Casting a vote requires a verified signature (the server layers
/// authentication over this router) and the caller must be the validator named
/// in the body.
pub fn protected_routes(pool: SharedPool) -> Router {
    Router::new().route("/{task_id}/validate", post(validate_task)).with_state(pool)
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
    security(("AgentSignature" = [])),
    tag = "validation",
    request_body = ValidateRequest,
    params(
        ("task_id" = String, Path, description = "Task ID"),
    ),
    responses(
        (status = 200, description = "Quorum decision", body = ValidateResponse,
         example = json!({"decision": "verified"})),
        (status = 400, description = "Invalid request"),
        (status = 401, description = "Missing or invalid request signature"),
        (status = 403, description = "validator_id is not the authenticated agent"),
        (status = 404, description = "Task not found"),
    ),
)]
pub async fn validate_task(
    State(pool): State<SharedPool>,
    Extension(AuthenticatedAgent(caller)): Extension<AuthenticatedAgent>,
    Path(task_id): Path<String>,
    Json(req): Json<ValidateRequest>,
) -> Result<Json<ValidateResponse>, AcError> {
    if caller != req.validator_id {
        return Err(AcError::Forbidden("validator_id must be the authenticated agent".to_string()));
    }
    let pool = pool.load();
    let service = ValidationService::new(pool);
    let decision = service
        .validate_task(&task_id, &req.validator_id, &req.decision, req.reasoning.as_deref())
        .await?;

    let decision_str = match decision {
        QuorumDecision::Verified => "verified",
        QuorumDecision::Rejected => "rejected",
        QuorumDecision::Disputed => "disputed",
        QuorumDecision::Pending => "pending",
    };

    Ok(Json(ValidateResponse { decision: decision_str.to_string() }))
}

#[utoipa::path(
    get,
    path = "/v1/validations/{task_id}",
    tag = "validation",
    params(
        ("task_id" = String, Path, description = "Task ID"),
    ),
    responses(
        (status = 200, description = "Validation history", body = serde_json::Value),
    ),
)]
pub async fn get_validations(
    State(pool): State<SharedPool>,
    Path(task_id): Path<String>,
) -> Result<Json<serde_json::Value>, ac_types::error::AcError> {
    let pool = pool.load();
    let service = ValidationService::new(pool);
    let history = service.get_validations(&task_id).await?;

    Ok(Json(serde_json::json!({
        "task_id": task_id,
        "validations": history,
        "protocol": "acp/1",
    })))
}
