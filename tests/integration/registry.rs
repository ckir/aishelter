//! Integration tests for `/v1/agents/*` registry endpoints.
//!
//! Covers the full agent registration lifecycle:
//! - Successful registration with a valid Ed25519 public key
//! - Duplicate public key rejection (409 Conflict)
//! - Missing required fields (4xx Bad Request)
//! - Retrieval of a registered agent by ID
//! - Agent not found (404)
//! - Updating an agent's card (name and description)
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
/// deterministic UUID-style string and `public_key_hex` is the 64-character
/// hex-encoded Ed25519 public key.
fn make_keypair() -> (String, String) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let public_key = signing_key.verifying_key();
    let agent_id = format!("agent_test_{}", hex::encode(public_key.to_bytes()));
    let public_key_hex = encode(public_key.to_bytes());
    (agent_id, public_key_hex)
}

/// Register a new agent and return its `agent_id`.
///
/// Uses a freshly generated Ed25519 keypair and includes minimal profile
/// fields in the registration request body.
async fn register_agent(app: &TestApp) -> String {
    let (agent_id, public_key) = make_keypair();
    let body = json!({
        "agent_id": agent_id,
        "public_key": public_key,
        "profile": {
            "name": "Test Agent",
            "description": "A test agent for integration tests"
        }
    });
    let resp = app.post_json("/v1/agents/register", body).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK, "register_agent helper: expected 200 OK");
    let resp_json: serde_json::Value = TestApp::json_body(resp).await;
    resp_json["agent_id"].as_str().unwrap().to_string()
}

/// POST /v1/agents/register — valid registration returns 200 with correct body.
#[tokio::test]
async fn register_agent_success() {
    let app = TestApp::setup().await;
    let (agent_id, public_key) = make_keypair();

    let body = json!({
        "agent_id": agent_id,
        "public_key": public_key,
        "profile": {
            "name": "Integration Test Agent",
            "description": "Created by register_agent_success test"
        }
    });

    let resp = app.post_json("/v1/agents/register", body).await;
    let status = TestApp::status(&resp);
    assert_eq!(status, StatusCode::OK);

    let json: serde_json::Value = TestApp::json_body(resp).await;
    assert_eq!(json["agent_id"].as_str().unwrap(), agent_id);
    assert_eq!(json["status"].as_str().unwrap(), "REGISTERED");
    assert_eq!(json["protocol"].as_str().unwrap(), "acp/1");
}

/// POST /v1/agents/register — same public key returns 409 Conflict.
#[tokio::test]
async fn register_agent_duplicate_public_key() {
    let app = TestApp::setup().await;
    let (_agent_id, public_key) = make_keypair();

    // First registration should succeed.
    let body1 = json!({
        "agent_id": "agent_first",
        "public_key": public_key,
        "profile": { "name": "First Agent" }
    });
    let resp1 = app.post_json("/v1/agents/register", body1).await;
    assert_eq!(TestApp::status(&resp1), StatusCode::OK);

    // Second registration with the same public key should fail.
    let body2 = json!({
        "agent_id": "agent_second",
        "public_key": public_key,
        "profile": { "name": "Second Agent" }
    });
    let resp2 = app.post_json("/v1/agents/register", body2).await;
    assert_eq!(TestApp::status(&resp2), StatusCode::CONFLICT);
}

/// POST /v1/agents/register — empty body returns 4xx Bad Request.
#[tokio::test]
async fn register_agent_missing_fields() {
    let app = TestApp::setup().await;

    // An empty JSON object lacks the required `agent_id` and `public_key` fields.
    let body = json!({});
    let resp = app.post_json("/v1/agents/register", body).await;
    let status = TestApp::status(&resp);
    assert!(status.is_client_error(), "expected 4xx client error, got {status}");
}

/// GET /v1/agents/{id} — returns 200 with agent details.
#[tokio::test]
async fn get_agent_success() {
    let app = TestApp::setup().await;
    let agent_id = register_agent(&app).await;

    let resp = app.get(&format!("/v1/agents/{agent_id}")).await;
    let status = TestApp::status(&resp);
    assert_eq!(status, StatusCode::OK);

    let json: serde_json::Value = TestApp::json_body(resp).await;
    assert_eq!(json["agent_id"].as_str().unwrap(), agent_id);
    assert_eq!(json["protocol"].as_str().unwrap(), "acp/1");
    assert!(json["public_key"].as_str().is_some());
}

/// GET /v1/agents/{id} — nonexistent agent returns 404.
#[tokio::test]
async fn get_agent_not_found() {
    let app = TestApp::setup().await;

    let resp = app.get("/v1/agents/nonexistent_agent_id").await;
    assert_eq!(TestApp::status(&resp), StatusCode::NOT_FOUND);
}

/// PUT /v1/agents/{id}/card — update name and description returns 200.
#[tokio::test]
async fn update_agent_card_success() {
    let app = TestApp::setup().await;
    let agent_id = register_agent(&app).await;

    let body = json!({
        "name": "Updated Agent Name",
        "description": "This description was updated by the integration test"
    });
    let resp = app.put_json(&format!("/v1/agents/{agent_id}/card"), body).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);

    // Verify the update persisted by fetching the agent again.
    let resp = app.get(&format!("/v1/agents/{agent_id}")).await;
    let json: serde_json::Value = TestApp::json_body(resp).await;
    assert_eq!(json["profile_name"].as_str().unwrap(), "Updated Agent Name");
    assert_eq!(
        json["profile_description"].as_str().unwrap(),
        "This description was updated by the integration test"
    );
}
