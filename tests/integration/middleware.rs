//! Integration tests for the idempotency middleware.
//!
//! Covers:
//! - Idempotency: duplicate requests with same key return cached response
//! - Idempotency: different keys execute independently
//! - Idempotency: missing key passes through normally
//!
//! Request authentication and nonce replay are covered in `auth.rs`.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use ed25519_dalek::SigningKey;
use rand_core::OsRng;
use serde_json::json;

use crate::harness::TestApp;

fn make_keypair() -> (String, String) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let public_key = signing_key.verifying_key();
    let agent_id = format!("agent_test_{}", hex::encode(public_key.to_bytes()));
    let public_key_hex = hex::encode(public_key.to_bytes());
    (agent_id, public_key_hex)
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

    // Unique per run: the dev database persists between runs.
    let idempotency_key = format!("idem-{agent_id}");
    let idempotency_key = idempotency_key.as_str();

    // First request — should execute normally
    let json_str = serde_json::to_string(&body).unwrap();
    let uri = "/v1/agents/register".parse::<axum::http::Uri>().unwrap();
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
        .uri("/v1/agents/register".parse::<axum::http::Uri>().unwrap())
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
        .uri("/v1/agents/register".parse::<axum::http::Uri>().unwrap())
        .header(header::CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", format!("idem-{agent_id1}"))
        .body(Body::from(json_str1))
        .unwrap();
    let resp1 = app.request(req1).await;
    assert_eq!(TestApp::status(&resp1), StatusCode::OK);

    let req2 = Request::builder()
        .method("POST")
        .uri("/v1/agents/register".parse::<axum::http::Uri>().unwrap())
        .header(header::CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", format!("idem-{agent_id2}"))
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
