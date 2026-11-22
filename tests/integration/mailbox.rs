//! Integration tests for `/v1/messages` mailbox endpoints.
//!
//! Covers the asynchronous message lifecycle with signed requests:
//! - Sending a message between registered agents
//! - Retrieving messages from an agent's mailbox
//! - Acknowledging messages to mark them as processed
//! - Sending to a nonexistent agent (rejection)
//!
//! Identity binding (who may send as whom, who may read what) is covered in
//! `mailbox_authz.rs`.

use axum::http::StatusCode;
use serde_json::json;

use crate::harness::TestApp;

/// POST /v1/messages — valid message between registered agents returns 200.
#[tokio::test]
async fn send_message_success() {
    let app = TestApp::setup().await;
    let sender = app.register_agent().await;
    let receiver = app.register_agent().await;

    let body = json!({
        "from_agent_id": sender.id,
        "to_agent_id": receiver.id,
        "type": "task.offer",
        "payload": {"task": "research"}
    });
    let resp = app.send_signed(&sender, "POST", "/v1/messages", Some(&body)).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
}

/// GET /v1/messages — the recipient retrieves their own mailbox.
#[tokio::test]
async fn get_messages_success() {
    let app = TestApp::setup().await;
    let sender = app.register_agent().await;
    let receiver = app.register_agent().await;

    let body = json!({
        "from_agent_id": sender.id,
        "to_agent_id": receiver.id,
        "type": "test",
        "payload": {}
    });
    let sent = app.send_signed(&sender, "POST", "/v1/messages", Some(&body)).await;
    assert_eq!(TestApp::status(&sent), StatusCode::OK);

    let path = format!("/v1/messages?agent_id={}", receiver.id);
    let resp = app.send_signed(&receiver, "GET", &path, None).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
    let json: serde_json::Value = TestApp::json_body(resp).await;
    assert_eq!(json["messages"].as_array().map(Vec::len), Some(1));
}

/// POST /v1/messages — sending to a nonexistent agent returns 4xx.
#[tokio::test]
async fn send_message_nonexistent_agent() {
    let app = TestApp::setup().await;
    let sender = app.register_agent().await;

    let body = json!({
        "from_agent_id": sender.id,
        "to_agent_id": "nonexistent_agent_id",
        "type": "test",
        "payload": {}
    });
    let resp = app.send_signed(&sender, "POST", "/v1/messages", Some(&body)).await;
    assert!(TestApp::status(&resp).is_client_error());
}

/// POST /v1/messages/{id}/ack — the recipient acknowledges a message.
#[tokio::test]
async fn acknowledge_message_success() {
    let app = TestApp::setup().await;
    let sender = app.register_agent().await;
    let receiver = app.register_agent().await;

    let body = json!({
        "from_agent_id": sender.id,
        "to_agent_id": receiver.id,
        "type": "test",
        "payload": {}
    });
    let send_resp = app.send_signed(&sender, "POST", "/v1/messages", Some(&body)).await;
    assert_eq!(TestApp::status(&send_resp), StatusCode::OK);
    let sent: serde_json::Value = TestApp::json_body(send_resp).await;
    let msg_id = sent["message_id"].as_str().expect("response should contain message_id");

    let path = format!("/v1/messages/{msg_id}/ack");
    let resp = app.send_signed(&receiver, "POST", &path, None).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
}

/// POST /v1/messages — an unparseable `expires_at` is rejected, not silently
/// dropped (which would store an immortal message the sender meant to expire).
#[tokio::test]
async fn send_message_with_malformed_expires_at_is_400() {
    let app = TestApp::setup().await;
    let sender = app.register_agent().await;
    let receiver = app.register_agent().await;

    let body = json!({
        "from_agent_id": sender.id,
        "to_agent_id": receiver.id,
        "type": "test",
        "payload": {},
        "expires_at": "tomorrow-ish"
    });
    let resp = app.send_signed(&sender, "POST", "/v1/messages", Some(&body)).await;
    assert_eq!(TestApp::status(&resp), StatusCode::BAD_REQUEST);

    let path = format!("/v1/messages?agent_id={}&unacknowledged=false", receiver.id);
    let list = app.send_signed(&receiver, "GET", &path, None).await;
    let json: serde_json::Value = TestApp::json_body(list).await;
    assert_eq!(json["messages"].as_array().map(Vec::len), Some(0), "nothing was stored");
}

/// An expired message is gone everywhere: the list already hid it, and a lookup
/// by id or an ack must treat it as not found too.
#[tokio::test]
async fn expired_message_is_404_on_get_and_ack() {
    let app = TestApp::setup().await;
    let sender = app.register_agent().await;
    let receiver = app.register_agent().await;

    let body = json!({
        "from_agent_id": sender.id,
        "to_agent_id": receiver.id,
        "type": "test",
        "payload": {},
        "expires_at": "2000-01-01T00:00:00Z"
    });
    let sent = app.send_signed(&sender, "POST", "/v1/messages", Some(&body)).await;
    assert_eq!(TestApp::status(&sent), StatusCode::OK);
    let sent: serde_json::Value = TestApp::json_body(sent).await;
    let id = sent["message_id"].as_str().expect("message_id");

    let get = app.send_signed(&receiver, "GET", &format!("/v1/messages/{id}"), None).await;
    assert_eq!(TestApp::status(&get), StatusCode::NOT_FOUND);
    let ack = app.send_signed(&receiver, "POST", &format!("/v1/messages/{id}/ack"), None).await;
    assert_eq!(TestApp::status(&ack), StatusCode::NOT_FOUND);
}
