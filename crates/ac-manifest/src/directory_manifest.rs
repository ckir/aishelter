//! Directory Manifest type for `/.well-known/agent-directory.json`.
//!
//! A Directory Manifest describes a service directory that aggregates
//! and indexes other agents and services for discovery.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Directory Manifest — describes a service directory.
///
/// Published at `/.well-known/agent-directory.json` so that agents
/// can discover directories and directories can federate with each other.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectoryManifest {
    /// Unique directory identifier (e.g. `"dir_<UUID>"`).
    pub directory_id: String,
    /// URL where the full manifest document can be fetched.
    pub manifest_url: url::Url,
    /// Schema version string (e.g. `"1.0"`).
    pub schema_version: String,
    /// Human-readable directory name.
    pub name: String,
    /// Longer description of the directory's scope.
    pub description: String,
    /// URL of the operator that runs this directory.
    pub operator_url: Option<url::Url>,
    /// List of protocol identifiers this directory supports.
    pub protocols: Vec<String>,
    /// Whether this directory is open for new registrations.
    pub open_registration: bool,
    /// Timestamp when this record was last updated.
    pub updated_at: Option<DateTime<Utc>>,
}
