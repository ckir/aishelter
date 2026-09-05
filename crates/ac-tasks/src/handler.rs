use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
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

#[derive(Deserialize)]
pub struct CreateTaskRequest {
    pub requester: String,
    pub capability: String,
    pub description: String,
    pub input: serde_json::Value,
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
    pub verification_method: Option<String>,
    pub required_validators: Option<i32>,
}

#[derive(Deserialize)]
pub struct TaskActionRequest {
    pub agent_id: String,
}

#[derive(Deserialize)]
pub struct SubmitResultRequest {
    pub agent_id: String,
    pub result: serde_json::Value,
    pub output_hash: String,
}

async fn create_task(
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

async fn get_task(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let service = TaskService::new(pool);
    match service.get_task(&id).await {
        Ok(task) => Json(serde_json::json!({ "data": task })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn accept_task(
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

async fn reject_task(
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

async fn submit_result(
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
