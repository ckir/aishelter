use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use crate::agent::AgentId;

/// Dimensions of reputation scoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReputationDimension {
    Reliability,
    TaskSuccess,
    VerificationAccuracy,
    Responsiveness,
}

/// A single reputation event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationEvent {
    pub event_id: Uuid,
    pub agent_id: AgentId,
    pub r#type: String,
    pub task_id: Option<Uuid>,
    pub dimension: ReputationDimension,
    pub value: f64,
    pub timestamp: DateTime<Utc>,
}

/// Multidimensional reputation snapshot for an agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReputationSnapshot {
    pub agent_id: AgentId,
    pub reliability: f64,
    pub reliability_confidence: f64,
    pub task_success: f64,
    pub task_success_confidence: f64,
    pub verification_accuracy: f64,
    pub verification_accuracy_confidence: f64,
    pub responsiveness: f64,
    pub responsiveness_confidence: f64,
    pub vwu_total: u64,
}
