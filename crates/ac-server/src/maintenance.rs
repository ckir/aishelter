//! Periodic cleanup of tables that only ever grow.
//!
//! Nonces (replay protection) and idempotency keys carry an `expires_at`;
//! nothing else removes them once it passes.

use std::time::Duration;

use ac_crypto::nonce::NonceStore;
use ac_db::pool::SharedPool;
use tracing::{error, info};

/// How many rows one purge pass removed.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Purged {
    /// Rows removed from `nonce_replay_cache`.
    pub nonces: u64,
    /// Rows removed from `idempotency_keys`.
    pub idempotency_keys: u64,
}

/// Delete expired nonces and idempotency keys once.
///
/// # Errors
///
/// Returns the database error if either delete fails.
pub async fn purge_expired(pool: &SharedPool) -> Result<Purged, sqlx::Error> {
    let nonces = NonceStore::new(pool.clone()).purge_expired().await?;
    let idempotency_keys = sqlx::query("DELETE FROM idempotency_keys WHERE expires_at < NOW()")
        .execute(&pool.load())
        .await?
        .rows_affected();
    Ok(Purged { nonces, idempotency_keys })
}

/// Run [`purge_expired`] every `interval` on the current Tokio runtime.
///
/// Long-lived servers call this once at startup. Short-lived (serverless)
/// entry points have no background runtime and should call
/// [`purge_expired`] from a scheduled invocation instead.
pub fn spawn_purge_task(pool: SharedPool, interval: Duration) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(interval);
        loop {
            ticker.tick().await;
            match purge_expired(&pool).await {
                Ok(p) if p != Purged::default() => info!(?p, "purged expired rows"),
                Ok(_) => {}
                Err(e) => error!(error = %e, "purge of expired rows failed"),
            }
        }
    })
}
