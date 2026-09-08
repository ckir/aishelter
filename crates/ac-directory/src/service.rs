use ac_db::pool::SharedPool;
use ac_manifest::validation::{validate_service_manifest, validate_directory_manifest, ManifestValidationError};
use ac_types::error::AcError;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::{error, info, warn};

/// Directory-layer errors specific to service and directory registration.
#[derive(Debug, Error)]
pub enum ServiceError {
    /// The requested service or directory was not found.
    #[error("not found: {0}")]
    NotFound(String),

    /// The manifest JSON failed validation.
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),

    /// Fetching the manifest from the source URL failed.
    #[error("fetch error: {0}")]
    FetchError(String),

    /// A duplicate service or directory was registered.
    #[error("duplicate registration: {0}")]
    Duplicate(String),
}

impl From<ServiceError> for AcError {
    fn from(err: ServiceError) -> Self {
        match err {
            ServiceError::NotFound(msg) => AcError::AgentNotFound(msg),
            ServiceError::InvalidManifest(msg) => AcError::Database(format!("invalid manifest: {msg}")),
            ServiceError::FetchError(msg) => AcError::Internal(format!("fetch error: {msg}")),
            ServiceError::Duplicate(msg) => AcError::DuplicateRegistration(msg),
        }
    }
}

/// Database row for a registered service manifest.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ServiceRow {
    pub service_id: String,
    pub manifest_url: String,
    pub manifest_json: String,
    pub manifest_sha256: String,
    pub schema_version: String,
    pub status: String,
    pub fetched_at: Option<chrono::DateTime<Utc>>,
    pub updated_at: chrono::DateTime<Utc>,
    pub created_at: chrono::DateTime<Utc>,
}

/// Database row for a registered directory manifest.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct DirectoryRow {
    pub directory_id: String,
    pub manifest_url: String,
    pub manifest_json: String,
    pub manifest_sha256: String,
    pub schema_version: String,
    pub status: String,
    pub updated_at: chrono::DateTime<Utc>,
    pub created_at: chrono::DateTime<Utc>,
}

/// Service layer for service and directory registration.
pub struct DirectoryService {
    pool: sqlx::PgPool,
}

impl DirectoryService {
    /// Create a new directory service with the given database pool.
    pub fn new(pool: sqlx::PgPool) -> Self {
        Self { pool }
    }

    /// Register a new service manifest.
    ///
    /// Validates the manifest JSON, computes the SHA-256 digest, and
    /// inserts a row into the `services` table.  Returns the stored row.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError::InvalidManifest`] if validation fails,
    /// or [`ServiceError::Duplicate`] if the service_id is already registered.
    pub async fn register_service(
        &self,
        service_id: &str,
        manifest_url: &str,
        manifest_json: &str,
    ) -> Result<ServiceRow, ServiceError> {
        info!(service_id = %service_id, "registering service");

        // Validate the manifest JSON.
        validate_service_manifest(manifest_json).map_err(|e| {
            ServiceError::InvalidManifest(format!("schema validation failed: {e:?}"))
        })?;

        // Compute SHA-256 of the raw manifest.
        let manifest_sha256 = format!("{:x}", sha2::Sha256::digest(manifest_json.as_bytes()));

        // Parse the schema_version from the JSON for indexing.
        let schema_version = extract_schema_version(manifest_json)?;

        let now = Utc::now();

        let row = sqlx::query_as::<_, ServiceRow>(
            r#"
            INSERT INTO services
                (service_id, manifest_url, manifest_json, manifest_sha256, schema_version, status, fetched_at, updated_at, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING *
            "#,
        )
        .bind(service_id)
        .bind(manifest_url)
        .bind(manifest_json)
        .bind(&manifest_sha256)
        .bind(&schema_version)
        .bind("ACTIVE")
        .bind(now)
        .bind(now)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, service_id = %service_id, "failed to insert service");
            ServiceError::Duplicate(format!("service {service_id} already registered: {e}"))
        })?
        .ok_or_else(|| {
            ServiceError::Duplicate(format!("service {service_id} already registered"))
        })?;

        info!(service_id = %service_id, "service registered successfully");
        Ok(row)
    }

    /// Get a service manifest by ID.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError::NotFound`] if the service does not exist.
    pub async fn get_service(&self, service_id: &str) -> Result<ServiceRow, ServiceError> {
        info!(service_id = %service_id, "looking up service");

        let row = sqlx::query_as::<_, ServiceRow>(
            r#"
            SELECT service_id, manifest_url, manifest_json, manifest_sha256,
                   schema_version, status, fetched_at, updated_at, created_at
            FROM services
            WHERE service_id = $1
            "#,
        )
        .bind(service_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, service_id = %service_id, "database error looking up service");
            ServiceError::NotFound(format!("service {service_id}: {e}"))
        })?
        .ok_or_else(|| ServiceError::NotFound(format!("service {service_id}")))?;

        Ok(row)
    }

    /// Refresh a service manifest by re-fetching from its source URL.
    ///
    /// Updates the `manifest_json`, `manifest_sha256`, and `fetched_at`
    /// columns with the fresh content.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError::NotFound`] if the service does not exist,
    /// or [`ServiceError::FetchError`] if the HTTP fetch fails.
    pub async fn refresh_service(&self, service_id: &str) -> Result<ServiceRow, ServiceError> {
        info!(service_id = %service_id, "refreshing service manifest");

        // Look up the existing service to get its manifest_url.
        let existing = self.get_service(service_id).await?;

        // Fetch the manifest from the source URL.
        let client = reqwest::Client::new();
        let response = client
            .get(&existing.manifest_url)
            .send()
            .await
            .map_err(|e| {
                error!(error = %e, url = %existing.manifest_url, "failed to fetch manifest");
                ServiceError::FetchError(format!("failed to fetch {}: {e}", existing.manifest_url))
            })?;

        if !response.status().is_success() {
            warn!(
                status = %response.status(),
                url = %existing.manifest_url,
                "manifest fetch returned non-success status"
            );
            return Err(ServiceError::FetchError(format!(
                "manifest at {} returned HTTP {}",
                existing.manifest_url,
                response.status()
            )));
        }

        let manifest_json = response.text().await.map_err(|e| {
            error!(error = %e, url = %existing.manifest_url, "failed to read manifest body");
            ServiceError::FetchError(format!("failed to read manifest body: {e}"))
        })?;

        // Validate the fresh manifest.
        validate_service_manifest(&manifest_json).map_err(|e| {
            ServiceError::InvalidManifest(format!("schema validation failed: {e:?}"))
        })?;

        let manifest_sha256 = format!("{:x}", sha2::Sha256::digest(manifest_json.as_bytes()));
        let schema_version = extract_schema_version(&manifest_json)?;
        let now = Utc::now();

        let row = sqlx::query_as::<_, ServiceRow>(
            r#"
            UPDATE services
            SET manifest_json = $2,
                manifest_sha256 = $3,
                schema_version = $4,
                fetched_at = $5,
                updated_at = $6
            WHERE service_id = $1
            RETURNING *
            "#,
        )
        .bind(service_id)
        .bind(&manifest_json)
        .bind(&manifest_sha256)
        .bind(&schema_version)
        .bind(now)
        .bind(now)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, service_id = %service_id, "failed to update service after refresh");
            ServiceError::FetchError(format!("failed to persist refreshed manifest: {e}"))
        })?;

        info!(service_id = %service_id, "service manifest refreshed");
        Ok(row)
    }

    /// List all registered services.
    ///
    /// Returns rows ordered by `created_at` descending (newest first).
    pub async fn list_services(&self) -> Result<Vec<ServiceRow>, ServiceError> {
        info!("listing all services");

        let rows = sqlx::query_as::<_, ServiceRow>(
            r#"
            SELECT service_id, manifest_url, manifest_json, manifest_sha256,
                   schema_version, status, fetched_at, updated_at, created_at
            FROM services
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, "database error listing services");
            ServiceError::FetchError(format!("failed to list services: {e}"))
        })?;

        info!(count = rows.len(), "services listed");
        Ok(rows)
    }

    /// Register a new directory manifest.
    ///
    /// # Errors
    ///
    /// Returns [`ServiceError::InvalidManifest`] if validation fails,
    /// or [`ServiceError::Duplicate`] if the directory_id is already registered.
    pub async fn register_directory(
        &self,
        directory_id: &str,
        manifest_url: &str,
        manifest_json: &str,
    ) -> Result<DirectoryRow, ServiceError> {
        info!(directory_id = %directory_id, "registering directory");

        validate_directory_manifest(manifest_json).map_err(|e| {
            ServiceError::InvalidManifest(format!("schema validation failed: {e:?}"))
        })?;

        let manifest_sha256 = format!("{:x}", sha2::Sha256::digest(manifest_json.as_bytes()));
        let schema_version = extract_schema_version(manifest_json)?;
        let now = Utc::now();

        let row = sqlx::query_as::<_, DirectoryRow>(
            r#"
            INSERT INTO directories
                (directory_id, manifest_url, manifest_json, manifest_sha256, schema_version, status, updated_at, created_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
            RETURNING *
            "#,
        )
        .bind(directory_id)
        .bind(manifest_url)
        .bind(manifest_json)
        .bind(&manifest_sha256)
        .bind(&schema_version)
        .bind("ACTIVE")
        .bind(now)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, directory_id = %directory_id, "failed to insert directory");
            ServiceError::Duplicate(format!("directory {directory_id} already registered: {e}"))
        })?
        .ok_or_else(|| ServiceError::Duplicate(format!("directory {directory_id} already registered")))?;

        info!(directory_id = %directory_id, "directory registered successfully");
        Ok(row)
    }

    /// List all registered directories.
    ///
    /// Returns rows ordered by `created_at` descending (newest first).
    pub async fn list_directories(&self) -> Result<Vec<DirectoryRow>, ServiceError> {
        info!("listing all directories");

        let rows = sqlx::query_as::<_, DirectoryRow>(
            r#"
            SELECT directory_id, manifest_url, manifest_json, manifest_sha256,
                   schema_version, status, updated_at, created_at
            FROM directories
            ORDER BY created_at DESC
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, "database error listing directories");
            ServiceError::FetchError(format!("failed to list directories: {e}"))
        })?;

        info!(count = rows.len(), "directories listed");
        Ok(rows)
    }
}

/// Extract the `schema_version` field from a JSON string.
fn extract_schema_version(json: &str) -> Result<String, ServiceError> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| ServiceError::InvalidManifest(e.to_string()))?;
    value
        .get("schema_version")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| ServiceError::InvalidManifest("missing field: schema_version".to_string()))
}
