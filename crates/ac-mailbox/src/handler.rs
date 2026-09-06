//! Axum HTTP route handlers for mailbox operations (§15-16).
//!
//! Exposes four REST endpoints:
//! - `POST /` — send a message to another agent's mailbox
//! - `GET /` — retrieve messages for the calling agent
//! - `GET /{id}` — retrieve a single message by UUID
//! - `POST /{id}/ack` — mark a message as acknowledged (read)

use axum::{
    Json, Router,
    extract::{Path, Query, State},
    routing::{get, post},
};
use serde::Deserialize;
use sqlx::PgPool;

use super::service::MailboxService;

/// Build the mailbox router with all four endpoints.
pub fn routes(pool: PgPool) -> Router {
    Router::new()
        .route("/", post(send_message))
        .route("/", get(get_messages))
        .route("/{id}", get(get_message))
        .route("/{id}/ack", post(acknowledge_message))
        .with_state(pool)
}

/// Query parameters for [`get_messages`].
#[derive(Deserialize, utoipa::IntoParams)]
pub struct GetMessagesQuery {
    /// Agent ID whose mailbox to query.
    pub agent_id: String,
    /// If `true`, only return messages that have not been acknowledged.
    pub unacknowledged: Option<bool>,
}

/// Request body for [`send_message`].
#[derive(Deserialize, utoipa::ToSchema)]
pub struct SendMessageRequest {
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

/// Send a message to another agent's mailbox.
///
/// Validates that both the sender and recipient exist in the agents table
/// before inserting the message row.  Returns the generated message UUID
/// on success (§15).
#[utoipa::path(
    post,
    path = "/v1/messages",
    tag = "mailbox",
    request_body = SendMessageRequest,
    responses(
        (status = 200, description = "Message sent", body = serde_json::Value),
        (status = 400, description = "Invalid request or agent not found"),
    ),
)]
pub async fn send_message(
    State(pool): State<PgPool>,
    Json(req): Json<SendMessageRequest>,
) -> Json<serde_json::Value> {
    let service = MailboxService::new(pool);
    // Parse the optional RFC 3339 expiry into a DateTime<Utc>.
    let expires_at = req
        .expires_at
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc));

    match service
        .send_message(
            &req.from_agent_id,
            &req.to_agent_id,
            &req.message_type,
            req.payload,
            expires_at,
        )
        .await
    {
        Ok(_) => Json(serde_json::json!({ "status": "sent" })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

/// Retrieve messages for an agent's mailbox.
///
/// Supports filtering by acknowledgement status via the `unacknowledged`
/// query parameter.  Expired messages are always excluded (§15).
#[utoipa::path(
    get,
    path = "/v1/messages",
    tag = "mailbox",
    params(GetMessagesQuery),
    responses(
        (status = 200, description = "Messages retrieved", body = serde_json::Value),
    ),
)]
pub async fn get_messages(
    State(pool): State<PgPool>,
    Query(query): Query<GetMessagesQuery>,
) -> Json<serde_json::Value> {
    let service = MailboxService::new(pool);
    // Default to returning all (non-expired) messages.
    let only_unacknowledged = query.unacknowledged.unwrap_or(false);

    match service.get_messages(&query.agent_id, only_unacknowledged).await {
        Ok(msgs) => Json(serde_json::json!({ "data": msgs.len(), "messages": msgs })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

/// Retrieve a single message by its UUID.
#[utoipa::path(
    get,
    path = "/v1/messages/{id}",
    tag = "mailbox",
    params(
        ("id" = String, Path, description = "Message UUID"),
    ),
    responses(
        (status = 200, description = "Message retrieved", body = serde_json::Value),
        (status = 404, description = "Message not found"),
    ),
)]
pub async fn get_message(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let service = MailboxService::new(pool);
    match service.get_message(&id).await {
        Ok(msg) => Json(serde_json::json!({ "data": msg })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

/// Mark a message as acknowledged (read).
///
/// Sets `acknowledged = true` and records the current timestamp in
/// `acknowledged_at` (§15).
#[utoipa::path(
    post,
    path = "/v1/messages/{id}/ack",
    tag = "mailbox",
    params(
        ("id" = String, Path, description = "Message UUID"),
    ),
    responses(
        (status = 200, description = "Message acknowledged", body = serde_json::Value),
        (status = 404, description = "Message not found"),
    ),
)]
pub async fn acknowledge_message(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let service = MailboxService::new(pool);
    match service.acknowledge_message(&id).await {
        Ok(_) => Json(serde_json::json!({ "status": "acknowledged" })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}
