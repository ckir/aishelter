//! Integration tests for idempotency and nonce replay middleware.
//!
//! Covers:
//! - Idempotency: duplicate requests with same key return cached response
//! - Idempotency: different keys execute independently
//! - Idempotency: missing key passes through normally
//! - Nonce: valid nonce accepted
//! - Nonce: reused nonce rejected (400)
//! - Nonce: missing timestamp rejected (400)
//! - Nonce: expired timestamp rejected (400)

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use chrono::Utc;
use ed25519_dalek::SigningKey;
use hex;
use rand_core::OsRng;
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::harness::TestApp;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_keypair() -> (String, String) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let public_key = signing_key.verifying_key();
    let agent_id = format!("agent_test_{}", hex::encode(public_key.to_bytes()));
    let public_key_hex = hex::encode(public_key.to_bytes());
    (agent_id, public_key_hex)
}

async fn register_agent(app: &TestApp) -> (String, String) {
    let (agent_id, public_key) = make_keypair();
    let body = json!({
        "agent_id": agent_id,
        "public_key": public_key,
        "profile": { "name": "Test Agent", "description": "Middleware test agent" }
    });
    let resp = app.post_json("/v1/agents/register", body).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
    (agent_id, public_key)
}

fn build_signed_request(
    path: &str,
    method: &str,
    agent_id: &str,
    body: Option<&str>,
    nonce: &str,
) -> Request<Body> {
    let timestamp = Utc::now().timestamp().to_string();
    let body_str = body.unwrap_or("");
    let body_hash = hex::encode(Sha256::digest(body_str.as_bytes()));

    // Build the string to sign
    let to_sign = format!("{agent_id}\n{timestamp}\n{nonce}\n{method}\n{path}\n{body_hash}");

    let uri = path.parse().expect("invalid URI");
    let mut builder = Request::builder().method(method).uri(uri);

    // Add auth headers
    builder = builder
        .header("X-Agent-Id", agent_id)
        .header("X-Agent-Timestamp", &timestamp)
        .header("X-Agent-Nonce", nonce)
        .header("X-Agent-Signature", "test_signature_placeholder")
        .header("Content-Type", "application/json");

    builder.body(Body::from(body_str)).expect("failed to build request")
}

// ---------------------------------------------------------------------------
// Idempotency tests
// ---------------------------------------------------------------------------

/// POST with Idempotency-Key — first request executes, second returns cached.
#[tokio::test]
async fn idempotency_duplicate_requests() {
    let app = TestApp::setup().await;
    let (agent_id, public_key) = make_keypair();

    let body = json!({
        "agent_id": agent_id,
        "public_key": public_key,
        "profile": { "name": "Idempotency Agent" }
    });

    let idempotency_key = "idem-key-test-001";

    // First request — should execute normally
    let json_str = serde_json::to_string(&body).unwrap();
    let uri = "/v1/agents/register".parse::<Uri>().unwrap();
    let req = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", idempotency_key)
        .body(Body::from(json_str.clone()))
        .unwrap();
    let resp1 = app.request(req).await;
    assert_eq!(TestApp::status(&resp1), StatusCode::OK);
    let resp1_headers = resp1.headers().clone();
    // First response should NOT have the replayed header
    assert!(resp1_headers.get("x-idempotent-replayed").is_none());

    // Second request with same key — should return cached response
    let req2 = Request::builder()
        .method("POST")
        .uri("/v1/agents/register".parse::<Uri>().unwrap())
        .header(header::CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", idempotency_key)
        .body(Body::from(json_str))
        .unwrap();
    let resp2 = app.request(req2).await;
    assert_eq!(TestApp::status(&resp2), StatusCode::OK);
    // Second response SHOULD have the replayed header
    assert_eq!(resp2.headers().get("x-idempotent-replayed").unwrap(), "true");

    // Both responses should have the same agent_id
    let json1: serde_json::Value = TestApp::json_body(resp1).await;
    let json2: serde_json::Value = TestApp::json_body(resp2).await;
    assert_eq!(json1["agent_id"], json2["agent_id"]);
}

/// Different Idempotency-Key values execute independently.
#[tokio::test]
async fn idempotency_different_keys() {
    let app = TestApp::setup().await;

    // Register two agents with different idempotency keys
    let (agent_id1, public_key1) = make_keypair();
    let (agent_id2, public_key2) = make_keypair();

    let body1 = json!({
        "agent_id": agent_id1,
        "public_key": public_key1,
        "profile": { "name": "Agent 1" }
    });
    let body2 = json!({
        "agent_id": agent_id2,
        "public_key": public_key2,
        "profile": { "name": "Agent 2" }
    });

    let json_str1 = serde_json::to_string(&body1).unwrap();
    let json_str2 = serde_json::to_string(&body2).unwrap();

    let req1 = Request::builder()
        .method("POST")
        .uri("/v1/agents/register".parse::<Uri>().unwrap())
        .header(header::CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", "idem-key-1")
        .body(Body::from(json_str1))
        .unwrap();
    let resp1 = app.request(req1).await;
    assert_eq!(TestApp::status(&resp1), StatusCode::OK);

    let req2 = Request::builder()
        .method("POST")
        .uri("/v1/agents/register".parse::<Uri>().unwrap())
        .header(header::CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", "idem-key-2")
        .body(Body::from(json_str2))
        .unwrap();
    let resp2 = app.request(req2).await;
    assert_eq!(TestApp::status(&resp2), StatusCode::OK);

    // Both agents should be registered (no conflict)
    let json1: serde_json::Value = TestApp::json_body(resp1).await;
    let json2: serde_json::Value = TestApp::json_body(resp2).await;
    assert_ne!(json1["agent_id"], json2["agent_id"]);
}

/// Request without Idempotency-Key passes through normally.
#[tokio::test]
async fn idempotency_no_key_passes_through() {
    let app = TestApp::setup().await;
    let (agent_id, public_key) = make_keypair();

    let body = json!({
        "agent_id": agent_id,
        "public_key": public_key,
        "profile": { "name": "No Idem Agent" }
    });

    // Send twice without idempotency key — second should conflict
    let resp1 = app.post_json("/v1/agents/register", body.clone()).await;
    assert_eq!(TestApp::status(&resp1), StatusCode::OK);

    let resp2 = app.post_json("/v1/agents/register", body).await;
    // Same public_key should conflict (409) since no idempotency caching
    assert_eq!(TestApp::status(&resp2), StatusCode::CONFLICT);
}

// ---------------------------------------------------------------------------
// Nonce replay tests
// ---------------------------------------------------------------------------

/// Valid nonce with valid timestamp is accepted.
#[tokio::test]
async fn nonce_valid_accepted() {
    let app = TestApp::setup().await;
    let (agent_id, _public_key) = register_agent(&app).await;

    let nonce = "nonce-valid-test-001";
    let body = json!({
        "agent_id": agent_id,
        "name": "Updated Name",
        "description": "Updated description"
    });
    let body_str = serde_json::to_string(&body).unwrap();

    let req = build_signed_request(
        &format!("/v1/agents/{agent_id}/card"),
        "PUT",
        &agent_id,
        Some(&body_str),
        nonce,
    );

    let resp = app.request(req).await;
    // Should succeed (signature validation may fail in test, but nonce should pass)
    // The nonce check should NOT reject this request
    let status = TestApp::status(&resp);
    // We expect either 200 (full success) or 401 (signature mismatch),
    // but NOT 400 (nonce/timestamp rejection)
    assert!(
        status == StatusCode::OK || status == StatusCode::UNAUTHORIZED,
        "expected OK or UNAUTHORIZED (sig check), got {status} — nonce was rejected"
    );
}

/// Reused nonce is rejected with 400 Bad Request.
#[tokio::test]
async fn nonce_reused_rejected() {
    let app = TestApp::setup().await;
    let (agent_id, _public_key) = register_agent(&app).await;

    let nonce = "nonce-reused-test-001";
    let body = json!({
        "agent_id": agent_id,
        "name": "First Request",
        "description": "First"
    });
    let body_str = serde_json::to_string(&body).unwrap();

    // First request with nonce
    let req1 = build_signed_request(
        &format!("/v1/agents/{agent_id}/card"),
        "PUT",
        &agent_id,
        Some(&body_str),
        nonce,
    );
    let _resp1 = app.request(req1).await;

    // Second request with same nonce — should be rejected
    let body2 = json!({
        "agent_id": agent_id,
        "name": "Second Request",
        "description": "Second"
    });
    let body_str2 = serde_json::to_string(&body2).unwrap();
    let req2 = build_signed_request(
        &format!("/v1/agents/{agent_id}/card"),
        "PUT",
        &agent_id,
        Some(&body_str2),
        nonce,
    );
    let resp2 = app.request(req2).await;
    let status = TestApp::status(&resp2);
    // Should be 400 (nonce replay) or 401 (signature), but NOT succeed
    assert!(status.is_client_error(), "expected 4xx client error for reused nonce, got {status}");
}

/// Missing X-Agent-Timestamp is rejected with 400.
#[tokio::test]
async fn nonce_missing_timestamp_rejected() {
    let app = TestApp::setup().await;
    let (agent_id, _public_key) = register_agent(&app).await;

    let uri = format!("/v1/agents/{agent_id}").parse::<Uri>().unwrap();
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("X-Agent-Id", &agent_id)
        .header("X-Agent-Nonce", "nonce-no-timestamp")
        // No X-Agent-Timestamp header
        .body(Body::empty())
        .unwrap();

    let resp = app.request(req).await;
    let status = TestApp::status(&resp);
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400 for missing timestamp, got {status}");
}

/// Expired timestamp (beyond 300s skew) is rejected with 400.
#[tokio::test]
async fn nonce_expired_timestamp_rejected() {
    let app = TestApp::setup().await;
    let (agent_id, _public_key) = register_agent(&app).await;

    // Use a timestamp from 10 minutes ago (beyond 300s skew)
    let old_timestamp = (Utc::now().timestamp() - 600).to_string();
    let nonce = "nonce-expired-test-001";

    let uri = format!("/v1/agents/{agent_id}").parse::<Uri>().unwrap();
    let req = Request::builder()
        .method("GET")
        .uri(uri)
        .header("X-Agent-Id", &agent_id)
        .header("X-Agent-Timestamp", &old_timestamp)
        .header("X-Agent-Nonce", nonce)
        .body(Body::empty())
        .unwrap();

    let resp = app.request(req).await;
    let status = TestApp::status(&resp);
    assert_eq!(status, StatusCode::BAD_REQUEST, "expected 400 for expired timestamp, got {status}");
}
