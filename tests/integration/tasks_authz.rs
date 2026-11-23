//! Authentication and identity binding for the mutating `/v1/tasks` routes.
//!
//! Scope of this slice: a signature is required and the verified caller must
//! equal the identity named in the body. Role rules (who may accept, reject or
//! submit) are a later slice. The task lifecycle currently rejects accept,
//! reject and result on a freshly created task, so tests that need a request to
//! pass the gate assert only that it is not an auth failure; they must not pin
//! that lifecycle behaviour.

use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::harness::TestApp;

fn create_body(requester: &str) -> Value {
    json!({
        "requester": requester,
        "capability": "research",
        "description": "t",
        "input": {},
        "verification_method": "peer",
        "required_validators": 1
    })
}

fn result_body(agent: &str) -> Value {
    json!({ "agent_id": agent, "result": {}, "output_hash": "h" })
}

#[tokio::test]
async fn creating_a_task_requires_credentials() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;
    let resp = app.post_json("/v1/tasks", create_body(&agent.id)).await;
    assert_eq!(TestApp::status(&resp), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn cannot_create_a_task_as_another_requester() {
    let app = TestApp::setup().await;
    let attacker = app.register_agent().await;
    let victim = app.register_agent().await;

    let resp =
        app.send_signed(&attacker, "POST", "/v1/tasks", Some(&create_body(&victim.id))).await;
    assert_eq!(TestApp::status(&resp), StatusCode::FORBIDDEN);

    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM tasks WHERE requester_agent_id = $1")
        .bind(&victim.id)
        .fetch_one(app.pool())
        .await
        .unwrap();
    assert_eq!(count, 0, "no task was created in the victim's name");
}

#[tokio::test]
async fn a_task_is_created_in_the_callers_name() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;
    let id = app.create_task(&agent).await;
    let requester: String =
        sqlx::query_scalar("SELECT requester_agent_id FROM tasks WHERE task_id = $1")
            .bind(&id)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(requester, agent.id);
}

#[tokio::test]
async fn task_actions_require_credentials() {
    let app = TestApp::setup().await;
    let owner = app.register_agent().await;
    let id = app.create_task(&owner).await;
    for action in ["accept", "reject"] {
        let resp = app
            .post_json(&format!("/v1/tasks/{id}/{action}"), json!({ "agent_id": owner.id }))
            .await;
        assert_eq!(TestApp::status(&resp), StatusCode::UNAUTHORIZED, "{action}");
    }
    let resp = app.post_json(&format!("/v1/tasks/{id}/result"), result_body(&owner.id)).await;
    assert_eq!(TestApp::status(&resp), StatusCode::UNAUTHORIZED, "result");
}

#[tokio::test]
async fn task_actions_cannot_name_another_agent() {
    let app = TestApp::setup().await;
    let owner = app.register_agent().await;
    let attacker = app.register_agent().await;
    let id = app.create_task(&owner).await;

    // The identity check comes before any state check: a CREATED task would
    // otherwise answer 400 for these.
    for action in ["accept", "reject"] {
        let path = format!("/v1/tasks/{id}/{action}");
        let resp =
            app.send_signed(&attacker, "POST", &path, Some(&json!({ "agent_id": owner.id }))).await;
        assert_eq!(TestApp::status(&resp), StatusCode::FORBIDDEN, "{action}");
    }
    let path = format!("/v1/tasks/{id}/result");
    let resp = app.send_signed(&attacker, "POST", &path, Some(&result_body(&owner.id))).await;
    assert_eq!(TestApp::status(&resp), StatusCode::FORBIDDEN, "result");
}

#[tokio::test]
async fn naming_oneself_passes_the_gate_on_every_task_action() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;
    let id = app.create_task(&agent).await;
    let cases = [
        ("accept", json!({ "agent_id": agent.id })),
        ("reject", json!({ "agent_id": agent.id })),
        ("result", result_body(&agent.id)),
    ];
    for (action, body) in cases {
        let path = format!("/v1/tasks/{id}/{action}");
        let status = TestApp::status(&app.send_signed(&agent, "POST", &path, Some(&body)).await);
        assert_ne!(status, StatusCode::UNAUTHORIZED, "{action}");
        assert_ne!(status, StatusCode::FORBIDDEN, "{action}");
    }
}

#[tokio::test]
async fn reading_a_task_stays_public() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;
    let id = app.create_task(&agent).await;
    assert_eq!(TestApp::status(&app.get(&format!("/v1/tasks/{id}")).await), StatusCode::OK);
    // A missing task is a 404 from the handler, not a 401 from the auth layer.
    let resp = app.get("/v1/tasks/00000000-0000-0000-0000-000000000000").await;
    assert_eq!(TestApp::status(&resp), StatusCode::NOT_FOUND);
}
