use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde::Deserialize;
use sqlx::PgPool;

use crate::service::TaskService;

pub fn routes(pool: PgPool) -> Router {
    Router::new()
        .route("/", post(create_task))
        .route("/{id}", get(get_task))
        .route("/{id}/accept", post(accept_task))
        .route("/{id}/reject", post(reject_task))
        .route("/{id}/result", post(submit_result))
        .with_state(pool)
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateTaskRequest {
    pub requester: String,
    pub capability: String,
    pub description: String,
    pub input: serde_json::Value,
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
    pub verification_method: Option<String>,
    pub required_validators: Option<i32>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct TaskActionRequest {
    pub agent_id: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SubmitResultRequest {
    pub agent_id: String,
    pub result: serde_json::Value,
    pub output_hash: String,
}

/// Create a new task.
#[utoipa::path(
    post,
    path = "/v1/tasks",
    tag = "tasks",
    request_body = CreateTaskRequest,
    responses(
        (status = 200, description = "Task created", body = serde_json::Value),
    ),
)]
pub async fn create_task(
    State(pool): State<PgPool>,
    Json(req): Json<CreateTaskRequest>,
) -> Json<serde_json::Value> {
    let service = TaskService::new(pool);
    let verification_method = req.verification_method.unwrap_or_else(|| "peer".to_string());
    let required_validators = req.required_validators.unwrap_or(2);

    match service
        .create_task(
            &req.requester,
            &req.capability,
            &req.description,
            req.input,
            req.deadline,
            &verification_method,
            required_validators,
        )
        .await
    {
        Ok(task_id) => Json(serde_json::json!({ "task_id": task_id, "status": "CREATED" })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

/// Get task details by ID.
#[utoipa::path(
    get,
    path = "/v1/tasks/{id}",
    tag = "tasks",
    params(
        ("id" = String, Path, description = "Task ID"),
    ),
    responses(
        (status = 200, description = "Task details", body = serde_json::Value),
        (status = 404, description = "Task not found"),
    ),
)]
pub async fn get_task(State(pool): State<PgPool>, Path(id): Path<String>) -> Json<serde_json::Value> {
    let service = TaskService::new(pool);
    match service.get_task(&id).await {
        Ok(task) => Json(serde_json::json!({ "data": task })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

/// Accept a task offer.
#[utoipa::path(
    post,
    path = "/v1/tasks/{id}/accept",
    tag = "tasks",
    request_body = TaskActionRequest,
    params(
        ("id" = String, Path, description = "Task ID"),
    ),
    responses(
        (status = 200, description = "Task accepted", body = serde_json::Value),
        (status = 400, description = "Invalid state transition"),
        (status = 404, description = "Task not found"),
    ),
)]
pub async fn accept_task(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
    Json(req): Json<TaskActionRequest>,
) -> Json<serde_json::Value> {
    let service = TaskService::new(pool);
    match service.accept_task(&id, &req.agent_id).await {
        Ok(()) => Json(serde_json::json!({ "status": "accepted" })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

/// Reject a task offer.
#[utoipa::path(
    post,
    path = "/v1/tasks/{id}/reject",
    tag = "tasks",
    request_body = TaskActionRequest,
    params(
        ("id" = String, Path, description = "Task ID"),
    ),
    responses(
        (status = 200, description = "Task rejected", body = serde_json::Value),
        (status = 400, description = "Invalid state transition"),
        (status = 404, description = "Task not found"),
    ),
)]
pub async fn reject_task(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
    Json(req): Json<TaskActionRequest>,
) -> Json<serde_json::Value> {
    let service = TaskService::new(pool);
    match service.reject_task(&id, &req.agent_id).await {
        Ok(()) => Json(serde_json::json!({ "status": "rejected" })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

/// Submit task result.
#[utoipa::path(
    post,
    path = "/v1/tasks/{id}/result",
    tag = "tasks",
    request_body = SubmitResultRequest,
    params(
        ("id" = String, Path, description = "Task ID"),
    ),
    responses(
        (status = 200, description = "Result submitted", body = serde_json::Value),
        (status = 400, description = "Invalid state transition"),
        (status = 404, description = "Task not found"),
    ),
)]
pub async fn submit_result(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
    Json(req): Json<SubmitResultRequest>,
) -> Json<serde_json::Value> {
    let service = TaskService::new(pool);
    match service.submit_result(&id, &req.agent_id, req.result, &req.output_hash).await {
        Ok(()) => Json(serde_json::json!({ "status": "submitted" })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}
