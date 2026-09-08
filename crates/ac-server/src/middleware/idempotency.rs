//! Idempotency middleware.
//!
//! Caches responses for mutation endpoints based on the `Idempotency-Key`
//! header. Repeated requests with the same key + principal + route return
//! the cached response instead of re-executing the operation.

use ac_crypto::signature::X_AGENT_ID;
use ac_db::pool::SharedPool;
use axum::{
    body::Body,
    extract::{Request, State},
    http::{HeaderName, HeaderValue, StatusCode},
    middleware::Next,
    response::Response,
};
use chrono::Utc;
use hex;
use sha2::{Digest, Sha256};
use sqlx::Row;
use tracing::{info, warn};

pub const X_IDEMPOTENCY_KEY: HeaderName = HeaderName::from_static("idempotency-key");
const IDEMPOTENCY_TTL_SECS: i64 = 86400; // 24 hours

/// Idempotency middleware.
///
/// For requests with an `Idempotency-Key` header:
/// 1. Check if key + principal + route already has a cached response
/// 2. If yes, return the cached response immediately
/// 3. If no, execute the request, cache the response, and return it
pub async fn enforce_idempotency(
    State(pool): State<SharedPool>,
    req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    let headers = req.headers();
    let idempotency_key = match headers.get(&X_IDEMPOTENCY_KEY) {
        Some(k) => k.to_str().map_err(|_| StatusCode::BAD_REQUEST)?.to_string(),
        None => return Ok(next.run(req).await), // No key, pass through
    };

    let principal_id =
        headers.get(X_AGENT_ID).and_then(|v| v.to_str().ok()).unwrap_or("anonymous").to_string();

    let route = req.uri().path().to_string();

    // Hash the request body for dedup
    let (parts, body) = req.into_parts();
    let body_bytes = axum::body::to_bytes(body, usize::MAX)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let request_hash = hex::encode(Sha256::digest(&body_bytes));

    // Restore request
    let req = Request::from_parts(parts, Body::from(body_bytes.clone()));

    let pool = pool.load();

    // Check for cached response
    let cached = sqlx::query(
        r#"
        SELECT response_status, response_body
        FROM idempotency_keys
        WHERE principal_id = $1 AND route = $2 AND idempotency_key = $3
          AND expires_at > NOW()
        "#,
    )
    .bind(&principal_id)
    .bind(&route)
    .bind(&idempotency_key)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        warn!(error = %e, "failed to check idempotency cache");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    if let Some(row) = cached {
        info!(idempotency_key = %idempotency_key, "returning cached idempotent response");
        let status_code: i32 = row.try_get("response_status").unwrap_or(200);
        let status = StatusCode::from_u16(status_code as u16).unwrap_or(StatusCode::OK);

        let body: Option<serde_json::Value> = row.try_get("response_body").ok().flatten();
        let body = match body {
            Some(json) => Body::from(json.to_string()),
            None => Body::empty(),
        };

        let mut resp = Response::builder().status(status).body(body).unwrap();
        resp.headers_mut().insert(
            HeaderName::from_static("x-idempotent-replayed"),
            HeaderValue::from_static("true"),
        );
        return Ok(resp);
    }

    // Execute the request
    let resp = next.run(req).await;

    // Cache successful responses (2xx, 3xx)
    let status = resp.status().as_u16();
    if status < 400 {
        let (parts, body) = resp.into_parts();

        // Collect body bytes for caching
        let body_bytes = match axum::body::to_bytes(body, usize::MAX).await {
            Ok(b) => b,
            Err(_) => {
                warn!("failed to collect response body for idempotency cache");
                return Ok(Response::from_parts(parts, Body::empty()));
            }
        };

        let response_body: Option<serde_json::Value> =
            if body_bytes.is_empty() { None } else { serde_json::from_slice(&body_bytes).ok() };

        let expires_at = Utc::now() + chrono::Duration::seconds(IDEMPOTENCY_TTL_SECS);

        // Insert idempotency key
        let _ = sqlx::query(
            r#"
            INSERT INTO idempotency_keys
                (principal_id, route, idempotency_key, request_hash, response_status, response_body, expires_at)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (principal_id, route, idempotency_key) DO NOTHING
            "#,
        )
        .bind(&principal_id)
        .bind(&route)
        .bind(&idempotency_key)
        .bind(&request_hash)
        .bind(status as i32)
        .bind(&response_body)
        .bind(expires_at)
        .execute(&pool)
        .await;

        let body = Body::from(body_bytes);
        let mut new_resp = Response::from_parts(parts, body);
        new_resp.headers_mut().insert(
            HeaderName::from_static("x-idempotency-key"),
            HeaderValue::try_from(&idempotency_key)
                .unwrap_or_else(|_| HeaderValue::from_static("")),
        );
        return Ok(new_resp);
    }

    Ok(resp)
}
