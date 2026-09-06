//! Integration tests for `/v1/agents/{id}/reputation` endpoints.
//!
//! Covers reputation retrieval:
//! - Getting reputation for a registered agent returns 200
//! - Getting reputation for a nonexistent agent returns 4xx
//!
//! Each test is self-contained and duplicates the `make_keypair` and
//! `register_agent` helpers so this file compiles independently.

use axum::http::StatusCode;
use ed25519_dalek::SigningKey;
use hex::encode;
use rand_core::OsRng;
use serde_json::json;

use crate::harness::TestApp;

/// Generate a fresh Ed25519 keypair for testing.
///
/// Returns a tuple of `(agent_id, public_key_hex)` where `agent_id` is a
/// deterministic string and `public_key_hex` is the 64-character hex-encoded
/// Ed25519 public key.
fn make_keypair() -> (String, String) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let public_key = signing_key.verifying_key();
    let agent_id = format!("agent_test_{}", hex::encode(public_key.to_bytes()));
    let public_key_hex = encode(public_key.to_bytes());
    (agent_id, public_key_hex)
}

/// Register a new agent and return its `agent_id`.
async fn register_agent(app: &TestApp) -> String {
    let (agent_id, public_key) = make_keypair();
    let body = json!({
        "agent_id": agent_id,
        "public_key": public_key,
        "profile": {
            "name": "Test Agent",
            "description": "A test agent for reputation integration tests"
        }
    });
    let resp = app.post_json("/v1/agents/register", body).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK, "register_agent helper: expected 200 OK");
    let resp_json: serde_json::Value = TestApp::json_body(resp).await;
    resp_json["agent_id"].as_str().unwrap().to_string()
}

/// GET /v1/agents/{id}/reputation — registered agent returns 200.
#[tokio::test]
async fn get_reputation_success() {
    let app = TestApp::setup().await;
    let id = register_agent(&app).await;
    let resp = app.get(&format!("/v1/agents/{}/reputation", id)).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
}

/// GET /v1/agents/{id}/reputation — nonexistent agent returns 4xx.
#[tokio::test]
async fn get_reputation_not_found() {
    let app = TestApp::setup().await;
    let resp = app.get("/v1/agents/nonexistent/reputation").await;
    assert!(TestApp::status(&resp).is_client_error());
}
