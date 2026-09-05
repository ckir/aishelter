use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use crate::agent::AgentId;

/// Message types for the agent protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    AgentHello,
    AgentCapability,
    TaskOffer,
    TaskAccept,
    TaskReject,
    TaskCancel,
    TaskProgress,
    TaskResult,
    TaskVerify,
    TaskDispute,
    ServiceOffer,
    ServiceRequest,
}

/// A message in the agent mailbox.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub message_id: Uuid,
    pub from: AgentId,
    pub to: AgentId,
    pub r#type: MessageType,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub acknowledged: bool,
}
