//! Authentication for the directory routes that create or refresh entries.
//!
//! Listing and lookup stay public. Ownership of an entry is not modelled yet
//! (the tables have no owner column), so the gate is "any registered agent".

use axum::http::StatusCode;
use serde_json::{Value, json};

use crate::harness::TestApp;

fn service_body() -> Value {
    json!({
        "service_id": "svc-authz-1",
        "manifest_url": "https://example.test/manifest.json",
        "manifest_json": "{}"
    })
}

fn directory_body() -> Value {
    json!({
        "directory_id": "dir-authz-1",
        "manifest_url": "https://example.test/dir.json",
        "manifest_json": "{}"
    })
}

#[tokio::test]
async fn creating_a_service_requires_credentials() {
    let app = TestApp::setup().await;
    let resp = app.post_json("/v1/services", service_body()).await;
    assert_eq!(TestApp::status(&resp), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn refreshing_a_service_requires_credentials() {
    let app = TestApp::setup().await;
    let resp = app.put_json("/v1/services/anything/refresh", json!({})).await;
    assert_eq!(TestApp::status(&resp), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn creating_a_directory_requires_credentials() {
    let app = TestApp::setup().await;
    let resp = app.post_json("/v1/directories", directory_body()).await;
    assert_eq!(TestApp::status(&resp), StatusCode::UNAUTHORIZED);
}

// NOTE: these tests assert only what the AUTH layer decides (401 or not). The
// directory service currently queries `services` / `directories` tables that no
// migration creates, so past the gate its handlers fail with 500 regardless of
// authentication. Pinning that status here would enshrine the defect.

#[tokio::test]
async fn a_registered_agent_passes_the_gate_on_every_protected_route() {
    let app = TestApp::setup().await;
    let agent = app.register_agent().await;
    let cases = [
        ("POST", "/v1/services", Some(service_body())),
        ("PUT", "/v1/services/anything/refresh", Some(json!({}))),
        ("POST", "/v1/directories", Some(directory_body())),
    ];
    for (method, path, body) in cases {
        let resp = app.send_signed(&agent, method, path, body.as_ref()).await;
        let status = TestApp::status(&resp);
        assert_ne!(status, StatusCode::UNAUTHORIZED, "{method} {path}");
        assert_ne!(status, StatusCode::FORBIDDEN, "{method} {path}");
    }
}

#[tokio::test]
async fn listing_and_lookup_stay_public() {
    let app = TestApp::setup().await;
    for path in ["/v1/services", "/v1/directories", "/v1/capabilities", "/v1/services/x"] {
        let resp = app.get(path).await;
        assert_ne!(TestApp::status(&resp), StatusCode::UNAUTHORIZED, "{path}");
    }
}
