//! Idempotency middleware.
//!
//! Replays the stored response when a client retries a mutation with the same
//! `Idempotency-Key`, so a retry after a timeout cannot execute it twice.
//!
//! Scope and rules:
//!
//! - The key is scoped to the **verified principal** (the
//!   [`AuthenticatedAgent`] set by the authentication layer, which therefore
//!   must run first) plus `METHOD /path`. Unauthenticated routes share the
//!   `anonymous` principal. The `X-Agent-Id` header is never consulted: it is
//!   attacker-controlled. The anonymous key namespace is therefore shared by
//!   every caller: clients must choose unguessable keys (a random UUID), or a
//!   stranger who guesses a key first makes the real caller's retry a 422.
//! - Replaying a key with a **different body** is a client error (422), not a
//!   silent replay of an unrelated response.
//! - Only successful (`< 400`) responses to unsafe methods are stored, so a
//!   failed attempt can be retried under the same key. `GET`/`HEAD`/`OPTIONS`
//!   pass through untouched.
//! - Request and response bodies are bounded; nothing is buffered unbounded.
//!
//! Known limitation: two *concurrent* requests with the same new key can both
//! execute, because the key is stored after the handler finishes. Closing that
//! needs a reserve-then-complete protocol.

use ac_db::pool::SharedPool;
use ac_types::auth::AuthenticatedAgent;
use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{OriginalUri, Request, State},
    http::{HeaderName, HeaderValue, Method, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use chrono::Utc;
use sha2::{Digest, Sha256};
use sqlx::Row;
use tracing::{info, warn};

use super::auth::MAX_BODY_BYTES;

/// Request header carrying the client-chosen idempotency key.
pub const X_IDEMPOTENCY_KEY: HeaderName = HeaderName::from_static("idempotency-key");
/// Response header set on a replayed response.
const X_IDEMPOTENT_REPLAYED: HeaderName = HeaderName::from_static("x-idempotent-replayed");

const IDEMPOTENCY_TTL_SECS: i64 = 86400; // 24 hours
const MAX_KEY_LEN: usize = 255;
/// Largest response body that will be stored for replay.
const MAX_CACHED_RESPONSE_BYTES: usize = 1024 * 1024;
/// Principal used on routes that carry no authentication.
const ANONYMOUS: &str = "anonymous";

fn reject(status: StatusCode, message: &str) -> Response {
    (status, Json(serde_json::json!({ "error": message }))).into_response()
}

/// Idempotency middleware.
///
/// For unsafe-method requests carrying an `Idempotency-Key` header:
/// 1. look up key + principal + route
/// 2. same body: return the stored response; different body: 422
/// 3. no entry: run the request and store a successful response
pub async fn enforce_idempotency(
    State(pool): State<SharedPool>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let Some(raw_key) = req.headers().get(&X_IDEMPOTENCY_KEY) else {
        return next.run(req).await;
    };
    if matches!(*req.method(), Method::GET | Method::HEAD | Method::OPTIONS) {
        return next.run(req).await;
    }
    let idempotency_key = match raw_key.to_str() {
        Ok(k) if !k.is_empty() && k.len() <= MAX_KEY_LEN => k.to_string(),
        _ => {
            return reject(
                StatusCode::BAD_REQUEST,
                "Idempotency-Key must be 1-255 visible ASCII characters",
            );
        }
    };

    let principal_id = req
        .extensions()
        .get::<AuthenticatedAgent>()
        .map_or_else(|| ANONYMOUS.to_string(), |agent| agent.0.clone());
    // `req.uri()` has the nest prefix stripped; the original URI names the route.
    let path = req
        .extensions()
        .get::<OriginalUri>()
        .map_or_else(|| req.uri().path().to_string(), |original| original.0.path().to_string());
    let route = format!("{} {}", req.method(), path);

    let (parts, body) = req.into_parts();
    let Ok(body_bytes) = to_bytes(body, MAX_BODY_BYTES).await else {
        return reject(
            StatusCode::PAYLOAD_TOO_LARGE,
            &format!("request body exceeds {MAX_BODY_BYTES} bytes"),
        );
    };
    let request_hash = hex::encode(Sha256::digest(&body_bytes));
    let req = Request::from_parts(parts, Body::from(body_bytes));

    let pool = pool.load();

    let cached = sqlx::query(
        r#"
        SELECT request_hash, response_status, response_body
        FROM idempotency_keys
        WHERE principal_id = $1 AND route = $2 AND idempotency_key = $3
          AND expires_at > NOW()
        "#,
    )
    .bind(&principal_id)
    .bind(&route)
    .bind(&idempotency_key)
    .fetch_optional(&pool)
    .await;
    let cached = match cached {
        Ok(row) => row,
        Err(e) => {
            warn!(error = %e, "failed to check idempotency cache");
            return reject(StatusCode::SERVICE_UNAVAILABLE, "idempotency store unavailable");
        }
    };

    if let Some(row) = cached {
        let stored_hash: String = row.try_get("request_hash").unwrap_or_default();
        if stored_hash != request_hash {
            return reject(
                StatusCode::UNPROCESSABLE_ENTITY,
                "Idempotency-Key was already used with a different request body",
            );
        }
        info!(idempotency_key = %idempotency_key, "returning cached idempotent response");
        let status_code: i32 = row.try_get("response_status").unwrap_or(200);
        let status = u16::try_from(status_code)
            .ok()
            .and_then(|code| StatusCode::from_u16(code).ok())
            .unwrap_or(StatusCode::OK);
        let stored: Option<serde_json::Value> = row.try_get("response_body").ok().flatten();

        let mut resp = match stored {
            Some(json) => {
                let mut r = Response::new(Body::from(json.to_string()));
                r.headers_mut()
                    .insert(header::CONTENT_TYPE, HeaderValue::from_static("application/json"));
                r
            }
            None => Response::new(Body::empty()),
        };
        *resp.status_mut() = status;
        resp.headers_mut().insert(X_IDEMPOTENT_REPLAYED, HeaderValue::from_static("true"));
        return resp;
    }

    let resp = next.run(req).await;
    if resp.status().as_u16() >= 400 {
        return resp;
    }

    let (parts, body) = resp.into_parts();
    let Ok(body_bytes) = to_bytes(body, MAX_CACHED_RESPONSE_BYTES).await else {
        warn!("response too large to store for idempotency");
        return reject(StatusCode::INTERNAL_SERVER_ERROR, "response too large");
    };

    let response_body: Option<serde_json::Value> =
        if body_bytes.is_empty() { None } else { serde_json::from_slice(&body_bytes).ok() };
    let expires_at = Utc::now() + chrono::Duration::seconds(IDEMPOTENCY_TTL_SECS);

    let stored = sqlx::query(
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
    .bind(i32::from(parts.status.as_u16()))
    .bind(&response_body)
    .bind(expires_at)
    .execute(&pool)
    .await;
    if let Err(e) = stored {
        warn!(error = %e, "failed to store idempotent response");
    }

    let mut new_resp = Response::from_parts(parts, Body::from(body_bytes));
    if let Ok(value) = HeaderValue::from_str(&idempotency_key) {
        new_resp.headers_mut().insert(HeaderName::from_static("x-idempotency-key"), value);
    }
    new_resp
}
