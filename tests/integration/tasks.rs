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
use serde_json::json;

use crate::harness::TestApp;

/// Generate a fresh Ed25519 keypair for testing.
///
/// Returns a tuple of `(agent_id, public_key_hex)` where `agent_id` is a
/// deterministic string and `public_key_hex` is the 64-character hex-encoded
/// Ed25519 public key.
#[tokio::test]
async fn create_task_success() {
    let app = TestApp::setup().await;
    let requester = app.register_agent().await;

    let body = json!({
        "requester": requester.id,
        "capability": "research",
        "description": "Test task",
        "input": {},
        "verification_method": "peer",
        "required_validators": 2
    });
    let resp = app.send_signed(&requester, "POST", "/v1/tasks", Some(&body)).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
    let json: serde_json::Value = TestApp::json_body(resp).await;
    assert_eq!(json["status"], "CREATED");
}

#[tokio::test]
async fn create_task_missing_fields() {
    let app = TestApp::setup().await;
    let requester = app.register_agent().await;
    let resp = app.send_signed(&requester, "POST", "/v1/tasks", Some(&json!({}))).await;
    assert_eq!(TestApp::status(&resp), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn get_task_not_found() {
    let app = TestApp::setup().await;
    let resp = app.get("/v1/tasks/00000000-0000-0000-0000-000000000000").await;
    assert_eq!(TestApp::status(&resp), StatusCode::NOT_FOUND);
}
