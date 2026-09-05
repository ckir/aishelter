use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::{Deserialize, Serialize};

use crate::service::ValidationService;

/// Validation route state.
#[derive(Clone)]
pub struct ValidationState {
    pub service: ValidationService,
}

/// Request body for submitting a validation.
#[derive(Debug, Deserialize)]
pub struct ValidateRequest {
    pub validator_id: String,
    pub decision: String,
    pub reasoning: Option<String>,
}

/// Validation record response.
#[derive(Debug, Serialize)]
pub struct ValidationResponse {
    pub id: String,
    pub task_id: String,
    pub validator_agent_id: String,
    pub decision: String,
    pub reasoning: Option<String>,
    pub created_at: String,
}

/// Quorum decision response.
#[derive(Debug, Serialize)]
pub struct QuorumResponse {
    pub decision: String,
}

/// Create the validation router.
pub fn routes() -> Router<ValidationState> {
    Router::new()
        .route("/{task_id}/validate", post(validate_task))
        .route("/{task_id}", get(get_validations))
}

/// POST /{task_id}/validate — Submit a validation decision.
async fn validate_task(
    State(state): State<ValidationState>,
    Path(task_id): Path<String>,
    Json(req): Json<ValidateRequest>,
) -> Json<QuorumResponse> {
    let decision = state
        .service
        .validate_task(&task_id, &req.validator_id, &req.decision, req.reasoning.as_deref())
        .await;

    let decision_str = match decision {
        Ok(d) => format!("{:?}", d),
        Err(e) => format!("error: {:?}", e),
    };

    Json(QuorumResponse {
        decision: decision_str,
    })
}

/// GET /{task_id} — Get all validations for a task.
async fn get_validations(
    State(state): State<ValidationState>,
    Path(task_id): Path<String>,
) -> Json<Vec<ValidationResponse>> {
    let validations = state.service.get_validations(&task_id).await.unwrap_or_default();

    Json(
        validations
            .into_iter()
            .map(|v| ValidationResponse {
                id: v.id.to_string(),
                task_id: v.task_id,
                validator_agent_id: v.validator_agent_id,
                decision: v.decision,
                reasoning: v.reasoning,
                created_at: v.created_at.to_rfc3339(),
            })
            .collect(),
    )
}
