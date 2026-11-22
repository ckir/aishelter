//! Authentication and ownership for the agent registry routes.
//!
//! Registration and lookup stay public; changing an agent's card requires a
//! signature from that same agent.

use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::harness::TestApp;

fn card_body() -> Value {
    json!({ "name": "Renamed", "description": "changed" })
}

async fn profile_name(app: &TestApp, id: &str) -> Value {
    let resp = app.get(&format!("/v1/agents/{id}")).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
    let json: Value = TestApp::json_body(resp).await;
    json["profile_name"].clone()
}

#[tokio::test]
async fn card_update_without_credentials_is_401() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;
    let before = profile_name(&app, &agent.id).await;

    let resp = app.put_json(&format!("/v1/agents/{}/card", agent.id), card_body()).await;
    assert_eq!(TestApp::status(&resp), StatusCode::UNAUTHORIZED);
    assert_eq!(profile_name(&app, &agent.id).await, before);
}

#[tokio::test]
async fn card_update_by_another_agent_is_403() {
    let app = TestApp::setup().await;
    let owner = app.register_agent().await;
    let intruder = app.register_agent().await;
    let before = profile_name(&app, &owner.id).await;

    let path = format!("/v1/agents/{}/card", owner.id);
    let resp = app.send_signed(&intruder, "PUT", &path, Some(&card_body())).await;
    assert_eq!(TestApp::status(&resp), StatusCode::FORBIDDEN);
    assert_eq!(profile_name(&app, &owner.id).await, before);
}

#[tokio::test]
async fn card_update_by_the_agent_itself_is_200() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;

    let path = format!("/v1/agents/{}/card", agent.id);
    let resp = app.send_signed(&agent, "PUT", &path, Some(&card_body())).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
    assert_eq!(profile_name(&app, &agent.id).await, json!("Renamed"));
}

#[tokio::test]
async fn registration_and_lookup_stay_public() {
    let app = TestApp::setup().await;
    // `register_agent` posts to /v1/agents/register without credentials.
    let agent = app.register_agent().await;
    let resp = app.get(&format!("/v1/agents/{}", agent.id)).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
}
