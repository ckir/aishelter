//! Nonce tracking for replay protection.
//!
//! Each signed request carries a unique nonce (§10).  The [`NonceStore`]
//! tracks which nonces have been seen and rejects any request whose nonce
//! has already been consumed, preventing attackers from replaying a
//! previously valid signed request.
//!
//! Backed by the `nonce_replay_cache` PostgreSQL table with TTL expiry.

use ac_db::pool::SharedPool;
use chrono::Utc;
use tracing::error;

/// Tracks nonces to prevent replay attacks.
///
/// Each nonce is a unique opaque string included in the signed request.
/// When the server processes a request, it calls [`check_and_consume`](NonceStore::check_and_consume);
/// if the nonce was already seen, the request is rejected as a replay.
///
/// Backed by PostgreSQL `nonce_replay_cache` table with expiry.
pub struct NonceStore {
    pool: SharedPool,
}

impl NonceStore {
    /// Create a new nonce store backed by the given database pool.
    pub fn new(pool: SharedPool) -> Self {
        Self { pool }
    }

    /// Check whether a nonce has been used and record it if it is fresh.
    ///
    /// Returns `true` if this is a **new** nonce (the request should be
    /// processed) or `false` if the nonce was already consumed (the
    /// request should be rejected as a replay).
    ///
    /// The `expiry_secs` parameter controls how long the nonce remains
    /// in the cache before expiring.
    pub async fn check_and_consume(
        &self,
        agent_id: &str,
        nonce: &str,
        expiry_secs: i64,
    ) -> Result<bool, sqlx::Error> {
        let expires_at = Utc::now() + chrono::Duration::seconds(expiry_secs);

        // Try to insert; ON CONFLICT DO NOTHING returns 0 rows if already present
        let result = sqlx::query(
            r#"
            INSERT INTO nonce_replay_cache (agent_id, nonce, expires_at)
            VALUES ($1, $2, $3)
            ON CONFLICT (agent_id, nonce) DO NOTHING
            "#,
        )
        .bind(agent_id)
        .bind(nonce)
        .bind(expires_at)
        .execute(&self.pool.load())
        .await;

        match result {
            Ok(rows) => Ok(rows.rows_affected() > 0),
            Err(e) => {
                error!(error = %e, agent_id = %agent_id, nonce = %nonce, "failed to insert nonce");
                Err(e)
            }
        }
    }

    /// Clean up expired nonces.
    ///
    /// This should be called periodically (e.g. by a background job or
    /// scheduled task) to prevent the table from growing unbounded.
    pub async fn purge_expired(&self) -> Result<u64, sqlx::Error> {
        let result = sqlx::query("DELETE FROM nonce_replay_cache WHERE expires_at < NOW()")
            .execute(&self.pool.load())
            .await?;

        Ok(result.rows_affected())
    }
}
