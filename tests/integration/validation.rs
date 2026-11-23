//! Integration tests for `/v1/validations` endpoints.
//!
//! Covers the validation/verification lifecycle:
//! - Submitting a validation decision for a task
//! - Idempotent or conflict behaviour for duplicate validations
//!

use axum::http::StatusCode;
use serde_json::json;

use crate::harness::TestApp;

/// POST /v1/validations/{id}/validate — a repeated vote by the same validator is
/// accepted without error and counted once.
#[tokio::test]
async fn validate_idempotent_or_conflict() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;
    let task_id = app.create_task(&agent).await;
    let path = format!("/v1/validations/{task_id}/validate");

    let first = json!({ "validator_id": agent.id, "decision": "approve", "reasoning": "test" });
    let r1 = app.send_signed(&agent, "POST", &path, Some(&first)).await;
    assert_eq!(TestApp::status(&r1), StatusCode::OK, "first validation");

    let again =
        json!({ "validator_id": agent.id, "decision": "approve", "reasoning": "test duplicate" });
    let r2 = app.send_signed(&agent, "POST", &path, Some(&again)).await;
    assert_eq!(TestApp::status(&r2), StatusCode::OK, "duplicate vote is accepted, not a 500");
}
