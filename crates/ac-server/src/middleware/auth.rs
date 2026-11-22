//! Request authentication middleware for protected routes.
//!
//! Verifies the Ed25519 request signature (§10-11) and binds the verified
//! agent id to the request as an [`AuthenticatedAgent`] extension. Handlers
//! must take the caller's identity from that extension, never from the body,
//! the query string or the `X-Agent-Id` header.
//!
//! Checks, in this order (cheap and side-effect free first):
//!
//! 1. credentials present (`X-Agent-Id`, `-Timestamp`, `-Nonce`, `-Signature`)
//! 2. timestamp is RFC 3339 and within ±[`MAX_TIMESTAMP_SKEW_SECS`]
//! 3. body is at most [`MAX_BODY_BYTES`] and is buffered for hashing
//! 4. agent exists and is `REGISTERED` or `ACTIVE`
//! 5. signature verifies over
//!    `agent_id \n timestamp \n nonce \n method \n path_and_query \n sha256(body)`
//! 6. nonce is consumed (single use), **only after** the signature verified,
//!    so an unauthenticated caller cannot burn another agent's nonces
//!
//! Failure mapping: 401 missing credentials or failed authentication, 400
//! malformed, stale or replayed credentials, 413 oversized body, 503 when
//! the database is unavailable (fail closed).

use ac_crypto::keypair::public_key_from_hex;
use ac_crypto::nonce::NonceStore;
use ac_crypto::signature::{
    SignedRequest, X_AGENT_ID, X_AGENT_NONCE, X_AGENT_SIGNATURE, X_AGENT_TIMESTAMP,
};
use ac_db::pool::SharedPool;
use axum::{
    Json,
    body::{Body, to_bytes},
    extract::{OriginalUri, Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Response},
};
use tracing::{error, warn};

/// Maximum accepted difference between the client timestamp and server time.
pub const MAX_TIMESTAMP_SKEW_SECS: i64 = 300;

/// How long a consumed nonce is remembered.
///
/// A request dated up to `+MAX_TIMESTAMP_SKEW_SECS` ahead is still inside the
/// skew window until `2 * MAX_TIMESTAMP_SKEW_SECS` after it was first seen, so
/// the replay cache must outlive that or the request can be replayed.
pub const NONCE_TTL_SECS: i64 = 2 * MAX_TIMESTAMP_SKEW_SECS;

/// Largest request body the authentication layer will buffer.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

const MAX_ID_LEN: usize = 256;
const MAX_NONCE_LEN: usize = 128;
const MAX_SIGNATURE_LEN: usize = 256;

/// Agent lifecycle states allowed to make signed requests.
const ACTIVE_STATUSES: [&str; 2] = ["REGISTERED", "ACTIVE"];

const TIMESTAMP_HELP: &str =
    "X-Agent-Timestamp must be an RFC 3339 UTC timestamp, e.g. 2026-11-22T20:30:00Z";

/// The agent whose signature verified for this request.
///
/// Inserted into request extensions by [`authenticate`]. Extract with
/// `Extension<AuthenticatedAgent>` in handlers on protected routes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedAgent(pub String);

/// Why a request failed authentication.
#[derive(Debug)]
pub enum AuthError {
    /// No `X-Agent-Id` header: the caller presented no credentials at all.
    MissingCredentials,
    /// Credentials present but malformed, stale, future-dated or replayed.
    Malformed(String),
    /// Unknown or inactive agent, or a signature that does not verify.
    /// Deliberately one message, so the response is not an agent-existence oracle.
    Rejected,
    /// Body larger than [`MAX_BODY_BYTES`] (or unreadable).
    PayloadTooLarge,
    /// The database could not be reached; fail closed.
    Unavailable,
}

impl IntoResponse for AuthError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AuthError::MissingCredentials => (
                StatusCode::UNAUTHORIZED,
                "authentication required: send X-Agent-Id, X-Agent-Timestamp, \
                 X-Agent-Nonce and X-Agent-Signature"
                    .to_string(),
            ),
            AuthError::Malformed(msg) => (StatusCode::BAD_REQUEST, msg),
            AuthError::Rejected => (StatusCode::UNAUTHORIZED, "authentication failed".to_string()),
            AuthError::PayloadTooLarge => (
                StatusCode::PAYLOAD_TOO_LARGE,
                format!("request body exceeds {MAX_BODY_BYTES} bytes"),
            ),
            AuthError::Unavailable => (
                StatusCode::SERVICE_UNAVAILABLE,
                "authentication temporarily unavailable".to_string(),
            ),
        };
        let mut resp = (status, Json(serde_json::json!({ "error": message }))).into_response();
        if status == StatusCode::UNAUTHORIZED {
            resp.headers_mut()
                .insert(header::WWW_AUTHENTICATE, HeaderValue::from_static("ACP1-Ed25519"));
        }
        resp
    }
}

/// Read an optional header as UTF-8 text.
fn header_text<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, AuthError> {
    match headers.get(name) {
        None => Ok(None),
        Some(v) => v
            .to_str()
            .map(Some)
            .map_err(|_| AuthError::Malformed(format!("{name} header is not valid text"))),
    }
}

/// Read a required header, enforcing a maximum length.
fn required_header<'a>(
    headers: &'a HeaderMap,
    name: &str,
    max_len: usize,
) -> Result<&'a str, AuthError> {
    let value = header_text(headers, name)?
        .ok_or_else(|| AuthError::Malformed(format!("missing {name} header")))?;
    if value.is_empty() || value.len() > max_len {
        return Err(AuthError::Malformed(format!("{name} header has an invalid length")));
    }
    Ok(value)
}

/// Authenticate the request and attach an [`AuthenticatedAgent`].
///
/// Layer it on protected routers with
/// `from_fn_with_state(pool, authenticate)`.
pub async fn authenticate(
    State(pool): State<SharedPool>,
    req: Request,
    next: Next,
) -> Result<Response, AuthError> {
    let headers = req.headers();

    // 1. Credentials. No agent id at all means "unauthenticated" (401);
    //    an id with incomplete or garbled companions is a malformed request (400).
    if header_text(headers, X_AGENT_ID)?.is_none() {
        return Err(AuthError::MissingCredentials);
    }
    let agent_id = required_header(headers, X_AGENT_ID, MAX_ID_LEN)?.to_string();
    let timestamp = required_header(headers, X_AGENT_TIMESTAMP, 64)?.to_string();
    let nonce = required_header(headers, X_AGENT_NONCE, MAX_NONCE_LEN)?.to_string();
    let signature = required_header(headers, X_AGENT_SIGNATURE, MAX_SIGNATURE_LEN)?.to_string();

    // 2. Timestamp format and clock skew.
    let parsed = SignedRequest::parse_timestamp(&timestamp)
        .map_err(|_| AuthError::Malformed(TIMESTAMP_HELP.to_string()))?;
    let skew = (chrono::Utc::now() - parsed).num_seconds().abs();
    if skew > MAX_TIMESTAMP_SKEW_SECS {
        return Err(AuthError::Malformed(format!(
            "X-Agent-Timestamp is outside the allowed clock skew of {MAX_TIMESTAMP_SKEW_SECS}s"
        )));
    }

    // The signed request target is what the client sent, query string
    // included. A nested router has already stripped its prefix from
    // `req.uri()`, so prefer the original URI that `nest` records.
    let target = req
        .extensions()
        .get::<OriginalUri>()
        .map(|original| original.0.clone())
        .unwrap_or_else(|| req.uri().clone());
    let path_and_query =
        target.path_and_query().map(|pq| pq.as_str().to_owned()).unwrap_or_else(|| "/".to_owned());

    // 3. Buffer the body (bounded) so it can be hashed, then restore it.
    let (mut parts, body) = req.into_parts();
    let body_bytes =
        to_bytes(body, MAX_BODY_BYTES).await.map_err(|_| AuthError::PayloadTooLarge)?;

    // 4. Agent lookup. A database failure fails closed.
    let pg = pool.load();
    let row: Option<(String, String)> =
        sqlx::query_as("SELECT public_key, status FROM agents WHERE agent_id = $1")
            .bind(&agent_id)
            .fetch_optional(&pg)
            .await
            .map_err(|e| {
                error!(error = %e, "auth: agent lookup failed");
                AuthError::Unavailable
            })?;
    let Some((public_key_hex, status)) = row else {
        warn!(agent_id = %agent_id, "auth: unknown agent");
        return Err(AuthError::Rejected);
    };
    if !ACTIVE_STATUSES.contains(&status.as_str()) {
        warn!(agent_id = %agent_id, status = %status, "auth: agent not active");
        return Err(AuthError::Rejected);
    }
    let Ok(public_key) = public_key_from_hex(&public_key_hex) else {
        warn!(agent_id = %agent_id, "auth: stored public key is not a valid Ed25519 key");
        return Err(AuthError::Rejected);
    };

    // 5. Signature over the exact bytes the client sent.
    let signed = SignedRequest {
        agent_id: agent_id.clone(),
        timestamp,
        nonce: nonce.clone(),
        method: parts.method.as_str().to_string(),
        path: path_and_query,
        body_sha256: SignedRequest::hash_body(&body_bytes),
        signature,
    };
    if signed.verify(&public_key).is_err() {
        warn!(agent_id = %agent_id, "auth: signature verification failed");
        return Err(AuthError::Rejected);
    }

    // 6. Consume the nonce only now that the caller has proven who they are.
    let fresh = NonceStore::new(pool.clone())
        .check_and_consume(&agent_id, &nonce, NONCE_TTL_SECS)
        .await
        .map_err(|e| {
            error!(error = %e, "auth: nonce store failed");
            AuthError::Unavailable
        })?;
    if !fresh {
        warn!(agent_id = %agent_id, "auth: nonce replay rejected");
        return Err(AuthError::Malformed("nonce already used".to_string()));
    }

    parts.extensions.insert(AuthenticatedAgent(agent_id));
    Ok(next.run(Request::from_parts(parts, Body::from(body_bytes))).await)
}
