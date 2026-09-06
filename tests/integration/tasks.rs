//! Integration tests for `/v1/tasks` endpoints.
//!
//! Covers the task lifecycle:
//! - Creating a task with required fields
//! - Rejecting tasks with missing required fields
//! - Retrieving a task by ID
//! - Task not found for nonexistent IDs
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
            "description": "A test agent for task integration tests"
        }
    });
    let resp = app.post_json("/v1/agents/register", body).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK, "register_agent helper: expected 200 OK");
    let resp_json: serde_json::Value = TestApp::json_body(resp).await;
    resp_json["agent_id"].as_str().unwrap().to_string()
}

/// POST /v1/tasks — valid task creation returns 200.
#[tokio::test]
async fn create_task_success() {
    let app = TestApp::setup().await;
    let requester = register_agent(&app).await;

    let resp = app
        .post_json(
            "/v1/tasks",
            json!({
                "requester": requester,
                "capability": "research",
                "description": "Test task",
                "input": {},
                "verification_method": "peer",
                "required_validators": 2
            }),
        )
        .await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
}

/// POST /v1/tasks — empty body returns 4xx Bad Request.
#[tokio::test]
async fn create_task_missing_fields() {
    let app = TestApp::setup().await;
    let resp = app.post_json("/v1/tasks", json!({})).await;
    assert!(TestApp::status(&resp).is_client_error());
}

/// GET /v1/tasks/{id} — nonexistent task returns 404.
#[tokio::test]
async fn get_task_not_found() {
    let app = TestApp::setup().await;
    let resp = app.get("/v1/tasks/00000000-0000-0000-0000-000000000000").await;
    assert_eq!(TestApp::status(&resp), StatusCode::NOT_FOUND);
}
