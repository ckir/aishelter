use axum::{
    Router,
    routing::{get, post},
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use sqlx::PgPool;

use super::service::MailboxService;

pub fn routes(pool: PgPool) -> Router {
    Router::new()
        .route("/", post(send_message))
        .route("/", get(get_messages))
        .route("/{id}", get(get_message))
        .route("/{id}/ack", post(acknowledge_message))
        .with_state(pool)
}

#[derive(Deserialize)]
pub struct GetMessagesQuery {
    agent_id: String,
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
    State(pool): State<PgPool>,
    Json(req): Json<SendMessageRequest>,
) -> Json<serde_json::Value> {
    let service = MailboxService::new(pool);
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

async fn get_messages(
    State(pool): State<PgPool>,
    Query(query): Query<GetMessagesQuery>,
) -> Json<serde_json::Value> {
    let service = MailboxService::new(pool);
    let only_unacknowledged = query.unacknowledged.unwrap_or(false);

    match service.get_messages(&query.agent_id, only_unacknowledged).await {
        Ok(msgs) => Json(serde_json::json!({ "data": msgs.len(), "messages": msgs })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn get_message(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let service = MailboxService::new(pool);
    match service.get_message(&id).await {
        Ok(msg) => Json(serde_json::json!({ "data": msg })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}

async fn acknowledge_message(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> Json<serde_json::Value> {
    let service = MailboxService::new(pool);
    match service.acknowledge_message(&id).await {
        Ok(_) => Json(serde_json::json!({ "status": "acknowledged" })),
        Err(e) => Json(serde_json::json!({ "error": e.to_string() })),
    }
}
