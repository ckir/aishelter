//! RDS IAM authentication token refresh.
//!
//! When the `rds-iam` feature is enabled, this module provides a
//! background task that periodically generates a fresh IAM auth token
//! and swaps the database connection pool so that new connections use
//! the latest credentials.
//!
//! IAM auth tokens are valid for ~15 minutes. The refresh loop runs
//! every `REFRESH_INTERVAL` (default 12 minutes) to ensure a fresh
//! token is always available before the current one expires.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────┐   swap()   ┌────────────┐
//! │ refresh_loop     │──────────▶│ SharedPool  │
//! │ (tokio task)     │           │ (ArcSwap)   │
//! └────────┬────────┘           └──────┬─────┘
//!          │                            │ load()
//!          │ generate_token()           ▼
//!          │                    ┌────────────────┐
//!          ▼                    │ Request handler │
//!    ┌───────────┐              └────────────────┘
//!    │ RDS IAM   │
//!    │ SigV4     │
//!    └───────────┘
//! ```

use ac_db::pool::SharedPool;
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use url::Url;

/// Default interval between token refreshes (12 minutes).
///
/// Chosen to be comfortably within the 15-minute token lifetime,
/// leaving a 3-minute safety margin.
const REFRESH_INTERVAL: Duration = Duration::from_secs(12 * 60);

/// Maximum database connections per pool.
const MAX_CONNECTIONS: u32 = 10;

/// Parsed RDS connection parameters.
///
/// Extracted from the `AC_DATABASE_URL` so the refresh loop can
/// reconstruct the URL with a fresh token on each cycle.
#[derive(Debug, Clone)]
pub struct RdsConnParams {
    /// RDS hostname (e.g. `db.cluster-xxx.us-east-1.rds.amazonaws.com`)
    pub hostname: String,
    /// Database port (default 5432)
    pub port: u16,
    /// Database user (e.g. `postgres`)
    pub username: String,
    /// Database name (e.g. `aishelter`)
    pub dbname: String,
    /// AWS region (e.g. `us-east-1`)
    pub region: String,
    /// Extra query parameters from the original URL (e.g. `sslmode=require`)
    pub query_params: String,
}

impl RdsConnParams {
    /// Parse RDS connection parameters from a `postgresql://` URL.
    ///
    /// The password field is ignored since it will be replaced with
    /// a fresh IAM auth token on each refresh cycle.
    ///
    /// # Errors
    ///
    /// Returns an error if the URL cannot be parsed or is missing
    /// required fields (host, username, database).
    pub fn from_url(database_url: &str) -> anyhow::Result<Self> {
        let url = Url::parse(database_url)?;

        let hostname = url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("database_url missing hostname"))?
            .to_string();

        let port = url.port().unwrap_or(5432);

        let username = if url.username().is_empty() {
            anyhow::bail!("database_url missing username");
        } else {
            url.username().to_string()
        };

        let dbname = url.path().trim_start_matches('/').to_string();
        if dbname.is_empty() {
            anyhow::bail!("database_url missing database name");
        }

        // Infer region from the RDS hostname:
        //   database-1.cluster-xxx.us-east-1.rds.amazonaws.com
        //                         ^^^^^^^^^ region
        let region = hostname
            .split('.')
            .rev()
            .nth(3) // "us-east-1" in the reversed split
            .unwrap_or("us-east-1")
            .to_string();

        let query_params = url.query().unwrap_or("").to_string();

        Ok(Self { hostname, port, username, dbname, region, query_params })
    }

    /// Build a `postgresql://` connection URL with the given password.
    ///
    /// The password is URL-encoded to handle the special characters
    /// (`/`, `=`, `+`) present in IAM auth tokens.
    pub fn build_url(&self, password: &str) -> String {
        // Percent-encode the password to handle IAM token characters.
        // We encode everything except unreserved chars (RFC 3986).
        use url::form_urlencoded;
        let encoded: String = form_urlencoded::byte_serialize(password.as_bytes()).collect();
        let base = format!(
            "postgresql://{}:{}@{}:{}/{}",
            self.username, encoded, self.hostname, self.port, self.dbname,
        );
        if self.query_params.is_empty() { base } else { format!("{}?{}", base, self.query_params) }
    }
}

/// Generate a fresh RDS IAM auth token using `aws-rds-signer`.
///
/// The token is a SigV4-signed URL valid for 15 minutes. It uses
/// the default AWS credential chain (env vars, `~/.aws/credentials`,
/// EC2/ECS instance role, etc.).
#[cfg(feature = "rds-iam")]
pub async fn generate_token(params: &RdsConnParams) -> anyhow::Result<String> {
    use aws_rds_signer::Signer;

    let signer = aws_rds_signer::Signer::builder()
        .host(params.hostname.clone())
        .port(params.port)
        .user(params.username.clone())
        .region(params.region.clone())
        .build();

    let token = signer
        .fetch_token()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to generate RDS IAM token: {}", e))?;

    Ok(token)
}

/// Start the background token refresh loop.
///
/// Spawns a tokio task that:
/// 1. Waits for `REFRESH_INTERVAL`
/// 2. Generates a fresh IAM auth token
/// 3. Creates a new `PgPool` with the fresh token
/// 4. Atomically swaps it into the `SharedPool`
/// 5. Repeats from step 1
///
/// The task runs until the tokio runtime is shut down.
///
/// # Arguments
///
/// * `shared_pool` — The [`SharedPool`] to swap on each refresh.
/// * `params` — Parsed RDS connection parameters.
/// * `interval` — How often to refresh. Pass `None` for the default
///   12-minute interval.
#[cfg(feature = "rds-iam")]
pub fn start_refresh_loop(
    shared_pool: SharedPool,
    params: RdsConnParams,
    interval: Option<Duration>,
) -> tokio::task::JoinHandle<()> {
    let interval = interval.unwrap_or(REFRESH_INTERVAL);

    tokio::spawn(async move {
        loop {
            tokio::time::sleep(interval).await;

            tracing::info!("RDS IAM: refreshing auth token...");

            match generate_token(&params).await {
                Ok(token) => {
                    let url = params.build_url(&token);
                    match PgPoolOptions::new().max_connections(MAX_CONNECTIONS).connect(&url).await
                    {
                        Ok(new_pool) => {
                            shared_pool.swap(new_pool);
                            tracing::info!("RDS IAM: pool swapped successfully");
                        }
                        Err(e) => {
                            tracing::error!("RDS IAM: failed to create pool with new token: {e}");
                            // Keep using the old pool — it may still have live connections
                        }
                    }
                }
                Err(e) => {
                    tracing::error!("RDS IAM: token generation failed: {e}");
                    // Keep using the old pool
                }
            }
        }
    })
}

/// Convenience: generate a token, create a pool, and return both the
/// pool and the parsed params (needed by the refresh loop).
///
/// Used at server startup to create the initial pool with a fresh token.
#[cfg(feature = "rds-iam")]
pub async fn create_iam_pool(database_url: &str) -> anyhow::Result<(sqlx::PgPool, RdsConnParams)> {
    let params = RdsConnParams::from_url(database_url)?;

    tracing::info!(
        "RDS IAM: generating initial auth token for {}@{}",
        params.username,
        params.hostname
    );

    let token = generate_token(&params).await?;
    let url = params.build_url(&token);

    let pool = PgPoolOptions::new().max_connections(MAX_CONNECTIONS).connect(&url).await?;

    Ok((pool, params))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_rds_url() {
        let url = "postgresql://postgres:pass@db.cluster-xxx.us-east-1.rds.amazonaws.com:5432/aishelter?sslmode=require";
        let params = RdsConnParams::from_url(url).unwrap();
        assert_eq!(params.hostname, "db.cluster-xxx.us-east-1.rds.amazonaws.com");
        assert_eq!(params.port, 5432);
        assert_eq!(params.username, "postgres");
        assert_eq!(params.dbname, "aishelter");
        assert_eq!(params.region, "us-east-1");
        assert_eq!(params.query_params, "sslmode=require");
    }

    #[test]
    fn build_url_encodes_token() {
        let params = RdsConnParams {
            hostname: "db.rds.amazonaws.com".to_string(),
            port: 5432,
            username: "postgres".to_string(),
            dbname: "test".to_string(),
            region: "us-east-1".to_string(),
            query_params: "sslmode=require".to_string(),
        };
        let url = params.build_url("token/with+special=chars");
        assert!(url.contains("token%2Fwith%2Bspecial%3Dchars"));
        assert!(url.ends_with("sslmode=require"));
    }
}
