//! Service Manifest type for `/.well-known/agent-service.json`.
//!
//! A Service Manifest describes a concrete service endpoint: its URL,
//! schema version, capabilities, and authentication requirements.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::Capability;

/// Service Manifest — describes a discoverable service endpoint.
///
/// Published at `/.well-known/agent-service.json` and optionally stored
/// in the database for directory-level caching and validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceManifest {
    /// Unique service identifier (e.g. `"svc_<UUID>"`).
    pub service_id: String,
    /// URL where the full manifest document can be fetched.
    pub manifest_url: url::Url,
    /// Schema version string (e.g. `"1.0"`).
    pub schema_version: String,
    /// Human-readable service name.
    pub name: String,
    /// Longer description of the service's purpose.
    pub description: String,
    /// List of capabilities this service exposes.
    pub capabilities: Vec<Capability>,
    /// Supported protocol identifiers (e.g. `"acp/1"`).
    pub protocols: Vec<String>,
    /// Whether this service requires authentication.
    pub requires_auth: bool,
    /// URL of the owner/agent that owns this service.
    pub owner_url: Option<url::Url>,
    /// Timestamp when the manifest was last fetched from the source URL.
    pub fetched_at: Option<DateTime<Utc>>,
    /// Timestamp when this record was last updated in the directory.
    pub updated_at: Option<DateTime<Utc>>,
}
