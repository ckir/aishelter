//! Database readiness check.
//!
//! Provides a simple readiness probe that tests database connectivity
//! with a short timeout, suitable for Kubernetes-style readiness checks.

use sqlx::PgPool;
use std::time::Duration;

/// Check if the database is reachable by attempting to acquire a connection.
///
/// Returns `true` if a connection can be acquired within 1 second,
/// `false` otherwise.
pub async fn check(pool: &PgPool) -> bool {
    tokio::time::timeout(Duration::from_secs(1), pool.acquire()).await.is_ok()
}
