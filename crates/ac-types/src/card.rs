use crate::agent::AgentId;
use serde::{Deserialize, Serialize};

/// A declared capability with a version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    pub name: String,
    pub version: String,
}

/// Agent availability mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Availability {
    pub mode: String,
}

/// Agent pricing declaration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pricing {
    pub currency: String,
}

/// Agent Card — a public declaration of an agent's capabilities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    pub agent_id: AgentId,
    pub name: String,
    pub description: String,
    pub version: String,
    pub capabilities: Vec<Capability>,
    pub protocols: Vec<String>,
    pub availability: Availability,
    pub pricing: Pricing,
}
