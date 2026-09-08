//! Service health checker.
//!
//! Periodically probes service manifest URLs to determine availability
//! and latency. Results are written back to the `services` table so
//! that discovery and reputation layers can filter on health status.

use chrono::{DateTime, Utc};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use thiserror::Error;
use tracing::{error, info, warn};

/// HTTP timeouts for health probes.
const CONNECT_TIMEOUT_SECS: u64 = 3;
const TOTAL_TIMEOUT_SECS: u64 = 10;

/// Latency thresholds (milliseconds) for status classification.
const ONLINE_THRESHOLD_MS: f64 = 500.0;
const DEGRADED_THRESHOLD_MS: f64 = 2000.0;

/// SSRF-safe: addresses that must never be probed.
const AWS_METADATA_IP: &str = "169.254.169.254";

/// Errors returned by the health checker.
#[derive(Debug, Error)]
pub enum HealthError {
    #[error("SSRF-blocked URL: {0}")]
    SsrfBlocked(String),

    #[error("HTTP error: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("invalid URL: {0}")]
    InvalidUrl(String),
}

/// Health status of a single service probe.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHealth {
    pub service_id: String,
    pub status: String,
    pub latency_ms: Option<f64>,
    pub last_checked: DateTime<Utc>,
}

impl ServiceHealth {
    fn classify(latency_ms: f64) -> &'static str {
        if latency_ms < ONLINE_THRESHOLD_MS {
            "online"
        } else if latency_ms < DEGRADED_THRESHOLD_MS {
            "degraded"
        } else {
            "offline"
        }
    }
}

/// SSRF guard: reject loopback, RFC 1918, link-local, and AWS metadata.
fn is_private_or_loopback(host: &str) -> bool {
    if host == "localhost" || host == "127.0.0.1" || host == "::1" {
        return true;
    }

    // AWS instance metadata
    if host == AWS_METADATA_IP {
        return true;
    }

    // RFC 1918 private ranges: 10.0.0.0/8, 172.16.0.0/12, 192.168.0.0/16
    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        if ip.is_loopback() {
            return true;
        }
        match ip {
            std::net::IpAddr::V4(v4) => {
                // 10.0.0.0/8
                if v4.octets()[0] == 10 {
                    return true;
                }
                // 172.16.0.0/12
                if v4.octets()[0] == 172 && (v4.octets()[1] & 0xF0) == 16 {
                    return true;
                }
                // 192.168.0.0/16
                if v4.octets()[0] == 192 && v4.octets()[1] == 168 {
                    return true;
                }
                // Link-local 169.254.0.0/16
                if v4.octets()[0] == 169 && v4.octets()[1] == 254 {
                    return true;
                }
            }
            std::net::IpAddr::V6(v6) => {
                // IPv6 link-local fe80::/10
                if v6.octets()[0] == 0xFE && (v6.octets()[1] & 0xC0) == 0x80 {
                    return true;
                }
                // IPv6 unique local fc00::/7
                if (v6.octets()[0] & 0xFE) == 0xFC {
                    return true;
                }
            }
        }
    }

    false
}

/// Build a reqwest client with health-check-appropriate timeouts.
fn build_client() -> Result<Client, HealthError> {
    let client = Client::builder()
        .connect_timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .timeout(std::time::Duration::from_secs(TOTAL_TIMEOUT_SECS))
        .build()
        .map_err(HealthError::HttpError)?;
    Ok(client)
}

/// Probe a single service URL and return its health result.
async fn probe_service(url: &str) -> Result<ServiceHealth, HealthError> {
    let parsed = url::Url::parse(url).map_err(|e| HealthError::InvalidUrl(e.to_string()))?;
    let host =
        parsed.host_str().ok_or_else(|| HealthError::InvalidUrl("URL has no host".to_string()))?;

    if is_private_or_loopback(host) {
        warn!(url = %url, host = %host, "blocking SSRF probe — host is private/loopback/metadata");
        return Err(HealthError::SsrfBlocked(url.to_string()));
    }

    let client = build_client()?;
    let start = Utc::now();
    let response = client.get(url).send().await?;
    let end = Utc::now();

    let latency_ms = (end - start).num_milliseconds() as f64;
    let status = if response.status().is_success() {
        ServiceHealth::classify(latency_ms).to_string()
    } else {
        warn!(
            url = %url,
            status = %response.status(),
            "service returned non-2xx status"
        );
        "offline".to_string()
    };

    Ok(ServiceHealth {
        service_id: String::new(), // filled in by caller
        status,
        latency_ms: Some(latency_ms),
        last_checked: end,
    })
}

/// HealthChecker — probes services and persists health status.
pub struct HealthChecker {
    pool: PgPool,
    #[allow(dead_code)]
    client: Client,
}

impl HealthChecker {
    /// Create a new HealthChecker with the given database pool.
    pub fn new(pool: PgPool) -> Self {
        // We build a client here but per-probe we also build one with
        // the same settings; keeping one here for reuse in check_service.
        let client = Client::builder()
            .connect_timeout(std::time::Duration::from_secs(CONNECT_TIMEOUT_SECS))
            .timeout(std::time::Duration::from_secs(TOTAL_TIMEOUT_SECS))
            .build()
            .expect("failed to build reqwest client");
        Self { pool, client }
    }

    /// Check a single service by its manifest URL.
    ///
    /// Returns a [`ServiceHealth`] record with status, latency, and
    /// timestamp. Does **not** persist — use [`Self::check_all_services`]
    /// for database writes.
    pub async fn check_service(
        &self,
        service_id: &str,
        manifest_url: &str,
    ) -> Result<ServiceHealth, HealthError> {
        info!(service_id = %service_id, url = %manifest_url, "checking service health");
        let mut health = probe_service(manifest_url).await?;
        health.service_id = service_id.to_string();
        Ok(health)
    }

    /// Iterate all registered services, probe each, and persist status.
    ///
    /// Updates `status`, `fetched_at` (to the probe timestamp), and
    /// `updated_at` on each row. Services that fail the SSRF guard or
    /// time out are marked `"offline"`.
    pub async fn check_all_services(&self) -> Result<Vec<ServiceHealth>, HealthError> {
        info!("starting health check for all services");

        // We use the ac-directory ServiceRow to read service_id + manifest_url.
        // Re-use the same query pattern that ac-directory uses.
        #[derive(sqlx::FromRow)]
        struct ServiceEndpoint {
            service_id: String,
            manifest_url: String,
        }

        let endpoints: Vec<ServiceEndpoint> = sqlx::query_as(
            "SELECT service_id, manifest_url FROM services ORDER BY created_at DESC",
        )
        .fetch_all(&self.pool)
        .await?;

        let mut results = Vec::with_capacity(endpoints.len());

        for ep in &endpoints {
            match self.check_service(&ep.service_id, &ep.manifest_url).await {
                Ok(health) => {
                    info!(
                        service_id = %ep.service_id,
                        status = %health.status,
                        latency_ms = ?health.latency_ms,
                        "service health check complete"
                    );

                    // Persist the result.
                    let now = Utc::now();

                    sqlx::query(
                        r#"
                        UPDATE services
                        SET status = $2,
                            fetched_at = $3,
                            updated_at = $4
                        WHERE service_id = $1
                        "#,
                    )
                    .bind(&ep.service_id)
                    .bind(&health.status)
                    .bind(now)
                    .bind(now)
                    .execute(&self.pool)
                    .await?;

                    results.push(health);
                }
                Err(HealthError::SsrfBlocked(_)) => {
                    warn!(
                        service_id = %ep.service_id,
                        url = %ep.manifest_url,
                        "blocking SSRF probe for service"
                    );

                    let now = Utc::now();
                    sqlx::query(
                        r#"
                        UPDATE services
                        SET status = 'offline',
                            fetched_at = $2,
                            updated_at = $3
                        WHERE service_id = $1
                        "#,
                    )
                    .bind(&ep.service_id)
                    .bind(now)
                    .bind(now)
                    .execute(&self.pool)
                    .await?;

                    results.push(ServiceHealth {
                        service_id: ep.service_id.clone(),
                        status: "offline".to_string(),
                        latency_ms: None,
                        last_checked: now,
                    });
                }
                Err(e) => {
                    error!(
                        service_id = %ep.service_id,
                        url = %ep.manifest_url,
                        error = %e,
                        "service health check failed"
                    );

                    let now = Utc::now();
                    sqlx::query(
                        r#"
                        UPDATE services
                        SET status = 'offline',
                            fetched_at = $2,
                            updated_at = $3
                        WHERE service_id = $1
                        "#,
                    )
                    .bind(&ep.service_id)
                    .bind(now)
                    .bind(now)
                    .execute(&self.pool)
                    .await?;

                    results.push(ServiceHealth {
                        service_id: ep.service_id.clone(),
                        status: "offline".to_string(),
                        latency_ms: None,
                        last_checked: now,
                    });
                }
            }
        }

        let online_count = results.iter().filter(|r| r.status == "online").count();
        let degraded_count = results.iter().filter(|r| r.status == "degraded").count();
        let offline_count = results.iter().filter(|r| r.status == "offline").count();

        info!(
            online = online_count,
            degraded = degraded_count,
            offline = offline_count,
            total = results.len(),
            "health check cycle complete"
        );

        Ok(results)
    }
}
