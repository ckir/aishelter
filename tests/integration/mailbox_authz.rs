//! Identity binding for `/v1/messages`: the verified caller, not the request
//! body or query string, decides who may send, list, read and acknowledge.
//!
//! Status contract:
//! - sending as someone else, or listing someone else's mailbox: 403
//! - reading or acknowledging a message the caller is not party to: 404
//!   (indistinguishable from a message that does not exist)
//! - a malformed message id: 400

use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::harness::{TestAgent, TestApp};

/// Send a message from `from` to `to`; return its id.
async fn send(app: &TestApp, from: &TestAgent, to: &TestAgent) -> String {
    let body = json!({
        "from_agent_id": from.id,
        "to_agent_id": to.id,
        "type": "test",
        "payload": {"n": 1}
    });
    let resp = app.send_signed(from, "POST", "/v1/messages", Some(&body)).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
    let json: Value = TestApp::json_body(resp).await;
    json["message_id"].as_str().expect("message_id").to_string()
}

async fn inbox_len(app: &TestApp, owner: &TestAgent) -> usize {
    let path = format!("/v1/messages?agent_id={}&unacknowledged=false", owner.id);
    let resp = app.send_signed(owner, "GET", &path, None).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
    let json: Value = TestApp::json_body(resp).await;
    json["messages"].as_array().map_or(0, Vec::len)
}

// ---- send ----------------------------------------------------------------

#[tokio::test]
async fn cannot_send_as_another_agent() {
    let app = TestApp::setup().await;
    let attacker = app.register_agent().await;
    let victim = app.register_agent().await;
    let target = app.register_agent().await;

    let body = json!({
        "from_agent_id": victim.id,
        "to_agent_id": target.id,
        "type": "test",
        "payload": {}
    });
    let resp = app.send_signed(&attacker, "POST", "/v1/messages", Some(&body)).await;
    assert_eq!(TestApp::status(&resp), StatusCode::FORBIDDEN);
    let json: Value = TestApp::json_body(resp).await;
    assert!(json["error"].is_string(), "403 carries a JSON error body: {json}");

    // The forged message must not have been stored.
    assert_eq!(inbox_len(&app, &target).await, 0);
}

// ---- list ----------------------------------------------------------------

#[tokio::test]
async fn cannot_list_another_agents_mailbox() {
    let app = TestApp::setup().await;
    let sender = app.register_agent().await;
    let owner = app.register_agent().await;
    let snoop = app.register_agent().await;
    send(&app, &sender, &owner).await;

    let path = format!("/v1/messages?agent_id={}", owner.id);
    let resp = app.send_signed(&snoop, "GET", &path, None).await;
    assert_eq!(TestApp::status(&resp), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn sender_cannot_list_the_recipients_mailbox() {
    let app = TestApp::setup().await;
    let sender = app.register_agent().await;
    let owner = app.register_agent().await;
    send(&app, &sender, &owner).await;

    let path = format!("/v1/messages?agent_id={}", owner.id);
    let resp = app.send_signed(&sender, "GET", &path, None).await;
    assert_eq!(TestApp::status(&resp), StatusCode::FORBIDDEN);
}

// ---- get one -------------------------------------------------------------

#[tokio::test]
async fn sender_and_recipient_can_read_a_message() {
    let app = TestApp::setup().await;
    let sender = app.register_agent().await;
    let recipient = app.register_agent().await;
    let id = send(&app, &sender, &recipient).await;
    let path = format!("/v1/messages/{id}");

    for party in [&sender, &recipient] {
        let resp = app.send_signed(party, "GET", &path, None).await;
        assert_eq!(TestApp::status(&resp), StatusCode::OK);
        let json: Value = TestApp::json_body(resp).await;
        assert_eq!(json["data"]["message_id"], id.as_str());
    }
}

#[tokio::test]
async fn third_party_cannot_read_a_message() {
    let app = TestApp::setup().await;
    let sender = app.register_agent().await;
    let recipient = app.register_agent().await;
    let outsider = app.register_agent().await;
    let id = send(&app, &sender, &recipient).await;

    let resp = app.send_signed(&outsider, "GET", &format!("/v1/messages/{id}"), None).await;
    assert_eq!(TestApp::status(&resp), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn unknown_message_is_404() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;
    let path = format!("/v1/messages/{}", uuid::Uuid::new_v4());
    let resp = app.send_signed(&agent, "GET", &path, None).await;
    assert_eq!(TestApp::status(&resp), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn malformed_message_id_is_400_on_get() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;
    let resp = app.send_signed(&agent, "GET", "/v1/messages/not-a-uuid", None).await;
    assert_eq!(TestApp::status(&resp), StatusCode::BAD_REQUEST);
}

// ---- acknowledge ---------------------------------------------------------

#[tokio::test]
async fn only_the_recipient_can_acknowledge() {
    let app = TestApp::setup().await;
    let sender = app.register_agent().await;
    let recipient = app.register_agent().await;
    let outsider = app.register_agent().await;
    let id = send(&app, &sender, &recipient).await;
    let ack = format!("/v1/messages/{id}/ack");

    // Neither the sender nor an outsider may acknowledge on the recipient's behalf.
    for impostor in [&sender, &outsider] {
        let resp = app.send_signed(impostor, "POST", &ack, None).await;
        assert_eq!(TestApp::status(&resp), StatusCode::NOT_FOUND);
    }

    // The failed attempts left the message unacknowledged.
    let get = app.send_signed(&recipient, "GET", &format!("/v1/messages/{id}"), None).await;
    let json: Value = TestApp::json_body(get).await;
    assert_eq!(json["data"]["acknowledged"], false);

    let resp = app.send_signed(&recipient, "POST", &ack, None).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
    let get = app.send_signed(&recipient, "GET", &format!("/v1/messages/{id}"), None).await;
    let json: Value = TestApp::json_body(get).await;
    assert_eq!(json["data"]["acknowledged"], true);
}

#[tokio::test]
async fn malformed_message_id_is_400_on_ack() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;
    let resp = app.send_signed(&agent, "POST", "/v1/messages/not-a-uuid/ack", None).await;
    assert_eq!(TestApp::status(&resp), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn unknown_message_ack_is_404() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;
    let path = format!("/v1/messages/{}/ack", uuid::Uuid::new_v4());
    let resp = app.send_signed(&agent, "POST", &path, None).await;
    assert_eq!(TestApp::status(&resp), StatusCode::NOT_FOUND);
}
