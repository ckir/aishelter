use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use crate::agent::AgentId;

/// Message types for the agent protocol.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
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
    pub from_agent_id: AgentId,
    pub to_agent_id: AgentId,
    pub message_type: MessageType,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub expires_at: Option<DateTime<Utc>>,
    pub acknowledged: bool,
    pub acknowledged_at: Option<DateTime<Utc>>,
}
