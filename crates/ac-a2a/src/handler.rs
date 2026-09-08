//! A2A (Agent-to-Agent) protocol adapter for Agent Commons v1.0.
//!
//! Translates incoming A2A requests into ACP/1 internal calls and
//! translates responses back. A2A adapters MUST NOT become the canonical
//! storage model — ACP/1 + HTTP/OpenAPI remain canonical.
//!
//! Exposes six POST endpoints over HTTP:
//! - `POST /a2a/discover` — delegate to ac-discovery search
//! - `POST /a2a/register` — delegate to ac-registry register
//! - `POST /a2a/message` — delegate to ac-mailbox send
//! - `POST /a2a/task` — delegate to ac-tasks create
//! - `POST /a2a/task/{id}/result` — delegate to ac-tasks submit
//! - `POST /a2a/task/{id}/validate` — delegate to ac-validation validate

use ac_db::pool::SharedPool;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::post,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

// --- Internal service imports ---
use ac_discovery::service::{DiscoveryService, SearchQuery};
use ac_mailbox::service::MailboxService;
use ac_registry::service::RegistryService;
use ac_tasks::service::TaskService;
use ac_types::error::AcError;

// --- Validation service (we call its service directly) ---
use ac_validation::service::ValidationService;

/// Build the A2A adapter router with all six endpoints.
pub fn a2a_router(pool: SharedPool) -> Router {
    Router::new()
        .route("/a2a/discover", post(a2a_discover))
        .route("/a2a/register", post(a2a_register))
        .route("/a2a/message", post(a2a_message))
        .route("/a2a/task", post(a2a_task))
        .route("/a2a/task/{id}/result", post(a2a_task_result))
        .route("/a2a/task/{id}/validate", post(a2a_task_validate))
        .with_state(pool)
}

// ---------------------------------------------------------------------------
// A2A response envelope
// ---------------------------------------------------------------------------

/// Standard A2A response envelope.
///
/// Every A2A endpoint returns this shape:
/// - `status`: `"ok"` on success, `"error"` on failure
/// - `data`: the successful payload (present only on `"ok"`)
/// - `error`: the error message (present only on `"error"`)
#[derive(Serialize)]
pub struct A2aResponse {
    /// `"ok"` or `"error"`.
    pub status: String,
    /// Successful response payload.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
    /// Error message when status is `"error"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl A2aResponse {
    /// Construct a successful A2A response.
    pub fn ok(data: serde_json::Value) -> Self {
        Self { status: "ok".to_string(), data: Some(data), error: None }
    }

    /// Construct an error A2A response.
    pub fn error(message: impl Into<String>) -> Self {
        Self { status: "error".to_string(), data: None, error: Some(message.into()) }
    }
}

/// A2A-specific error type for mapping internal failures to the envelope.
#[derive(Debug, Error)]
pub enum A2aError {
    #[error("internal ACP error: {0}")]
    Acp(#[from] AcError),
    #[error("invalid A2A request: {0}")]
    BadRequest(String),
}

impl From<A2aError> for A2aResponse {
    fn from(err: A2aError) -> Self {
        A2aResponse::error(err.to_string())
    }
}

// ---------------------------------------------------------------------------
// POST /a2a/discover — capability-based agent search
// ---------------------------------------------------------------------------

/// Request body for A2A discover.
#[derive(Deserialize)]
pub struct A2aDiscoverRequest {
    /// Capability name to search for (e.g. `"fact_verification"`).
    pub capability: Option<String>,
    /// Minimum reliability score threshold (0.0..1.0).
    pub min_reliability: Option<f64>,
    /// Protocol filter (e.g. `"acp/1"`).
    pub protocol: Option<String>,
    /// Agent status filter (e.g. `"ACTIVE"`).
    pub status: Option<String>,
    /// Maximum number of results (default: 20).
    pub limit: Option<i64>,
}

/// POST /a2a/discover — search for agents by capability.
///
/// Translates the A2A request into an internal [`DiscoveryService::search_agents`]
/// call and wraps the results in an A2A response envelope.
pub async fn a2a_discover(
    State(pool): State<SharedPool>,
    Json(req): Json<A2aDiscoverRequest>,
) -> Result<Json<A2aResponse>, A2aError> {
    let pool = pool.load();
    let service = DiscoveryService::new(pool);

    let query = SearchQuery {
        capability: req.capability,
        min_reliability: req.min_reliability,
        protocol: req.protocol,
        status: req.status,
        limit: req.limit.unwrap_or(20),
    };

    let results = service.search_agents(&query).await.map_err(A2aError::Acp)?;

    Ok(Json(A2aResponse::ok(serde_json::json!({
        "results": results,
        "count": results.len(),
    }))))
}

// ---------------------------------------------------------------------------
// POST /a2a/register — agent registration
// ---------------------------------------------------------------------------

/// Request body for A2A register.
#[derive(Deserialize)]
pub struct A2aRegisterRequest {
    /// Globally unique agent identifier (e.g. `"agent_<UUID>"`).
    pub agent_id: String,
    /// Ed25519 public key in hex-encoded form (64 hex characters).
    pub public_key: String,
    /// Optional human-readable profile information.
    pub profile: Option<A2aProfileFields>,
}

/// Optional profile fields for A2A registration.
#[derive(Deserialize)]
pub struct A2aProfileFields {
    /// Human-readable display name.
    pub name: Option<String>,
    /// Longer description of purpose or capabilities.
    pub description: Option<String>,
}

/// POST /a2a/register — register a new agent.
///
/// Translates the A2A request into an internal
/// [`RegistryService::register_agent`] call and returns an A2A response
/// envelope.
pub async fn a2a_register(
    State(pool): State<SharedPool>,
    Json(req): Json<A2aRegisterRequest>,
) -> Result<Json<A2aResponse>, A2aError> {
    let pool = pool.load();
    let service = RegistryService::new(pool);

    let agent = service
        .register_agent(
            &req.agent_id,
            &req.public_key,
            req.profile.as_ref().and_then(|p| p.name.clone()),
            req.profile.as_ref().and_then(|p| p.description.clone()),
        )
        .await
        .map_err(A2aError::Acp)?;

    Ok(Json(A2aResponse::ok(serde_json::json!({
        "agent_id": agent.agent_id,
        "status": agent.status,
        "protocol": "acp/1",
    }))))
}

// ---------------------------------------------------------------------------
// POST /a2a/message — send a message to another agent's mailbox
// ---------------------------------------------------------------------------

/// Request body for A2A message.
#[derive(Deserialize)]
pub struct A2aMessageRequest {
    /// The sending agent's ID.
    pub from_agent_id: String,
    /// The receiving agent's ID.
    pub to_agent_id: String,
    /// Message type string (maps to [`ac_types::message::MessageType`]).
    #[serde(rename = "type")]
    pub message_type: String,
    /// Arbitrary JSON payload carried by the message.
    pub payload: serde_json::Value,
    /// Optional RFC 3339 timestamp after which the message should be ignored.
    pub expires_at: Option<String>,
}

/// POST /a2a/message — send a message to another agent's mailbox.
///
/// Translates the A2A request into an internal
/// [`MailboxService::send_message`] call and returns an A2A response envelope.
pub async fn a2a_message(
    State(pool): State<SharedPool>,
    Json(req): Json<A2aMessageRequest>,
) -> Result<Json<A2aResponse>, A2aError> {
    let pool = pool.load();
    let service = MailboxService::new(pool);

    let expires_at = req
        .expires_at
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    let message_id = service
        .send_message(
            &req.from_agent_id,
            &req.to_agent_id,
            &req.message_type,
            req.payload,
            expires_at,
        )
        .await
        .map_err(A2aError::Acp)?;

    Ok(Json(A2aResponse::ok(serde_json::json!({
        "status": "sent",
        "message_id": message_id,
    }))))
}

// ---------------------------------------------------------------------------
// POST /a2a/task — create a new task
// ---------------------------------------------------------------------------

/// Request body for A2A task creation.
#[derive(Deserialize)]
pub struct A2aTaskRequest {
    /// Agent ID of the task requester.
    pub requester: String,
    /// Capability required to execute the task.
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

/// POST /a2a/task — create a new task contract.
///
/// Translates the A2A request into an internal [`TaskService::create_task`]
/// call and returns an A2A response envelope.
pub async fn a2a_task(
    State(pool): State<SharedPool>,
    Json(req): Json<A2aTaskRequest>,
) -> Result<Json<A2aResponse>, A2aError> {
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
        .await
        .map_err(A2aError::Acp)?;

    Ok(Json(A2aResponse::ok(serde_json::json!({
        "task_id": task_id,
        "status": "CREATED",
    }))))
}

// ---------------------------------------------------------------------------
// POST /a2a/task/{id}/result — submit a task result
// ---------------------------------------------------------------------------

/// Request body for A2A task result submission.
#[derive(Deserialize)]
pub struct A2aTaskResultRequest {
    /// Agent ID of the executor submitting the result.
    pub agent_id: String,
    /// JSON-encoded result data.
    pub result: serde_json::Value,
    /// SHA-256 hash of the output for tamper-evident verification.
    pub output_hash: String,
}

/// POST /a2a/task/{id}/result — submit a task result.
///
/// Translates the A2A request into an internal
/// [`TaskService::submit_result`] call and returns an A2A response envelope.
pub async fn a2a_task_result(
    State(pool): State<SharedPool>,
    Path(task_id): Path<String>,
    Json(req): Json<A2aTaskResultRequest>,
) -> Result<Json<A2aResponse>, A2aError> {
    let pool = pool.load();
    let service = TaskService::new(pool);

    service
        .submit_result(&task_id, &req.agent_id, req.result, &req.output_hash)
        .await
        .map_err(A2aError::Acp)?;

    Ok(Json(A2aResponse::ok(serde_json::json!({
        "status": "submitted",
    }))))
}

// ---------------------------------------------------------------------------
// POST /a2a/task/{id}/validate — submit a validation decision
// ---------------------------------------------------------------------------

/// Request body for A2A task validation.
#[derive(Deserialize)]
pub struct A2aTaskValidateRequest {
    /// The validator agent's ID.
    pub validator_id: String,
    /// The decision: `"approve"` or `"reject"`.
    pub decision: String,
    /// Optional reasoning for the decision.
    pub reasoning: Option<String>,
}

/// POST /a2a/task/{id}/validate — submit a validation decision.
///
/// Translates the A2A request into an internal
/// [`ValidationService::validate_task`] call and returns an A2A response
/// envelope.
pub async fn a2a_task_validate(
    State(pool): State<SharedPool>,
    Path(task_id): Path<String>,
    Json(req): Json<A2aTaskValidateRequest>,
) -> Result<Json<A2aResponse>, A2aError> {
    let pool = pool.load();
    let service = ValidationService::new(pool);

    let decision = service
        .validate_task(&task_id, &req.validator_id, &req.decision, req.reasoning.as_deref())
        .await
        .map_err(A2aError::Acp)?;

    let decision_str = match decision {
        ac_validation::quorum::QuorumDecision::Verified => "verified",
        ac_validation::quorum::QuorumDecision::Rejected => "rejected",
        ac_validation::quorum::QuorumDecision::Disputed => "disputed",
        ac_validation::quorum::QuorumDecision::Pending => "pending",
    };

    Ok(Json(A2aResponse::ok(serde_json::json!({
        "decision": decision_str,
    }))))
}
