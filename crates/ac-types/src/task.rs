use crate::agent::AgentId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Task lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TaskStatus {
    Created,
    Offered,
    Accepted,
    Running,
    Submitted,
    Verifying,
    Verified,
    Rejected,
    Disputed,
    Closed,
}

impl std::str::FromStr for TaskStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "CREATED" => Ok(Self::Created),
            "OFFERED" => Ok(Self::Offered),
            "ACCEPTED" => Ok(Self::Accepted),
            "RUNNING" => Ok(Self::Running),
            "SUBMITTED" => Ok(Self::Submitted),
            "VERIFYING" => Ok(Self::Verifying),
            "VERIFIED" => Ok(Self::Verified),
            "REJECTED" => Ok(Self::Rejected),
            "DISPUTED" => Ok(Self::Disputed),
            "CLOSED" => Ok(Self::Closed),
            _ => Err(format!("unknown TaskStatus: {}", s)),
        }
    }
}

/// A task contract between agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub task_id: String,
    pub requester: AgentId,
    pub assigned_agent_id: Option<AgentId>,
    pub capability: String,
    pub description: String,
    pub input: serde_json::Value,
    pub output: Option<serde_json::Value>,
    pub constraints_deadline: Option<DateTime<Utc>>,
    pub verification_method: String,
    pub required_validators: i32,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
