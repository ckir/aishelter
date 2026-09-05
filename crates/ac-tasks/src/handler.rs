use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use ac_api::types::ApiResponse;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Clone)]
pub struct TaskState {
    pub pool: PgPool,
}

/// Request body for creating a task.
#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub requester: String,
    pub capability: String,
    pub description: String,
    pub input: serde_json::Value,
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
    pub verification_method: Option<String>,
    pub required_validators: Option<usize>,
}

/// Request body for accepting/rejecting a task.
#[derive(Debug, Deserialize)]
pub struct TaskActionRequest {
    pub agent_id: String,
}

/// Request body for submitting a result.
#[derive(Debug, Deserialize)]
pub struct SubmitResultRequest {
    pub agent_id: String,
    pub result: serde_json::Value,
    pub output_hash: String,
}

/// Task routes.
pub fn routes() -> Router {
    Router::new()
        .route("/", post(create_task))
        .route("/{id}", get(get_task))
        .route("/{id}/accept", post(accept_task))
        .route("/{id}/reject", post(reject_task))
        .route("/{id}/result", post(submit_result))
}

async fn create_task(
    State(state): State<TaskState>,
    Json(req): Json<CreateTaskRequest>,
) -> Json<ApiResponse<serde_json::Value>> {
    let service = crate::service::TaskService::new(state.pool);
    let verification_method = req.verification_method.unwrap_or_else(|| "peer".to_string());
    let required_validators = req.required_validators.unwrap_or(2);

    match service
        .create_task(
            req.requester,
            req.capability,
            req.description,
            req.input,
            req.deadline,
            verification_method,
            required_validators,
        )
        .await
    {
        Ok(task) => Json(ApiResponse::ok(serde_json::to_value(task).unwrap())),
        Err(e) => Json(ApiResponse::err(400, e.to_string())),
    }
}

async fn get_task(
    State(state): State<TaskState>,
    Path(id): Path<String>,
) -> Json<ApiResponse<serde_json::Value>> {
    let service = crate::service::TaskService::new(state.pool);
    match service.get_task(&id).await {
        Ok(task) => Json(ApiResponse::ok(serde_json::to_value(task).unwrap())),
        Err(e) => Json(ApiResponse::err(404, e.to_string())),
    }
}

async fn accept_task(
    State(state): State<TaskState>,
    Path(id): Path<String>,
    Json(req): Json<TaskActionRequest>,
) -> Json<ApiResponse<()>> {
    let service = crate::service::TaskService::new(state.pool);
    match service.accept_task(&id, &req.agent_id).await {
        Ok(()) => Json(ApiResponse::ok(())),
        Err(e) => Json(ApiResponse::err(400, e.to_string())),
    }
}

async fn reject_task(
    State(state): State<TaskState>,
    Path(id): Path<String>,
    Json(req): Json<TaskActionRequest>,
) -> Json<ApiResponse<()>> {
    let service = crate::service::TaskService::new(state.pool);
    match service.reject_task(&id, &req.agent_id).await {
        Ok(()) => Json(ApiResponse::ok(())),
        Err(e) => Json(ApiResponse::err(400, e.to_string())),
    }
}

async fn submit_result(
    State(state): State<TaskState>,
    Path(id): Path<String>,
    Json(req): Json<SubmitResultRequest>,
) -> Json<ApiResponse<()>> {
    let service = crate::service::TaskService::new(state.pool);
    match service.submit_result(&id, &req.agent_id, req.result, &req.output_hash).await {
        Ok(()) => Json(ApiResponse::ok(())),
        Err(e) => Json(ApiResponse::err(400, e.to_string())),
    }
}
