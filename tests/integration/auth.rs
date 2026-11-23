//! Integration tests for request authentication on protected routes.
//!
//! Covers, via `GET/POST /v1/messages` (a protected route):
//! - a correctly signed request is accepted
//! - missing credentials, unknown agents, wrong keys, corrupted signatures,
//!   and tampered bodies or query strings are rejected with 401
//! - malformed, stale, future-dated and Unix-seconds timestamps are rejected
//!   with 400 and the error names the expected format
//! - nonces are single use, scoped per agent, retained for twice the skew,
//!   and are NOT consumed by a request whose signature fails
//! - suspended agents and oversized bodies are rejected
//! - public routes stay open without credentials

use axum::http::{HeaderName, HeaderValue, StatusCode};
use chrono::{Duration, Utc};
use serde_json::json;

use ac_crypto::signature::{
    SignedRequest, X_AGENT_ID, X_AGENT_NONCE, X_AGENT_SIGNATURE, X_AGENT_TIMESTAMP,
};

use crate::harness::{TestAgent, TestApp, request_from_signed};

fn list_own(agent: &TestAgent) -> String {
    format!("/v1/messages?agent_id={}", agent.id)
}

/// RFC 3339 timestamp `offset_secs` from now, in the canonical client form.
fn ts(offset_secs: i64) -> String {
    SignedRequest::format_timestamp(Utc::now() + Duration::seconds(offset_secs))
}

async fn body_text(resp: axum::http::Response<axum::body::Body>) -> String {
    let bytes = axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("read body");
    String::from_utf8_lossy(&bytes).into_owned()
}

#[tokio::test]
async fn valid_signed_request_is_accepted() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;

    let resp = app.send_signed(&agent, "GET", &list_own(&agent), None).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn request_without_credentials_is_401() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;

    let resp = app.get(&list_own(&agent)).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn public_routes_stay_open_without_credentials() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;

    assert_eq!(app.get("/v1/health").await.status(), StatusCode::OK);
    assert_eq!(app.get(&format!("/v1/agents/{}", agent.id)).await.status(), StatusCode::OK);
}

#[tokio::test]
async fn unknown_agent_is_401() {
    let app = TestApp::setup().await;
    let ghost = TestAgent::new(); // never registered

    let resp = app.send_signed(&ghost, "GET", &list_own(&ghost), None).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn signature_by_a_different_key_is_401() {
    let app = TestApp::setup().await;
    let victim = app.register_agent().await;

    // Imposter holds its own key but claims the victim's id.
    let mut imposter = TestAgent::new();
    imposter.id = victim.id.clone();

    let resp = app.send_signed(&imposter, "GET", &list_own(&victim), None).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn corrupted_signature_is_401() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;

    let mut signed = agent.sign("GET", &list_own(&agent), b"");
    let last = signed.signature.pop().unwrap();
    signed.signature.push(if last == '0' { '1' } else { '0' });

    let resp = app.request(request_from_signed(&signed, vec![])).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tampered_body_is_401() {
    let app = TestApp::setup().await;
    let sender = app.register_agent().await;
    let receiver = app.register_agent().await;

    let signed_for = serde_json::to_vec(&json!({
        "from_agent_id": sender.id, "to_agent_id": receiver.id, "type": "note", "payload": {"n": 1}
    }))
    .unwrap();
    let sent = serde_json::to_vec(&json!({
        "from_agent_id": sender.id, "to_agent_id": receiver.id, "type": "note", "payload": {"n": 2}
    }))
    .unwrap();

    let signed = sender.sign("POST", "/v1/messages", &signed_for);
    let resp = app.request(request_from_signed(&signed, sent)).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn tampered_query_string_is_401() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;

    let mut signed = agent.sign("GET", &list_own(&agent), b"");
    // Signature was computed for our own mailbox; ask for someone else's.
    signed.path = "/v1/messages?agent_id=someone-else".to_string();

    let resp = app.request(request_from_signed(&signed, vec![])).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn stale_timestamp_is_400() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;

    let signed = agent.sign_at("GET", &list_own(&agent), b"", &TestAgent::nonce(), &ts(-600));
    let resp = app.request(request_from_signed(&signed, vec![])).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn future_timestamp_beyond_skew_is_400() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;

    let signed = agent.sign_at("GET", &list_own(&agent), b"", &TestAgent::nonce(), &ts(600));
    let resp = app.request(request_from_signed(&signed, vec![])).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn timestamp_within_skew_is_accepted() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;

    let signed = agent.sign_at("GET", &list_own(&agent), b"", &TestAgent::nonce(), &ts(-200));
    let resp = app.request(request_from_signed(&signed, vec![])).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn unix_seconds_timestamp_is_400_and_names_the_expected_format() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;

    let unix = Utc::now().timestamp().to_string();
    let signed = agent.sign_at("GET", &list_own(&agent), b"", &TestAgent::nonce(), &unix);
    let resp = app.request(request_from_signed(&signed, vec![])).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let text = body_text(resp).await;
    assert!(text.contains("RFC 3339"), "error should name the expected format, got: {text}");
}

#[tokio::test]
async fn missing_timestamp_header_is_400() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;

    let signed = agent.sign("GET", &list_own(&agent), b"");
    let mut req = request_from_signed(&signed, vec![]);
    req.headers_mut().remove("x-agent-timestamp");

    let resp = app.request(req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn reused_nonce_is_rejected() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;
    let nonce = TestAgent::nonce();

    let first = agent.sign_at("GET", &list_own(&agent), b"", &nonce, &ts(0));
    let r1 = app.request(request_from_signed(&first, vec![])).await;
    assert_eq!(r1.status(), StatusCode::OK);

    let replay = agent.sign_at("GET", &list_own(&agent), b"", &nonce, &ts(0));
    let r2 = app.request(request_from_signed(&replay, vec![])).await;
    assert_eq!(r2.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn failed_signature_does_not_burn_the_nonce() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;
    let nonce = TestAgent::nonce();

    // An attacker who knows the agent id and a nonce sends a forged signature.
    let mut forged = agent.sign_at("GET", &list_own(&agent), b"", &nonce, &ts(0));
    let last = forged.signature.pop().unwrap();
    forged.signature.push(if last == '0' { '1' } else { '0' });
    let r1 = app.request(request_from_signed(&forged, vec![])).await;
    assert_eq!(r1.status(), StatusCode::UNAUTHORIZED);

    // The genuine request with the same nonce must still go through.
    let genuine = agent.sign_at("GET", &list_own(&agent), b"", &nonce, &ts(0));
    let r2 = app.request(request_from_signed(&genuine, vec![])).await;
    assert_eq!(r2.status(), StatusCode::OK);
}

#[tokio::test]
async fn same_nonce_for_two_agents_is_accepted() {
    let app = TestApp::setup().await;
    let a = app.register_agent().await;
    let b = app.register_agent().await;
    let nonce = TestAgent::nonce();

    let ra = a.sign_at("GET", &list_own(&a), b"", &nonce, &ts(0));
    let rb = b.sign_at("GET", &list_own(&b), b"", &nonce, &ts(0));
    assert_eq!(app.request(request_from_signed(&ra, vec![])).await.status(), StatusCode::OK);
    assert_eq!(app.request(request_from_signed(&rb, vec![])).await.status(), StatusCode::OK);
}

#[tokio::test]
async fn nonce_is_retained_for_at_least_twice_the_skew() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;
    let nonce = TestAgent::nonce();

    let signed = agent.sign_at("GET", &list_own(&agent), b"", &nonce, &ts(0));
    assert_eq!(app.request(request_from_signed(&signed, vec![])).await.status(), StatusCode::OK);

    // A request dated up to +300s stays valid until ~+600s after it was first
    // seen, so the replay cache must outlive that window.
    let remaining: f64 = sqlx::query_scalar(
        "SELECT EXTRACT(EPOCH FROM (expires_at - NOW()))::float8
         FROM nonce_replay_cache WHERE agent_id = $1 AND nonce = $2",
    )
    .bind(&agent.id)
    .bind(&nonce)
    .fetch_one(app.pool())
    .await
    .expect("nonce row should exist");
    assert!(remaining >= 590.0, "nonce retained only {remaining}s, need >= 600s");
}

#[tokio::test]
async fn suspended_agent_is_401() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;

    sqlx::query("UPDATE agents SET status = 'SUSPENDED' WHERE agent_id = $1")
        .bind(&agent.id)
        .execute(app.pool())
        .await
        .unwrap();

    let resp = app.send_signed(&agent, "GET", &list_own(&agent), None).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn oversized_body_is_413() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;

    let big = vec![b'x'; 1_100_000];
    let signed = agent.sign("POST", "/v1/messages", &big);
    let resp = app.request(request_from_signed(&signed, big)).await;
    assert_eq!(resp.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn handler_still_receives_the_body_after_verification() {
    let app = TestApp::setup().await;
    let sender = app.register_agent().await;
    let receiver = app.register_agent().await;

    let body = json!({
        "from_agent_id": sender.id, "to_agent_id": receiver.id,
        "type": "note", "payload": {"hello": "world"}
    });
    let resp = app.send_signed(&sender, "POST", "/v1/messages", Some(&body)).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

// ---- credential header bounds and the 401 challenge --------------------------

/// A correctly signed GET with one credential header replaced afterwards.
fn with_header(
    agent: &TestAgent,
    name: &'static str,
    value: &str,
) -> axum::http::Request<axum::body::Body> {
    let signed = agent.sign_at("GET", &list_own(agent), b"", &TestAgent::nonce(), &ts(0));
    let mut req = request_from_signed(&signed, vec![]);
    req.headers_mut().insert(HeaderName::from_static(name), HeaderValue::from_str(value).unwrap());
    req
}

#[tokio::test]
async fn oversized_credential_headers_are_400() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;

    for (name, len) in [(X_AGENT_ID, 257), (X_AGENT_NONCE, 129), (X_AGENT_SIGNATURE, 257)] {
        let resp = app.request(with_header(&agent, name, &"a".repeat(len))).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "{name} of {len} bytes");
    }
}

#[tokio::test]
async fn empty_credential_headers_are_400() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;

    for name in [X_AGENT_ID, X_AGENT_TIMESTAMP, X_AGENT_NONCE, X_AGENT_SIGNATURE] {
        let resp = app.request(with_header(&agent, name, "")).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST, "empty {name}");
    }
}

/// The limit is inclusive: a 128-byte nonce is accepted, a 129-byte one is not.
#[tokio::test]
async fn a_nonce_exactly_at_the_length_limit_is_accepted() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;

    let nonce = "n".repeat(127) + &uuid::Uuid::new_v4().simple().to_string()[..1];
    assert_eq!(nonce.len(), 128);
    let signed = agent.sign_at("GET", &list_own(&agent), b"", &nonce, &ts(0));
    let resp = app.request(request_from_signed(&signed, vec![])).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[tokio::test]
async fn unauthorized_responses_carry_the_authentication_challenge() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;

    // No credentials at all.
    let none = app.get(&list_own(&agent)).await;
    assert_eq!(none.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(none.headers().get("www-authenticate").unwrap(), "ACP1-Ed25519");

    // Credentials that fail verification.
    let forged = TestAgent::impostor_of(&agent.id);
    let bad = app.send_signed(&forged, "GET", &list_own(&agent), None).await;
    assert_eq!(bad.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(bad.headers().get("www-authenticate").unwrap(), "ACP1-Ed25519");

    // A malformed request is a 400 and is not a challenge.
    let stale = agent.sign_at("GET", &list_own(&agent), b"", &TestAgent::nonce(), &ts(-3600));
    let resp = app.request(request_from_signed(&stale, vec![])).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert!(resp.headers().get("www-authenticate").is_none());
}

/// A header value with bytes outside visible ASCII (legal on the wire as
/// obs-text, so the HTTP layer lets it through) is a malformed credential: 400,
/// not "no credentials" (401) and not a panic.
#[tokio::test]
async fn non_text_credential_header_is_400() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;

    let signed = agent.sign("GET", &list_own(&agent), b"");
    let mut req = request_from_signed(&signed, vec![]);
    req.headers_mut().insert(
        "x-agent-id",
        axum::http::HeaderValue::from_bytes(&[0xff, 0xfe, b'x'])
            .expect("obs-text is a legal value"),
    );
    let resp = app.request(req).await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    assert!(body_text(resp).await.contains("not valid text"));
}
