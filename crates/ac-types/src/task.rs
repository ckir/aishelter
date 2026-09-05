use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use crate::agent::AgentId;

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

/// Verification method for a task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Verification {
    pub method: String,
    pub required_validators: usize,
}

/// Task constraints.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskConstraints {
    pub deadline: Option<DateTime<Utc>>,
}

/// A task contract between agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub task_id: Uuid,
    pub requester: AgentId,
    pub capability: String,
    pub description: String,
    pub input: serde_json::Value,
    pub constraints: TaskConstraints,
    pub verification: Verification,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
}
