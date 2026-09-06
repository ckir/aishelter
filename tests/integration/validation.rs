//! Integration tests for `/v1/validations` endpoints.
//!
//! Covers the validation/verification lifecycle:
//! - Submitting a validation decision for a task
//! - Idempotent or conflict behaviour for duplicate validations
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
            "description": "A test agent for validation integration tests"
        }
    });
    let resp = app.post_json("/v1/agents/register", body).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK, "register_agent helper: expected 200 OK");
    let resp_json: serde_json::Value = TestApp::json_body(resp).await;
    resp_json["agent_id"].as_str().unwrap().to_string()
}

/// POST /v1/validations/validate — idempotent or conflict for duplicate validation.
#[tokio::test]
async fn validate_idempotent_or_conflict() {
    let app = TestApp::setup().await;
    let agent = register_agent(&app).await;

    // Create a task.
    let task_resp = app
        .post_json(
            "/v1/tasks",
            json!({
                "requester": agent,
                "capability": "test",
                "description": "test",
                "input": {},
                "verification_method": "peer",
                "required_validators": 1
            }),
        )
        .await;
    let task_body: serde_json::Value = TestApp::json_body(task_resp).await;
    let task_id = task_body["task_id"].as_str().unwrap();

    // First validation.
    let r1 = app
        .post_json(
            &format!("/v1/validations/{}/validate", task_id),
            json!({
                "validator_id": agent,
                "decision": "approve",
                "reasoning": "test"
            }),
        )
        .await;
    assert!(
        TestApp::status(&r1).is_success() || TestApp::status(&r1) == StatusCode::CONFLICT,
        "first validation should succeed or conflict"
    );

    // Second validation by same agent — should be idempotent (200) or conflict (409).
    let r2 = app
        .post_json(
            &format!("/v1/validations/{}/validate", task_id),
            json!({
                "validator_id": agent,
                "decision": "approve",
                "reasoning": "test duplicate"
            }),
        )
        .await;
    assert!(
        TestApp::status(&r2).is_success() || TestApp::status(&r2) == StatusCode::CONFLICT,
        "duplicate validation should be idempotent or conflict, not 500"
    );
}
