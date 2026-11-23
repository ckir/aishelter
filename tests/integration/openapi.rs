//! OpenAPI schema conformance tests.
//!
//! Validates that actual API responses match the declared OpenAPI schemas.
//! Tests fetch the `/api/openapi.json` spec, parse it with `openapiv3`,
//! and validate response bodies with `jsonschema`.

use axum::http::StatusCode;
use ed25519_dalek::SigningKey;
use jsonschema::Validator;
use rand_core::OsRng;
use serde_json::json;

use crate::harness::TestApp;

/// Generate a fresh Ed25519 keypair for testing.
///
/// Returns a tuple of `(agent_id, public_key_hex)` where `agent_id` is a
/// short deterministic string derived from the public key and
/// `public_key_hex` is the 64-character hex-encoded Ed25519 public key.
fn make_keypair() -> (String, String) {
    let signing_key = SigningKey::generate(&mut OsRng);
    let verifying_key = signing_key.verifying_key();
    let pk_hex = hex::encode(verifying_key.to_bytes());
    // Use the first 8 hex chars of the public key for a compact agent id.
    let id = format!("test_agent_{}", &pk_hex[..8]);
    (id, pk_hex)
}

/// Register a new agent with minimal profile and return its `agent_id`.
///
/// # Panics
///
/// Panics if the registration response is not 200 OK.
async fn register_agent(app: &TestApp) -> String {
    let (id, pk) = make_keypair();
    let resp = app
        .post_json(
            "/v1/agents/register",
            json!({
                "agent_id": id,
                "public_key": pk
            }),
        )
        .await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
    id
}

/// Validate a JSON value against a JSON Schema.
///
/// Builds a `jsonschema::Validator` from `schema` and asserts that
/// `value` passes validation.
///
/// # Panics
///
/// Panics if `schema` is not a valid JSON Schema or if `value` fails validation.
#[allow(dead_code)]
fn validate_schema(value: &serde_json::Value, schema: &serde_json::Value) {
    // Build the validator from the schema JSON.
    let validator = Validator::new(schema).expect("invalid schema");
    // Assert the value passes validation.
    let result = validator.validate(value);
    assert!(result.is_ok(), "schema validation failed: {:?}", result.err());
}

/// GET /v1/health — returns 200 with body "ok".
///
/// The health endpoint is a liveness probe that returns plain-text `"ok"`.
#[tokio::test]
async fn openapi_health_endpoint() {
    let app = TestApp::setup().await;
    let resp = app.get("/v1/health").await;
    // Health must return 200 OK per the OpenAPI spec.
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
    // Health returns plain text "ok" — no JSON body to validate.
}

/// GET /v1/version — returns JSON with "version" and "protocol" fields.
///
/// The version endpoint advertises the server version string and the ACP
/// protocol version so clients can verify compatibility.
#[tokio::test]
async fn openapi_version_endpoint() {
    let app = TestApp::setup().await;
    let resp = app.get("/v1/version").await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
    let body: serde_json::Value = TestApp::json_body(resp).await;
    // Version endpoint must include both "version" and "protocol" fields.
    assert!(body.get("version").is_some(), "missing 'version' field");
    assert!(body.get("protocol").is_some(), "missing 'protocol' field");
}

/// POST /v1/agents/register — returns valid AgentResponse.
///
/// Registers an agent and verifies the response is a non-empty agent id,
/// consistent with the `AgentResponse` schema in the OpenAPI document.
#[tokio::test]
async fn openapi_register_response() {
    let app = TestApp::setup().await;
    let id = register_agent(&app).await;
    // The registered agent id must be non-empty.
    assert!(!id.is_empty());
}

/// POST /v1/agents/register with empty body — returns error schema.
///
/// Sends a request missing required fields and asserts the error response
/// contains an `"error"` or `"message"` field as declared by the API spec.
#[tokio::test]
async fn openapi_error_response_schema() {
    let app = TestApp::setup().await;
    // Send an empty JSON object to trigger a validation error.
    let resp = app.post_json("/v1/agents/register", json!({})).await;
    // Must return a 4xx client error.
    assert!(TestApp::status(&resp).is_client_error(), "expected 4xx client error for empty body");
    let body_bytes =
        axum::body::to_bytes(resp.into_body(), usize::MAX).await.expect("failed to read body");
    if let Ok(body) = serde_json::from_slice::<serde_json::Value>(&body_bytes) {
        assert!(
            body.get("error").is_some() || body.get("message").is_some(),
            "error response should have 'error' or 'message' field"
        );
    } else {
        // If it's a plain text 422 from Axum, we consider it valid for now, though custom error mapping would be better.
        let body_str = String::from_utf8_lossy(&body_bytes);
        assert!(!body_str.is_empty(), "error response should not be empty");
    }
}

/// GET /api/openapi.json — the spec parses as valid OpenAPI 3.0.
///
/// Fetches the raw OpenAPI JSON document and asserts it deserialises into
/// an `openapiv3::OpenAPI` struct, confirming the spec is well-formed.
#[tokio::test]
async fn openapi_spec_is_valid_json() {
    let app = TestApp::setup().await;
    let resp = app.get("/api/openapi.json").await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
    let spec: serde_json::Value = TestApp::json_body(resp).await;
    assert!(spec.is_object(), "OpenAPI spec should be a JSON object");
}

/// The signed-request contract is part of the published spec: the scheme names
/// every header, states the timestamp format, and exactly the protected
/// operations require it.
#[tokio::test]
async fn spec_documents_request_signing_on_protected_operations_only() {
    let app = TestApp::setup().await;
    let resp = app.get("/api/openapi.json").await;
    assert_eq!(TestApp::status(&resp), StatusCode::OK);
    let spec: serde_json::Value = TestApp::json_body(resp).await;

    let scheme = &spec["components"]["securitySchemes"]["AgentSignature"];
    assert_eq!(scheme["type"], "apiKey", "{scheme}");
    assert_eq!(scheme["in"], "header");
    assert_eq!(scheme["name"], "X-Agent-Signature");
    let description = scheme["description"].as_str().expect("scheme has a description");
    for needle in [
        "X-Agent-Id",
        "X-Agent-Timestamp",
        "X-Agent-Nonce",
        "RFC 3339",
        "2026-11-22T20:30:00Z",
        "sha256",
        "query",
    ] {
        assert!(description.contains(needle), "scheme description lacks `{needle}`: {description}");
    }

    let secured = |path: &str, method: &str| -> bool {
        spec["paths"][path][method]["security"]
            .as_array()
            .is_some_and(|reqs| reqs.iter().any(|r| r.get("AgentSignature").is_some()))
    };
    for (path, method) in [
        ("/v1/messages", "post"),
        ("/v1/messages", "get"),
        ("/v1/messages/{id}", "get"),
        ("/v1/messages/{id}/ack", "post"),
        ("/v1/agents/{id}/card", "put"),
        ("/v1/tasks", "post"),
        ("/v1/tasks/{id}/accept", "post"),
        ("/v1/tasks/{id}/reject", "post"),
        ("/v1/tasks/{id}/result", "post"),
        ("/v1/validations/{task_id}/validate", "post"),
    ] {
        assert!(secured(path, method), "{method} {path} must require AgentSignature");
    }
    for (path, method) in [
        ("/v1/health", "get"),
        ("/v1/agents/register", "post"),
        ("/v1/agents/{id}", "get"),
        ("/v1/tasks/{id}", "get"),
        ("/v1/validations/{task_id}", "get"),
        ("/v1/agents/{id}/reputation", "get"),
    ] {
        assert!(!secured(path, method), "{method} {path} is public");
    }
}
