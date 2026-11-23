//! Authentication and identity binding for `POST /v1/validations/{id}/validate`.
//!
//! The verified caller must be the validator named in the body. Who is ELIGIBLE
//! to validate (excluding the parties, sybil resistance) is a later slice.

use axum::http::StatusCode;
use serde_json::json;

use crate::harness::TestApp;

fn vote(validator: &str) -> serde_json::Value {
    json!({ "validator_id": validator, "decision": "approve", "reasoning": "ok" })
}

#[tokio::test]
async fn validating_requires_credentials() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;
    let task = app.create_task(&agent).await;
    let resp = app.post_json(&format!("/v1/validations/{task}/validate"), vote(&agent.id)).await;
    assert_eq!(TestApp::status(&resp), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn cannot_cast_a_vote_as_another_validator() {
    let app = TestApp::setup().await;
    let requester = app.register_agent().await;
    let attacker = app.register_agent().await;
    let victim = app.register_agent().await;
    let task = app.create_task(&requester).await;

    let path = format!("/v1/validations/{task}/validate");
    let resp = app.send_signed(&attacker, "POST", &path, Some(&vote(&victim.id))).await;
    assert_eq!(TestApp::status(&resp), StatusCode::FORBIDDEN);

    let votes: i64 = sqlx::query_scalar("SELECT count(*) FROM validations WHERE task_id = $1")
        .bind(&task)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(votes, 0, "no vote was recorded in the victim's name");
}

#[tokio::test]
async fn a_validator_votes_as_themselves() {
    let app = TestApp::setup().await;
    let requester = app.register_agent().await;
    let validator = app.register_agent().await;
    let task = app.create_task(&requester).await;

    let path = format!("/v1/validations/{task}/validate");
    let resp = app.send_signed(&validator, "POST", &path, Some(&vote(&validator.id))).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);

    let recorded: String =
        sqlx::query_scalar("SELECT validator_agent_id FROM validations WHERE task_id = $1")
            .bind(&task)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(recorded, validator.id);
}

#[tokio::test]
async fn reading_validation_history_stays_public() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;
    let task = app.create_task(&agent).await;
    let resp = app.get(&format!("/v1/validations/{task}")).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
}
