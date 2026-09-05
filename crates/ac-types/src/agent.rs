use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Globally unique agent identifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentId(pub String);

impl AgentId {
    pub fn new() -> Self {
        Self(format!("agent_{}", Uuid::new_v4()))
    }
}

impl Default for AgentId {
    fn default() -> Self {
        Self::new()
    }
}

/// Agent lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentStatus {
    Unregistered,
    Registered,
    Active,
    Discoverable,
    Tasking,
    ReputationBuilding,
    Suspended,
    Revoked,
    Retired,
}

impl Default for AgentStatus {
    fn default() -> Self {
        Self::Registered
    }
}

/// Core agent record stored in the database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub agent_id: AgentId,
    pub public_key: String,
    pub profile_name: Option<String>,
    pub profile_description: Option<String>,
    pub status: AgentStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
