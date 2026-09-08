use ac_db::pool::SharedPool;
use ac_manifest::validation::validate_service_manifest;
use ac_types::error::AcError;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tracing::{error, info, warn};

#[derive(Debug, thiserror::Error)]
pub enum ServiceError {
    #[error("not found: {0}")]
    NotFound(String),
    #[error("invalid manifest: {0}")]
    InvalidManifest(String),
    #[error("fetch error: {0}")]
    FetchError(String),
    #[error("duplicate: {0}")]
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

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct ServiceManifestRow {
    pub service_id: String,
    pub manifest_url: String,
    pub manifest_json: serde_json::Value,
    pub manifest_sha256: String,
    pub schema_version: String,
    pub status: String,
    pub fetched_at: Option<chrono::DateTime<Utc>>,
    pub updated_at: chrono::DateTime<Utc>,
    pub last_success_at: Option<chrono::DateTime<Utc>>,
    pub last_failure_at: Option<chrono::DateTime<Utc>>,
    pub failure_count: i32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ServiceHealth {
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_checked: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<i64>,
}

pub struct DirectoryService {
    pool: SharedPool,
}

impl DirectoryService {
    pub fn new(pool: SharedPool) -> Self {
        Self { pool }
    }

    pub async fn register_service(
        &self,
        manifest_url: &str,
    ) -> Result<(String, String), ServiceError> {
        info!(url = %manifest_url, "registering service");

        let client = reqwest::Client::new();
        let response = client.get(manifest_url).send().await.map_err(|e| {
            error!(error = %e, url = %manifest_url, "failed to fetch manifest");
            ServiceError::FetchError(format!("failed to fetch {}: {e}", manifest_url))
        })?;

        if !response.status().is_success() {
            return Err(ServiceError::FetchError(format!(
                "manifest at {} returned {}", manifest_url, response.status()
            )));
        }

        let manifest_json = response.text().await.map_err(|e| {
            ServiceError::FetchError(format!("failed to read body: {e}"))
        })?;

        validate_service_manifest(&manifest_json).map_err(|e| {
            ServiceError::InvalidManifest(format!("validation failed: {e:?}"))
        })?;

        let manifest_value: serde_json::Value = serde_json::from_str(&manifest_json)
            .map_err(|e| ServiceError::InvalidManifest(format!("parse error: {e}")))?;

        let service_id = manifest_value.get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ServiceError::InvalidManifest("missing id field".to_string()))?
            .to_string();

        let schema_version = manifest_value.get("schema_version")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ServiceError::InvalidManifest("missing schema_version".to_string()))?
            .to_string();

        let manifest_sha256 = hex::encode(Sha256::digest(manifest_json.as_bytes()));

        sqlx::query!(
            r#"
            INSERT INTO service_manifests (service_id, manifest_url, manifest_json, manifest_sha256, schema_version)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (service_id) DO UPDATE SET
                manifest_url = EXCLUDED.manifest_url,
                manifest_json = EXCLUDED.manifest_json,
                manifest_sha256 = EXCLUDED.manifest_sha256,
                schema_version = EXCLUDED.schema_version,
                updated_at = NOW(),
                fetched_at = NOW(),
                failure_count = 0
            "#,
            service_id,
            manifest_url,
            manifest_json as _,
            manifest_sha256,
            schema_version,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| {
            error!(error = %e, service_id = %service_id, "failed to register service");
            ServiceError::FetchError(format!("database error: {e}"))
        })?;

        info!(service_id = %service_id, "service registered");
        Ok((service_id, "ACTIVE".to_string()))
    }

    pub async fn get_service(&self, service_id: &str) -> Result<ServiceManifestRow, ServiceError> {
        let row = sqlx::query_as!(
            ServiceManifestRow,
            r#"
            SELECT service_id, manifest_url, manifest_json as "manifest_json: serde_json::Value",
                   manifest_sha256, schema_version, status, fetched_at, updated_at,
                   last_success_at, last_failure_at, failure_count
            FROM service_manifests
            WHERE service_id = $1
            "#,
            service_id,
        )
        .fetch_optional(&*self.pool)
        .await
        .map_err(|e| ServiceError::FetchError(format!("database error: {e}")))?;

        row.ok_or_else(|| ServiceError::NotFound(service_id.to_string()))
    }

    pub async fn refresh_service(&self, service_id: &str) -> Result<(), ServiceError> {
        let existing = self.get_service(service_id).await?;

        let client = reqwest::Client::new();
        let response = client.get(&existing.manifest_url).send().await.map_err(|e| {
            ServiceError::FetchError(format!("failed to fetch {}: {e}", existing.manifest_url))
        })?;

        if !response.status().is_success() {
            sqlx::query!(
                "UPDATE service_manifests SET last_failure_at = NOW(), failure_count = failure_count + 1 WHERE service_id = $1",
                service_id,
            )
            .execute(&*self.pool)
            .await
            .map_err(|e| ServiceError::FetchError(format!("database error: {e}")))?;

            return Err(ServiceError::FetchError(format!("HTTP {}", response.status())));
        }

        let manifest_json = response.text().await.map_err(|e| {
            ServiceError::FetchError(format!("failed to read body: {e}"))
        })?;

        validate_service_manifest(&manifest_json).map_err(|e| {
            ServiceError::InvalidManifest(format!("validation failed: {e:?}"))
        })?;

        let manifest_sha256 = hex::encode(Sha256::digest(manifest_json.as_bytes()));

        sqlx::query!(
            r#"
            UPDATE service_manifests
            SET manifest_json = $2, manifest_sha256 = $3, updated_at = NOW(),
                fetched_at = NOW(), last_success_at = NOW(), failure_count = 0
            WHERE service_id = $1
            "#,
            service_id,
            manifest_json as _,
            manifest_sha256,
        )
        .execute(&*self.pool)
        .await
        .map_err(|e| ServiceError::FetchError(format!("database error: {e}")))?;

        Ok(())
    }

    pub async fn get_service_health(&self, service_id: &str) -> Result<ServiceHealth, ServiceError> {
        let row = self.get_service(service_id).await?;

        let status = if row.failure_count > 3 {
            "degraded"
        } else if row.status == "ACTIVE" {
            "online"
        } else {
            "unknown"
        };

        Ok(ServiceHealth {
            status: status.to_string(),
            last_checked: row.fetched_at.map(|t| t.to_rfc3339()),
            latency_ms: None,
        })
    }
}
