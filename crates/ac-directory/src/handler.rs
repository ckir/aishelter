//! HTTP handlers for service and directory registration.
//!
//! Exposes REST endpoints for managing service manifests and directory
//! manifests, plus a capabilities summary endpoint.

use ac_db::pool::SharedPool;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};

use crate::service::DirectoryService;

/// Build the directory router with all service and directory routes.
pub fn routes(pool: SharedPool) -> Router {
    Router::new()
        // Service routes
        .route("/v1/services", post(register_service_handler))
        .route("/v1/services", get(list_services_handler))
        .route("/v1/services/{id}", get(get_service_handler))
        .route("/v1/services/{id}/refresh", put(refresh_service_handler))
        // Directory routes
        .route("/v1/directories", post(register_directory_handler))
        .route("/v1/directories", get(list_directories_handler))
        // Capabilities route
        .route("/v1/capabilities", get(get_capabilities_handler))
        .with_state(pool)
}

// ── Request / Response types ────────────────────────────────────────────

/// Request body for `POST /v1/services`.
#[derive(Deserialize)]
pub struct RegisterServiceRequest {
    /// Unique service identifier.
    pub service_id: String,
    /// URL where the full manifest can be fetched.
    pub manifest_url: String,
    /// Raw JSON of the service manifest.
    pub manifest_json: String,
}

/// Response body for service registration.
#[derive(Serialize)]
pub struct RegisterServiceResponse {
    /// The registered service ID.
    pub service_id: String,
    /// Current lifecycle status.
    pub status: String,
    /// Protocol version identifier.
    pub protocol: String,
}

/// Request body for `POST /v1/directories`.
#[derive(Deserialize)]
pub struct RegisterDirectoryRequest {
    /// Unique directory identifier.
    pub directory_id: String,
    /// URL where the full manifest can be fetched.
    pub manifest_url: String,
    /// Raw JSON of the directory manifest.
    pub manifest_json: String,
}

/// Response body for directory registration.
#[derive(Serialize)]
pub struct RegisterDirectoryResponse {
    /// The registered directory ID.
    pub directory_id: String,
    /// Current lifecycle status.
    pub status: String,
    /// Protocol version identifier.
    pub protocol: String,
}

/// Summary response for a single service (omits large manifest_json).
#[derive(Serialize)]
pub struct ServiceSummary {
    pub service_id: String,
    pub manifest_url: String,
    pub manifest_sha256: String,
    pub schema_version: String,
    pub status: String,
    pub fetched_at: Option<chrono::DateTime<chrono::Utc>>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Summary response for a single directory (omits large manifest_json).
#[derive(Serialize)]
pub struct DirectorySummary {
    pub directory_id: String,
    pub manifest_url: String,
    pub manifest_sha256: String,
    pub schema_version: String,
    pub status: String,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

/// Capability entry returned by `GET /v1/capabilities`.
#[derive(Serialize)]
pub struct CapabilityEntry {
    /// Service that offers this capability.
    pub service_id: String,
    /// Capability name.
    pub name: String,
    /// Capability version string.
    pub version: String,
}

/// Response body for `GET /v1/capabilities`.
#[derive(Serialize)]
pub struct CapabilitiesResponse {
    /// All capabilities advertised by registered services.
    pub capabilities: Vec<CapabilityEntry>,
    /// Protocol version identifier.
    pub protocol: String,
}

// ── Service handlers ────────────────────────────────────────────────────

/// Register a new service manifest.
///
/// Validates and stores the provided manifest JSON.  Returns HTTP 409 if
/// the service_id is already registered.
pub async fn register_service_handler(
    State(pool): State<SharedPool>,
    Json(req): Json<RegisterServiceRequest>,
) -> Result<Json<RegisterServiceResponse>, ac_types::error::AcError> {
    let pool = pool.load();
    let service = DirectoryService::new(pool);
    let row =
        service.register_service(&req.service_id, &req.manifest_url, &req.manifest_json).await?;

    Ok(Json(RegisterServiceResponse {
        service_id: row.service_id,
        status: row.status,
        protocol: "acp/1".to_string(),
    }))
}

/// Get a service manifest by ID.
///
/// Returns the full manifest JSON along with metadata.
/// Returns HTTP 404 if the service does not exist.
pub async fn get_service_handler(
    State(pool): State<SharedPool>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ac_types::error::AcError> {
    let pool = pool.load();
    let service = DirectoryService::new(pool);
    let row = service.get_service(&id).await?;

    Ok(Json(serde_json::json!({
        "service_id": row.service_id,
        "manifest_url": row.manifest_url,
        "manifest_json": serde_json::from_str::<serde_json::Value>(&row.manifest_json).unwrap_or(serde_json::Value::Null),
        "manifest_sha256": row.manifest_sha256,
        "schema_version": row.schema_version,
        "status": row.status,
        "fetched_at": row.fetched_at,
        "updated_at": row.updated_at,
        "protocol": "acp/1",
    })))
}

/// Refresh a service manifest by re-fetching from its source URL.
///
/// Returns HTTP 404 if the service does not exist, or HTTP 502 if the
/// source URL cannot be reached.
pub async fn refresh_service_handler(
    State(pool): State<SharedPool>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, ac_types::error::AcError> {
    let pool = pool.load();
    let service = DirectoryService::new(pool);
    let row = service.refresh_service(&id).await?;

    Ok(Json(serde_json::json!({
        "service_id": row.service_id,
        "status": "refreshed",
        "fetched_at": row.fetched_at,
        "protocol": "acp/1",
    })))
}

/// List all registered services.
///
/// Returns a summary of each service (without the full manifest JSON).
pub async fn list_services_handler(
    State(pool): State<SharedPool>,
) -> Result<Json<serde_json::Value>, ac_types::error::AcError> {
    let pool = pool.load();
    let service = DirectoryService::new(pool);
    let rows = service.list_services().await?;

    let summaries: Vec<ServiceSummary> = rows
        .into_iter()
        .map(|r| ServiceSummary {
            service_id: r.service_id,
            manifest_url: r.manifest_url,
            manifest_sha256: r.manifest_sha256,
            schema_version: r.schema_version,
            status: r.status,
            fetched_at: r.fetched_at,
            updated_at: r.updated_at,
        })
        .collect();

    Ok(Json(serde_json::json!({
        "services": summaries,
        "count": summaries.len(),
        "protocol": "acp/1",
    })))
}

// ── Directory handlers ──────────────────────────────────────────────────

/// Register a new directory manifest.
///
/// Validates and stores the provided manifest JSON.  Returns HTTP 409 if
/// the directory_id is already registered.
pub async fn register_directory_handler(
    State(pool): State<SharedPool>,
    Json(req): Json<RegisterDirectoryRequest>,
) -> Result<Json<RegisterDirectoryResponse>, ac_types::error::AcError> {
    let pool = pool.load();
    let service = DirectoryService::new(pool);
    let row = service
        .register_directory(&req.directory_id, &req.manifest_url, &req.manifest_json)
        .await?;

    Ok(Json(RegisterDirectoryResponse {
        directory_id: row.directory_id,
        status: row.status,
        protocol: "acp/1".to_string(),
    }))
}

/// List all registered directories.
///
/// Returns a summary of each directory (without the full manifest JSON).
pub async fn list_directories_handler(
    State(pool): State<SharedPool>,
) -> Result<Json<serde_json::Value>, ac_types::error::AcError> {
    let pool = pool.load();
    let service = DirectoryService::new(pool);
    let rows = service.list_directories().await?;

    let summaries: Vec<DirectorySummary> = rows
        .into_iter()
        .map(|r| DirectorySummary {
            directory_id: r.directory_id,
            manifest_url: r.manifest_url,
            manifest_sha256: r.manifest_sha256,
            schema_version: r.schema_version,
            status: r.status,
            updated_at: r.updated_at,
        })
        .collect();

    Ok(Json(serde_json::json!({
        "directories": summaries,
        "count": summaries.len(),
        "protocol": "acp/1",
    })))
}

// ── Capabilities handler ────────────────────────────────────────────────

/// Get all capabilities advertised by registered services.
///
/// Parses the `manifest_json` of each active service to extract its
/// capabilities array and flattens them into a single list.
pub async fn get_capabilities_handler(
    State(pool): State<SharedPool>,
) -> Result<Json<CapabilitiesResponse>, ac_types::error::AcError> {
    let pool = pool.load();
    let service = DirectoryService::new(pool);
    let rows = service.list_services().await?;

    let mut capabilities = Vec::new();

    for row in &rows {
        if let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&row.manifest_json) {
            if let Some(caps) = manifest.get("capabilities").and_then(|v| v.as_array()) {
                for cap in caps {
                    // Try the modern name/version format first, then fall back to id-based format
                    if let (Some(name), Some(version)) = (
                        cap.get("name").and_then(|v| v.as_str()),
                        cap.get("version").and_then(|v| v.as_str()),
                    ) {
                        capabilities.push(CapabilityEntry {
                            service_id: row.service_id.clone(),
                            name: name.to_string(),
                            version: version.to_string(),
                        });
                    } else if let Some(id) = cap.get("id").and_then(|v| v.as_str()) {
                        // Fallback: use id as name, empty version
                        capabilities.push(CapabilityEntry {
                            service_id: row.service_id.clone(),
                            name: id.to_string(),
                            version: cap
                                .get("version")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                        });
                    }
                }
            }
        }
    }

    Ok(Json(CapabilitiesResponse { capabilities, protocol: "acp/1".to_string() }))
}
