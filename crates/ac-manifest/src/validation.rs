//! Manifest validation helpers using JSON Schema validation.

use thiserror::Error;

/// Errors that can occur during manifest validation.
#[derive(Debug, Error)]
pub enum ManifestValidationError {
    /// The manifest JSON failed schema validation.
    #[error("schema validation failed: {0}")]
    SchemaViolation(String),

    /// The manifest could not be parsed as valid JSON.
    #[error("invalid JSON: {0}")]
    InvalidJson(String),

    /// A required field was missing from the manifest.
    #[error("missing required field: {0}")]
    MissingField(String),

    /// The manifest schema version is not supported.
    #[error("unsupported schema version: {0}")]
    UnsupportedVersion(String),
}

/// Validate a service manifest JSON string against the known schema.
///
/// # Arguments
///
/// * `json` — Raw JSON string of the service manifest.
///
/// # Errors
///
/// Returns [`ManifestValidationError`] if the JSON is invalid or fails
/// schema validation.
pub fn validate_service_manifest(json: &str) -> Result<(), ManifestValidationError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| {
        ManifestValidationError::InvalidJson(e.to_string())
    })?;

    let Some(obj) = value.as_object() else {
        return Err(ManifestValidationError::InvalidJson(
            "manifest must be a JSON object".to_string(),
        ));
    };

    if !obj.contains_key("service_id") {
        return Err(ManifestValidationError::MissingField(
            "service_id".to_string(),
        ));
    }
    if !obj.contains_key("manifest_url") {
        return Err(ManifestValidationError::MissingField(
            "manifest_url".to_string(),
        ));
    }
    if !obj.contains_key("schema_version") {
        return Err(ManifestValidationError::MissingField(
            "schema_version".to_string(),
        ));
    }

    let Some(version) = obj.get("schema_version").and_then(|v| v.as_str()) else {
        return Err(ManifestValidationError::MissingField(
            "schema_version".to_string(),
        ));
    };

    if version != "1.0" {
        return Err(ManifestValidationError::UnsupportedVersion(version.to_string()));
    }

    Ok(())
}

/// Validate a directory manifest JSON string against the known schema.
///
/// # Arguments
///
/// * `json` — Raw JSON string of the directory manifest.
///
/// # Errors
///
/// Returns [`ManifestValidationError`] if the JSON is invalid or fails
/// schema validation.
pub fn validate_directory_manifest(json: &str) -> Result<(), ManifestValidationError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| {
        ManifestValidationError::InvalidJson(e.to_string())
    })?;

    let Some(obj) = value.as_object() else {
        return Err(ManifestValidationError::InvalidJson(
            "manifest must be a JSON object".to_string(),
        ));
    };

    if !obj.contains_key("directory_id") {
        return Err(ManifestValidationError::MissingField(
            "directory_id".to_string(),
        ));
    }
    if !obj.contains_key("manifest_url") {
        return Err(ManifestValidationError::MissingField(
            "manifest_url".to_string(),
        ));
    }
    if !obj.contains_key("schema_version") {
        return Err(ManifestValidationError::MissingField(
            "schema_version".to_string(),
        ));
    }

    let Some(version) = obj.get("schema_version").and_then(|v| v.as_str()) else {
        return Err(ManifestValidationError::MissingField(
            "schema_version".to_string(),
        ));
    };

    if version != "1.0" {
        return Err(ManifestValidationError::UnsupportedVersion(version.to_string()));
    }

    Ok(())
}

/// Validate an agent card JSON string against the known schema.
///
/// # Arguments
///
/// * `json` — Raw JSON string of the agent card.
///
/// # Errors
///
/// Returns [`ManifestValidationError`] if the JSON is invalid or fails
/// schema validation.
pub fn validate_agent_card(json: &str) -> Result<(), ManifestValidationError> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| {
        ManifestValidationError::InvalidJson(e.to_string())
    })?;

    let Some(obj) = value.as_object() else {
        return Err(ManifestValidationError::InvalidJson(
            "agent card must be a JSON object".to_string(),
        ));
    };

    if !obj.contains_key("agent_id") {
        return Err(ManifestValidationError::MissingField(
            "agent_id".to_string(),
        ));
    }
    if !obj.contains_key("name") {
        return Err(ManifestValidationError::MissingField("name".to_string()));
    }
    if !obj.contains_key("version") {
        return Err(ManifestValidationError::MissingField("version".to_string()));
    }

    Ok(())
}
