//! Axum HTTP route handlers for task operations (§18).
//!
//! Exposes five REST endpoints:
//! - `POST /` — create a new task contract
//! - `GET /{id}` — retrieve task details
//! - `POST /{id}/accept` — accept a task offer
//! - `POST /{id}/reject` — reject a task offer
//! - `POST /{id}/result` — submit a task result

use axum::{
    extract::{Path, State},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use sqlx::PgPool;

use crate::service::TaskService;

/// Build the task router with all five endpoints.
pub fn routes(pool: PgPool) -> Router {
    Router::new()
        .route("/", post(create_task))
        .route("/{id}", get(get_task))
        .route("/{id}/accept", post(accept_task))
        .route("/{id}/reject", post(reject_task))
        .route("/{id}/result", post(submit_result))
        .with_state(pool)
}

/// Request body for [`create_task`].
#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateTaskRequest {
    /// Agent ID of the task requester.
    pub requester: String,
    /// Capability required to execute the task (e.g. `"fact_verification"`).
    pub capability: String,
    /// Human-readable description of the work.
    pub description: String,
    /// JSON-encoded input data for the task executor.
    pub input: serde_json::Value,
    /// Optional deadline for task completion.
    pub deadline: Option<chrono::DateTime<chrono::Utc>>,
    /// Verification method: `"peer"`, `"requester"`, or `"deterministic"`.
    pub verification_method: Option<String>,
    /// Number of validator approvals required (default 2).
    pub required_validators: Option<i32>,
}

/// Request body for [`accept_task`] and [`reject_task`].
#[derive(Deserialize, utoipa::ToSchema)]
pub struct TaskActionRequest {
    /// Agent ID of the executor responding to the offer.
    pub agent_id: String,
}

/// Request body for [`submit_result`].
#[derive(Deserialize, utoipa::ToSchema)]
pub struct SubmitResultRequest {
    /// Agent ID of the executor submitting the result.
    pub agent_id: String,
    /// JSON-encoded result data.
    pub result: serde_json::Value,
    /// SHA-256 hash of the output for tamper-evident verification.
    pub output_hash: String,
}

/// Create a new task contract.
///
/// The task starts in the `CREATED` state.  The requester is recorded,
/// and the capability, input data, and verification parameters are stored
/// (§18).
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
    // Apply sensible defaults when the caller omits them.
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

/// Retrieve full task details by ID.
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
pub async fn get_task(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let service = TaskService::new(pool);
    match service.get_task(&id).await {
        Ok(task) => Json(serde_json::json!({ "data": task })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

/// Accept a task offer.
///
/// Transitions the task from `CREATED` or `OFFERED` to `ACCEPTED`
/// and records the accepting agent as the assignee (§18).
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
///
/// Only valid when the task is in the `OFFERED` state (§18).
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

/// Submit a task result for validation.
///
/// The executor provides the result JSON and a SHA-256 hash of the
/// output.  The task transitions to `SUBMITTED` and a row is inserted
/// into the `task_results` table (§20).
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
