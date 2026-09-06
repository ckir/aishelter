//! Integration tests for `/v1/discovery/*` endpoints.
//!
//! Covers capability-based agent discovery search:
//! - Searching by a capability that registered agents have returns results
//! - Searching for a nonexistent capability returns an empty OK response
//!
//! Each test is self-contained and duplicates the `make_keypair` and
//! `register_agent` helpers so this file compiles independently.
//!
//! Discovery requires agents to have capabilities registered in the
//! `agent_capabilities` table, so tests insert capability rows directly
//! into the database after agent registration.

use axum::http::StatusCode;
use ed25519_dalek::SigningKey;
use hex::encode;
use rand_core::OsRng;
use serde_json::json;

use crate::harness::TestApp;

/// Generate a fresh Ed25519 keypair for testing.
///
/// Returns a tuple of `(agent_id, public_key_hex)` where `agent_id` is a
/// deterministic UUID-style string and `public_key_hex` is the 64-character
/// hex-encoded Ed25519 public key.
fn make_keypair() -> (String, String) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let public_key = signing_key.verifying_key();
    let agent_id = format!("agent_test_{}", hex::encode(public_key.to_bytes()));
    let public_key_hex = encode(public_key.to_bytes());
    (agent_id, public_key_hex)
}

/// Register a new agent and return its `agent_id`.
///
/// Uses a freshly generated Ed25519 keypair and includes minimal profile
/// fields in the registration request body.
async fn register_agent(app: &TestApp) -> String {
    let (agent_id, public_key) = make_keypair();
    let body = json!({
        "agent_id": agent_id,
        "public_key": public_key,
        "profile": {
            "name": "Test Agent",
            "description": "A test agent for integration tests"
        }
    });
    let resp = app.post_json("/v1/agents/register", body).await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK, "register_agent helper: expected 200 OK");
    let resp_json: serde_json::Value = TestApp::json_body(resp).await;
    resp_json["agent_id"].as_str().unwrap().to_string()
}

/// Register an agent, insert a capability row, and return the agent_id.
///
/// The discovery search joins `agents` with `agent_capabilities`, so this
/// helper ensures the agent is discoverable by the given capability name.
async fn register_agent_with_capability(app: &TestApp, capability: &str, version: &str) -> String {
    let agent_id = register_agent(app).await;

    // Insert a capability row so the agent appears in discovery results.
    sqlx::query(
        r#"
        INSERT INTO agent_capabilities (agent_id, capability_name, capability_version)
        VALUES ($1, $2, $3)
        "#,
    )
    .bind(&agent_id)
    .bind(capability)
    .bind(version)
    .execute(app.pool())
    .await
    .expect("failed to insert agent_capability");

    agent_id
}

/// GET /v1/discovery/search — search returns agents with matching capability.
#[tokio::test]
async fn discovery_search_by_capability() {
    let app = TestApp::setup().await;
    let agent_id = register_agent_with_capability(&app, "fact_verification", "v1").await;

    let resp = app.get("/v1/discovery/search?capability=fact_verification&status=REGISTERED").await;
    let status = TestApp::status(&resp);
    assert_eq!(status, StatusCode::OK);

    let json: serde_json::Value = TestApp::json_body(resp).await;
    assert_eq!(json["protocol"].as_str().unwrap(), "acp/1");

    let results = json["results"].as_array().expect("results should be an array");
    assert!(!results.is_empty(), "expected at least one result");

    // The registered agent should appear in the results.
    let found = results.iter().any(|r| r["agent_id"].as_str() == Some(&agent_id));
    assert!(found, "registered agent should be in search results");
}

/// GET /v1/discovery/search — search with nonexistent capability returns empty OK.
#[tokio::test]
async fn discovery_search_no_results() {
    let app = TestApp::setup().await;

    let resp = app.get("/v1/discovery/search?capability=nonexistent_capability_xyz").await;
    let status = TestApp::status(&resp);
    assert_eq!(status, StatusCode::OK);

    let json: serde_json::Value = TestApp::json_body(resp).await;
    assert_eq!(json["protocol"].as_str().unwrap(), "acp/1");
    assert_eq!(json["count"].as_i64().unwrap(), 0);

    let results = json["results"].as_array().expect("results should be an array");
    assert!(results.is_empty(), "expected no results for unknown capability");
}
