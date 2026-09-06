//! Agent identity and lifecycle state.
//!
//! Defines the [`AgentId`] wrapper type for globally unique agent identifiers
//! and the [`AgentStatus`] enum that tracks an agent's position in its
//! lifecycle from initial registration through retirement.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Globally unique agent identifier.
///
/// Each agent in the commons is identified by a string of the form
/// `"agent_<UUID>"`.  This newtype wraps a plain [`String`] to prevent
/// accidentally passing arbitrary strings where an agent ID is expected.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentId(
    /// The underlying identifier string.
    pub String,
);

impl AgentId {
    /// Create a new random agent ID using a UUID v4.
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
///
/// An agent progresses through these states as it registers, publishes its
/// capabilities, accepts tasks, builds reputation, and eventually retires.
/// The default state for a newly-inserted agent row is [`Registered`].
///
/// [`Registered`]: AgentStatus::Registered
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AgentStatus {
    /// The agent record exists but has not completed registration.
    Unregistered,
    /// The agent is registered and has a valid public key on file.
    #[default]
    Registered,
    /// The agent is actively participating in the network.
    Active,
    /// The agent's card is published and it appears in discovery results.
    Discoverable,
    /// The agent is currently executing one or more tasks.
    Tasking,
    /// The agent is accumulating reputation from recently completed tasks.
    ReputationBuilding,
    /// The agent is temporarily blocked from accepting new work.
    Suspended,
    /// The agent's key has been revoked due to a security incident.
    Revoked,
    /// The agent has voluntarily exited the network.
    Retired,
}

/// Core agent record stored in the database.
///
/// Each row represents one agent identity with its Ed25519 public key,
/// optional human-readable profile, current [`AgentStatus`], and audit
/// timestamps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    /// Unique agent identifier.
    pub agent_id: AgentId,
    /// Ed25519 public key in hex-encoded form.
    pub public_key: String,
    /// Optional human-readable name.
    pub profile_name: Option<String>,
    /// Optional description of the agent's purpose.
    pub profile_description: Option<String>,
    /// Current lifecycle state.
    pub status: AgentStatus,
    /// When the agent record was created.
    pub created_at: DateTime<Utc>,
    /// When the agent record was last modified.
    pub updated_at: DateTime<Utc>,
}
