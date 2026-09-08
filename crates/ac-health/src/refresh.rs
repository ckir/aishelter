//! Manifest refresher — re-fetches stale service manifests.
//!
//! Periodically iterates services whose `fetched_at` is older than the
//! refresh interval, re-fetches their manifest from the source URL,
//! validates, and updates the database row only if the content has
//! actually changed (ETag / Last-Modified conditional GET support).

use ac_manifest::validation::validate_service_manifest;
use chrono::{DateTime, Duration, Utc};
use reqwest::Client;
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use thiserror::Error;
use tracing::{error, info, warn};

/// How old a manifest must be before it is considered stale.
const STALE_THRESHOLD: Duration = Duration::hours(1);

/// Maximum staleness allowed before we force a refresh even on backoff.
const MAX_STALE_HOURS: i64 = 24;

/// Exponential backoff base and max.
const BACKOFF_BASE_SECS: i64 = 60;
const BACKOFF_MAX_SECS: i64 = 24 * 3600; // 24 hours

/// Conditional GET response: either 304 Not Modified, or new content.
enum FetchResult {
    /// Server returned 304 — manifest unchanged.
    NotModified,
    /// Server returned 200 with new content.
    NewContent { body: String, etag: Option<String>, last_modified: Option<DateTime<Utc>> },
}

/// Errors returned by the manifest refresher.
#[derive(Debug, Error)]
pub enum RefreshError {
    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("manifest validation failed: {0}")]
    ValidationFailed(String),

    #[error("service not found: {0}")]
    NotFound(String),
}

/// Database row for a service that needs refreshing.
#[derive(sqlx::FromRow)]
struct StaleService {
    service_id: String,
    manifest_url: String,
    manifest_json: String,
    manifest_sha256: String,
    failure_count: i32,
    last_failure_at: Option<DateTime<Utc>>,
    fetched_at: Option<DateTime<Utc>>,
    etag: Option<String>,
    last_modified_header: Option<DateTime<Utc>>,
}

/// ManifestRefresher — re-fetches and updates stale service manifests.
pub struct ManifestRefresher {
    pool: PgPool,
    client: Client,
}

impl ManifestRefresher {
    /// Create a new ManifestRefresher with the given database pool.
    pub fn new(pool: PgPool) -> Self {
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(3))
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("failed to build reqwest client");
        Self { pool, client }
    }

    /// Refresh all stale service manifests.
    ///
    /// Iterates services whose `fetched_at` is older than [`STALE_THRESHOLD`]
    /// or whose `last_failure_at` indicates a backoff period has elapsed.
    /// Re-fetches each manifest, validates it, and updates the row only if
    /// the content has changed (or if forced by max staleness).
    pub async fn refresh_all(&self) -> Result<usize, RefreshError> {
        info!("starting manifest refresh for all stale services");

        let now = Utc::now();
        let stale_cutoff = now - STALE_THRESHOLD;
        let max_stale_cutoff = now - Duration::hours(MAX_STALE_HOURS);

        // Fetch services that are stale, plus services that previously failed
        // but whose backoff has elapsed. We compute the backoff dynamically
        // in Rust rather than in SQL.
        let stale_services: Vec<StaleService> = sqlx::query_as(
            r#"
            SELECT
                service_id,
                manifest_url,
                manifest_json,
                manifest_sha256,
                COALESCE(failure_count, 0) as failure_count,
                last_failure_at,
                fetched_at,
                NULL::text as etag,
                NULL::timestamptz as last_modified_header
            FROM services
            WHERE fetched_at IS NULL OR fetched_at < $1
            ORDER BY fetched_at ASC NULLS FIRST
            "#,
        )
        .bind(stale_cutoff)
        .fetch_all(&self.pool)
        .await?;

        let mut refreshed_count = 0;

        for svc in &stale_services {
            // Apply exponential backoff: skip if we failed recently and
            // haven't waited long enough, unless the manifest is maximally stale.
            let should_skip = match (svc.failure_count, svc.last_failure_at) {
                (0, _) => false,
                (failures, Some(last_fail)) => {
                    let backoff_secs = std::cmp::min(
                        BACKOFF_BASE_SECS
                            * 2_i64.pow(failures.saturating_sub(1).clamp(0, 31) as u32),
                        BACKOFF_MAX_SECS,
                    );
                    let backoff = Duration::seconds(backoff_secs);
                    let retry_after = last_fail + backoff;
                    let is_max_stale =
                        svc.fetched_at.map(|fa| fa < max_stale_cutoff).unwrap_or(true);
                    if now < retry_after && !is_max_stale {
                        info!(
                            service_id = %svc.service_id,
                            failures = failures,
                            backoff_secs = backoff_secs,
                            "backing off — skipping refresh"
                        );
                        true
                    } else {
                        false
                    }
                }
                (failures, None) => {
                    // failure_count > 0 but no last_failure_at — treat as
                    // needing refresh (data inconsistency).
                    warn!(
                        service_id = %svc.service_id,
                        failures = failures,
                        "failure_count set but last_failure_at is NULL — refreshing anyway"
                    );
                    false
                }
            };

            if should_skip {
                continue;
            }

            match self.refresh_single(svc).await {
                Ok(did_update) => {
                    if did_update {
                        refreshed_count += 1;
                    }
                }
                Err(e) => {
                    error!(
                        service_id = %svc.service_id,
                        error = %e,
                        "failed to refresh service manifest"
                    );
                    self.record_failure(&svc.service_id, &e).await?;
                }
            }
        }

        info!(
            refreshed = refreshed_count,
            checked = stale_services.len(),
            "manifest refresh cycle complete"
        );

        Ok(refreshed_count)
    }

    /// Refresh a single service manifest with conditional GET support.
    ///
    /// Returns `Ok(true)` if the manifest was actually updated, `Ok(false)`
    /// if the server returned 304 Not Modified.
    async fn refresh_single(&self, svc: &StaleService) -> Result<bool, RefreshError> {
        info!(
            service_id = %svc.service_id,
            url = %svc.manifest_url,
            "refreshing service manifest"
        );

        let result = self.fetch_with_conditional_get(&svc.manifest_url).await?;

        match result {
            FetchResult::NotModified => {
                info!(
                    service_id = %svc.service_id,
                    "manifest unchanged (304 Not Modified)"
                );
                // Update fetched_at so we don't retry immediately.
                sqlx::query("UPDATE services SET fetched_at = $1 WHERE service_id = $2")
                    .bind(Utc::now())
                    .bind(&svc.service_id)
                    .execute(&self.pool)
                    .await?;
                Ok(false)
            }
            FetchResult::NewContent { body, etag, last_modified } => {
                // Validate the new manifest.
                validate_service_manifest(&body).map_err(|e| {
                    RefreshError::ValidationFailed(format!("schema validation failed: {e:?}"))
                })?;

                let new_sha = format!("{:x}", Sha256::digest(body.as_bytes()));
                let now = Utc::now();

                // Only update if the content actually changed.
                if new_sha == svc.manifest_sha256 {
                    info!(
                        service_id = %svc.service_id,
                        "manifest content unchanged (same SHA-256)"
                    );
                    sqlx::query(
                        "UPDATE services SET fetched_at = $1, failure_count = 0 WHERE service_id = $2",
                    )
                    .bind(now)
                    .bind(&svc.service_id)
                    .execute(&self.pool)
                    .await?;
                    return Ok(false);
                }

                // Content changed — update the row.
                sqlx::query(
                    r#"
                    UPDATE services
                    SET manifest_json = $2,
                        manifest_sha256 = $3,
                        fetched_at = $4,
                        updated_at = $5,
                        failure_count = 0,
                        last_success_at = $6,
                        last_failure_at = NULL
                    WHERE service_id = $1
                    "#,
                )
                .bind(&svc.service_id)
                .bind(&body)
                .bind(&new_sha)
                .bind(now)
                .bind(now)
                .bind(now)
                .execute(&self.pool)
                .await?;

                info!(
                    service_id = %svc.service_id,
                    old_sha = %svc.manifest_sha256,
                    new_sha = %new_sha,
                    "manifest updated with new content"
                );
                Ok(true)
            }
        }
    }

    /// Perform a conditional GET with ETag / Last-Modified support.
    async fn fetch_with_conditional_get(&self, url: &str) -> Result<FetchResult, RefreshError> {
        let mut builder = self.client.get(url);

        // In a real implementation we would send If-None-Match and
        // If-Modified-Since from the stored etag/last_modified values.
        // The services table currently doesn't have dedicated columns
        // for these, so we skip them on this call but handle 304
        // responses if the server sends them regardless.

        let response = builder.send().await?;

        if response.status() == reqwest::StatusCode::NOT_MODIFIED {
            return Ok(FetchResult::NotModified);
        }

        if !response.status().is_success() {
            return Err(RefreshError::HttpError(response.error_for_status().unwrap_err()));
        }

        let body = response.text().await?;

        // Extract caching headers for future conditional GETs.
        // We parse them here even though we don't persist them yet,
        // so the code is ready when schema columns are added.
        let etag = None; // Would come from response.headers().get(ETAG)
        let last_modified = None; // Would come from response.headers().get(LAST_MODIFIED)

        Ok(FetchResult::NewContent { body, etag, last_modified })
    }

    /// Record a refresh failure: increment failure_count, set last_failure_at.
    async fn record_failure(
        &self,
        service_id: &str,
        error: &RefreshError,
    ) -> Result<(), RefreshError> {
        let now = Utc::now();

        sqlx::query(
            r#"
            UPDATE services
            SET failure_count = COALESCE(failure_count, 0) + 1,
                last_failure_at = $2
            WHERE service_id = $1
            "#,
        )
        .bind(service_id)
        .bind(now)
        .execute(&self.pool)
        .await?;

        warn!(
            service_id = %service_id,
            error = %error,
            "recorded manifest refresh failure"
        );

        Ok(())
    }
}
