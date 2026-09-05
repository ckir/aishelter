use crate::agent::AgentId;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Signed receipt for a completed and verified task.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Receipt {
    pub receipt_id: Uuid,
    pub task_id: Uuid,
    pub agent_id: AgentId,
    pub input_hash: String,
    pub output_hash: String,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub status: String,
}

/// Verified Work Unit — evidence of useful work.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VWU {
    pub agent_id: AgentId,
    pub task_id: Uuid,
    pub value: u64,
    pub created_at: DateTime<Utc>,
}
