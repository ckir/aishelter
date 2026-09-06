//! Capability advertisement (Agent Card).
//!
//! An Agent Card is a self-declared advertisement of what an agent can do.
//! It lists the agent's capabilities (§14), availability mode, and pricing
//! model so that other agents can discover and negotiate with it.

use crate::agent::AgentId;
use serde::{Deserialize, Serialize};

/// A declared capability with a version string.
///
/// Each capability represents a specific skill the agent claims to possess,
/// such as `"fact_verification"` or `"code_review"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capability {
    /// Human-readable capability name.
    pub name: String,
    /// Semantic version of the capability implementation.
    pub version: String,
}

/// Agent availability mode declaration.
///
/// Indicates how the agent prefers to receive work:
/// `"direct"` for synchronous HTTP calls,
/// `"mailbox"` for asynchronous message-based delivery,
/// or `"both"`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Availability {
    /// The availability mode string.
    pub mode: String,
}

/// Agent pricing declaration.
///
/// Declares what currency (if any) the agent charges for its services.
/// Use `"none"` for free agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pricing {
    /// Currency code or `"none"`.
    pub currency: String,
}

/// Agent Card — a public declaration of an agent's capabilities.
///
/// This is the document that gets published to the registry so other agents
/// can discover this agent through capability-based search (§13).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentCard {
    /// The agent this card describes.
    pub agent_id: AgentId,
    /// Display name for the agent.
    pub name: String,
    /// Longer description of the agent's purpose.
    pub description: String,
    /// Card schema version.
    pub version: String,
    /// List of capabilities this agent claims to support.
    pub capabilities: Vec<Capability>,
    /// Supported protocol identifiers (e.g. `"acp/1"`).
    pub protocols: Vec<String>,
    /// How this agent prefers to receive work.
    pub availability: Availability,
    /// Pricing model for this agent's services.
    pub pricing: Pricing,
}
