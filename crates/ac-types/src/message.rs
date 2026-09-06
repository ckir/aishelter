//! Mailbox message types and envelope.
//!
//! Defines the [`MessageType`] enum for the various protocol-level messages
//! agents exchange (task offers, accepts, results, etc.) and the [`Message`]
//! struct that wraps them in a persistent, addressable envelope.

use crate::agent::AgentId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Message types for the agent protocol.
///
/// Each variant corresponds to a specific phase of the task lifecycle
/// (§16) or a service-level announcement between agents.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    /// Agent announced its presence on the network.
    AgentHello,
    /// Agent published or updated its capabilities.
    AgentCapability,
    /// A task is being offered to another agent.
    TaskOffer,
    /// The receiving agent accepts the offered task.
    TaskAccept,
    /// The receiving agent declines the offered task.
    TaskReject,
    /// The requester cancels a previously offered task.
    TaskCancel,
    /// Progress update from the executing agent.
    TaskProgress,
    /// The executing agent submits the task result.
    TaskResult,
    /// A validator submits a verification decision.
    TaskVerify,
    /// A validator raises a dispute about the result.
    TaskDispute,
    /// An agent offers a service it can perform.
    ServiceOffer,
    /// An agent requests a service from another agent.
    ServiceRequest,
}

/// A message in the agent mailbox.
///
/// Messages are stored persistently in the database and delivered to the
/// recipient agent when it next polls its mailbox (§15).  Each message has
/// a unique UUID, sender, recipient, type, JSON payload, optional expiry,
/// and an acknowledgement flag.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    /// Unique message identifier (UUID v4).
    pub message_id: Uuid,
    /// The sending agent's ID.
    pub from_agent_id: AgentId,
    /// The receiving agent's ID.
    pub to_agent_id: AgentId,
    /// The kind of protocol message.
    pub message_type: MessageType,
    /// Arbitrary JSON payload specific to the message type.
    pub payload: serde_json::Value,
    /// When the message was created.
    pub created_at: DateTime<Utc>,
    /// Optional expiration — the message should be ignored after this time.
    pub expires_at: Option<DateTime<Utc>>,
    /// Whether the recipient has acknowledged reading this message.
    pub acknowledged: bool,
    /// When the message was acknowledged (if it has been).
    pub acknowledged_at: Option<DateTime<Utc>>,
}
