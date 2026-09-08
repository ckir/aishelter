use serde::{Deserialize, Serialize};
use crate::Capability;

/// Agent Card (matches agent.schema.json and OpenAPI AgentCard schema)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentCard {
    pub schema_version: String,
    pub agent_id: String,
    pub name: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity: Option<AgentIdentity>,
    pub capabilities: Vec<Capability>,
    pub protocols: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub endpoints: Option<AgentEndpoints>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub availability: Option<Availability>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub economics: Option<AgentEconomics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentIdentity {
    pub algorithm: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub key_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentEndpoints {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tasks: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub messages: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openapi: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Availability {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct AgentEconomics {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payment_required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub currency: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<String>,
}
