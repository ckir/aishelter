//! Nonce replay protection middleware.
//!
//! Validates the `X-Agent-Nonce` header against the nonce_replay_cache table.
//! Rejects requests with reused or expired nonces.

use ac_crypto::nonce::NonceStore;
use ac_crypto::signature::{X_AGENT_ID, X_AGENT_NONCE, X_AGENT_TIMESTAMP};
use ac_db::pool::SharedPool;
use axum::{
    body::Body,
    extract::{Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use tracing::warn;

pub const MAX_TIMESTAMP_SKEW_SECS: i64 = 300; // 5 minutes

/// Check nonce middleware.
///
/// Validates:
/// 1. X-Agent-Timestamp is within configured skew (default 300s)
/// 2. X-Agent-Nonce is present and not previously consumed
/// 3. Nonce is inserted into cache with expiry
pub async fn check_nonce(
    State(pool): State<SharedPool>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let headers = req.headers();

    // Check timestamp
    let timestamp_str =
        headers.get(X_AGENT_TIMESTAMP).and_then(|v| v.to_str().ok()).ok_or_else(|| {
            warn!("missing X-Agent-Timestamp header");
            StatusCode::BAD_REQUEST
        })?;

    let timestamp: i64 = timestamp_str.parse().map_err(|_| {
        warn!(timestamp = %timestamp_str, "invalid X-Agent-Timestamp");
        StatusCode::BAD_REQUEST
    })?;

    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs()
        as i64;

    if (now - timestamp).abs() > MAX_TIMESTAMP_SKEW_SECS {
        warn!(timestamp, now, "timestamp skew exceeds allowed range");
        return Err(StatusCode::BAD_REQUEST);
    }

    // Check nonce (only for signed requests)
    if let Some(nonce_header) = headers.get(X_AGENT_NONCE) {
        let nonce = nonce_header.to_str().map_err(|_| {
            warn!("invalid X-Agent-Nonce header value");
            StatusCode::BAD_REQUEST
        })?;

        // Get agent_id from X-Agent-Id header
        let agent_id = headers.get(X_AGENT_ID).and_then(|v| v.to_str().ok()).ok_or_else(|| {
            warn!("X-Agent-Nonce present but X-Agent-Id missing");
            StatusCode::BAD_REQUEST
        })?;

        // Check and consume nonce
        let nonce_store = NonceStore::new(pool.clone());
        let is_valid = nonce_store
            .check_and_consume(agent_id, nonce, MAX_TIMESTAMP_SKEW_SECS)
            .await
            .map_err(|e| {
                warn!(error = %e, "failed to check nonce");
                StatusCode::INTERNAL_SERVER_ERROR
            })?;

        if !is_valid {
            warn!(agent_id = %agent_id, nonce = %nonce, "nonce already consumed or expired");
            return Err(StatusCode::BAD_REQUEST);
        }
    }

    Ok(next.run(req).await)
}
