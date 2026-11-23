//! Axum HTTP route handlers for task operations (§18).
//!
//! Exposes five REST endpoints:
//! - `POST /` — create a new task contract
//! - `GET /{id}` — retrieve task details
//! - `POST /{id}/accept` — accept a task offer
//! - `POST /{id}/reject` — reject a task offer
//! - `POST /{id}/result` — submit a task result

use ac_db::pool::SharedPool;
use ac_types::auth::AuthenticatedAgent;
use ac_types::error::AcError;
use axum::{
    Json, Router,
    extract::{Extension, Path, State},
    routing::{get, post},
};
use serde::Deserialize;

use crate::service::TaskService;

/// Build the task router with all five endpoints.
pub fn routes(pool: SharedPool) -> Router {
    public_routes(pool.clone()).merge(protected_routes(pool))
}

/// Read-only task lookup, open to anyone.
pub fn public_routes(pool: SharedPool) -> Router {
    Router::new().route("/{id}", get(get_task)).with_state(pool)
}

/// Routes that change task state. The server must layer authentication over
/// this router; each handler then checks that the verified caller is the agent
/// named in the request body.
pub fn protected_routes(pool: SharedPool) -> Router {
    Router::new()
        .route("/", post(create_task))
        .route("/{id}/accept", post(accept_task))
        .route("/{id}/reject", post(reject_task))
        .route("/{id}/result", post(submit_result))
        .with_state(pool)
}

/// The body must name the verified caller; anything else is a 403.
fn require_caller(caller: &str, named: &str, what: &str) -> Result<(), AcError> {
    if caller == named {
        Ok(())
    } else {
        Err(AcError::Forbidden(format!("{what} must be the authenticated agent")))
    }
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
    security(("AgentSignature" = [])),
    tag = "tasks",
    request_body = CreateTaskRequest,
    responses(
        (status = 200, description = "Task created", body = serde_json::Value),
    ),
)]
pub async fn create_task(
    State(pool): State<SharedPool>,
    Extension(AuthenticatedAgent(caller)): Extension<AuthenticatedAgent>,
    Json(req): Json<CreateTaskRequest>,
) -> Result<Json<serde_json::Value>, AcError> {
    require_caller(&caller, &req.requester, "requester")?;
    let pool = pool.load();
    let service = TaskService::new(pool);
    let verification_method = req.verification_method.unwrap_or_else(|| "peer".to_string());
    let required_validators = req.required_validators.unwrap_or(2);

    let task_id = service
        .create_task(
            &req.requester,
            &req.capability,
            &req.description,
            req.input,
            req.deadline,
            &verification_method,
            required_validators,
        )
        .await?;

    Ok(Json(serde_json::json!({ "task_id": task_id, "status": "CREATED" })))
}

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
    State(pool): State<SharedPool>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ac_types::error::AcError> {
    let pool = pool.load();
    let service = TaskService::new(pool);
    let task = service.get_task(&id).await?;
    Ok(Json(serde_json::json!({ "data": task })))
}

#[utoipa::path(
    post,
    path = "/v1/tasks/{id}/accept",
    security(("AgentSignature" = [])),
    tag = "tasks",
    request_body = TaskActionRequest,
    params(
        ("id" = String, Path, description = "Task ID"),
    ),
    responses(
        (status = 200, description = "Task accepted", body = serde_json::Value),
        (status = 400, description = "Invalid state transition"),
        (status = 401, description = "Missing or invalid request signature"),
        (status = 403, description = "agent_id is not the authenticated agent"),
        (status = 404, description = "Task not found"),
    ),
)]
pub async fn accept_task(
    State(pool): State<SharedPool>,
    Extension(AuthenticatedAgent(caller)): Extension<AuthenticatedAgent>,
    Path(id): Path<String>,
    Json(req): Json<TaskActionRequest>,
) -> Result<Json<serde_json::Value>, AcError> {
    require_caller(&caller, &req.agent_id, "agent_id")?;
    let pool = pool.load();
    let service = TaskService::new(pool);
    service.accept_task(&id, &req.agent_id).await?;
    Ok(Json(serde_json::json!({ "status": "accepted" })))
}

#[utoipa::path(
    post,
    path = "/v1/tasks/{id}/reject",
    security(("AgentSignature" = [])),
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
    State(pool): State<SharedPool>,
    Extension(AuthenticatedAgent(caller)): Extension<AuthenticatedAgent>,
    Path(id): Path<String>,
    Json(req): Json<TaskActionRequest>,
) -> Result<Json<serde_json::Value>, AcError> {
    require_caller(&caller, &req.agent_id, "agent_id")?;
    let pool = pool.load();
    let service = TaskService::new(pool);
    service.reject_task(&id, &req.agent_id).await?;
    Ok(Json(serde_json::json!({ "status": "rejected" })))
}

#[utoipa::path(
    post,
    path = "/v1/tasks/{id}/result",
    security(("AgentSignature" = [])),
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
    State(pool): State<SharedPool>,
    Extension(AuthenticatedAgent(caller)): Extension<AuthenticatedAgent>,
    Path(id): Path<String>,
    Json(req): Json<SubmitResultRequest>,
) -> Result<Json<serde_json::Value>, AcError> {
    require_caller(&caller, &req.agent_id, "agent_id")?;
    let pool = pool.load();
    let service = TaskService::new(pool);
    service.submit_result(&id, &req.agent_id, req.result, &req.output_hash).await?;
    Ok(Json(serde_json::json!({ "status": "submitted" })))
}
