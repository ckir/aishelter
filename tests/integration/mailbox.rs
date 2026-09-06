//! Integration tests for `/v1/messages` mailbox endpoints.
//!
//! Covers the asynchronous message lifecycle:
//! - Sending a message between registered agents
//! - Retrieving messages from an agent's mailbox
//! - Acknowledging messages to mark them as processed
//! - Sending to a nonexistent agent (rejection)
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
            "description": "A test agent for mailbox integration tests"
        }
    });
    let resp = app.post_json("/v1/agents/register", body).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK, "register_agent helper: expected 200 OK");
    let resp_json: serde_json::Value = TestApp::json_body(resp).await;
    resp_json["agent_id"].as_str().unwrap().to_string()
}

/// POST /v1/messages — valid message between registered agents returns 200.
#[tokio::test]
async fn send_message_success() {
    let app = TestApp::setup().await;
    let sender = register_agent(&app).await;
    let receiver = register_agent(&app).await;

    let resp = app
        .post_json(
            "/v1/messages",
            json!({
                "from_agent_id": sender,
                "to_agent_id": receiver,
                "type": "task.offer",
                "payload": {"task": "research"}
            }),
        )
        .await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
}

/// GET /v1/messages — retrieve messages for an agent returns 200.
#[tokio::test]
async fn get_messages_success() {
    let app = TestApp::setup().await;
    let sender = register_agent(&app).await;
    let receiver = register_agent(&app).await;

    // Send a message first.
    app.post_json(
        "/v1/messages",
        json!({
            "from_agent_id": sender,
            "to_agent_id": receiver,
            "type": "test",
            "payload": {}
        }),
    )
    .await;

    let resp = app.get(&format!("/v1/messages?agent_id={}", receiver)).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
}

/// POST /v1/messages — sending to a nonexistent agent returns 4xx.
#[tokio::test]
async fn send_message_nonexistent_agent() {
    let app = TestApp::setup().await;
    let sender = register_agent(&app).await;

    let resp = app
        .post_json(
            "/v1/messages",
            json!({
                "from_agent_id": sender,
                "to_agent_id": "nonexistent_agent_id",
                "type": "test",
                "payload": {}
            }),
        )
        .await;
    assert!(TestApp::status(&resp).is_client_error());
}

/// POST /v1/messages/{id}/ack — acknowledging a message returns 200.
#[tokio::test]
async fn acknowledge_message_success() {
    let app = TestApp::setup().await;
    let sender = register_agent(&app).await;
    let receiver = register_agent(&app).await;

    let send_resp = app
        .post_json(
            "/v1/messages",
            json!({
                "from_agent_id": sender,
                "to_agent_id": receiver,
                "type": "test",
                "payload": {}
            }),
        )
        .await;
    let body: serde_json::Value = TestApp::json_body(send_resp).await;
    let msg_id = body["message_id"].as_str().expect("response should contain message_id");

    let resp = app.post(&format!("/v1/messages/{}/ack", msg_id)).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
}
