use axum::{
    Router,
    routing::{get, post},
    extract::{State, Path, Query},
    Json,
};
use serde::Deserialize;
use super::service::MailboxService;
use std::sync::Arc;

pub fn routes() -> Router {
    Router::new()
        .route("/", post(send_message))
        .route("/", get(get_messages))
        .route("/{id}", get(get_message))
        .route("/{id}/ack", post(acknowledge_message))
}

#[derive(Deserialize)]
pub struct GetMessagesQuery {
    unacknowledged: Option<bool>,
}

#[derive(Deserialize)]
pub struct SendMessageRequest {
    pub from_agent_id: String,
    pub to_agent_id: String,
    #[serde(rename = "type")]
    pub message_type: String,
    pub payload: serde_json::Value,
    pub expires_at: Option<String>,
}

async fn send_message(
    State(service): State<Arc<MailboxService>>,
    Json(req): Json<SendMessageRequest>,
) -> Json<serde_json::Value> {
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
        Ok(msg) => Json(serde_json::json!({ "data": msg })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_messages(
    State(service): State<Arc<MailboxService>>,
    query: Option<Query<GetMessagesQuery>>,
) -> Json<serde_json::Value> {
    let agent_id = "agent_placeholder".to_string(); // TODO: extract from auth
    let only_unacknowledged = query.and_then(|q| q.unacknowledged).unwrap_or(false);

    match service.get_messages(&agent_id, only_unacknowledged).await {
        Ok(msgs) => Json(serde_json::json!({ "data": msgs })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_message(
    State(service): State<Arc<MailboxService>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match service.get_message(&id).await {
        Ok(msg) => Json(serde_json::json!({ "data": msg })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn acknowledge_message(
    State(service): State<Arc<MailboxService>>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    match service.acknowledge_message(&id).await {
        Ok(_) => Json(serde_json::json!({ "status": "acknowledged" })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}
