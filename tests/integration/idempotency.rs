//! Idempotency-Key semantics.
//!
//! A key is scoped to the verified principal and the route. Replaying the same
//! key with the same body returns the stored response; the same key with a
//! different body is a client error (422); only successful mutations are
//! stored; reads and oversized bodies are never cached.

use axum::body::Body;
use axum::http::{Request, StatusCode, header};
use serde_json::{Value, json};

use crate::harness::{TestAgent, TestApp};

const REPLAYED: &str = "x-idempotent-replayed";

fn message(from: &TestAgent, to: &str, n: u32) -> Value {
    json!({ "from_agent_id": from.id, "to_agent_id": to, "type": "test", "payload": {"n": n} })
}

async fn inbox_len(app: &TestApp, owner: &TestAgent) -> usize {
    let path = format!("/v1/messages?agent_id={}&unacknowledged=false", owner.id);
    let resp = app.send_signed(owner, "GET", &path, None).await;
    let json: Value = TestApp::json_body(resp).await;
    json["messages"].as_array().map_or(0, Vec::len)
}

#[tokio::test]
async fn signed_retry_with_same_key_is_replayed_not_reexecuted() {
    let app = TestApp::setup().await;
    let sender = app.register_agent().await;
    let receiver = app.register_agent().await;
    let body = message(&sender, &receiver.id, 1);
    let key = format!("k-{}", sender.id);

    let first = app.request(sender.request_with_key("POST", "/v1/messages", Some(&body), &key));
    let first = first.await;
    assert_eq!(first.status(), StatusCode::OK);
    assert!(first.headers().get(REPLAYED).is_none());
    let first_json: Value = TestApp::json_body(first).await;

    // A retry is re-signed (fresh nonce) but carries the same Idempotency-Key.
    let second = app.request(sender.request_with_key("POST", "/v1/messages", Some(&body), &key));
    let second = second.await;
    assert_eq!(second.status(), StatusCode::OK);
    assert_eq!(second.headers().get(REPLAYED).unwrap(), "true");
    assert_eq!(
        second.headers().get(header::CONTENT_TYPE).and_then(|v| v.to_str().ok()),
        Some("application/json"),
        "a replayed JSON response keeps its content type"
    );
    let second_json: Value = TestApp::json_body(second).await;
    assert_eq!(first_json["message_id"], second_json["message_id"]);

    assert_eq!(inbox_len(&app, &receiver).await, 1, "the message was stored once");
}

#[tokio::test]
async fn same_key_with_a_different_body_is_422() {
    let app = TestApp::setup().await;
    let sender = app.register_agent().await;
    let receiver = app.register_agent().await;
    let key = format!("k-{}", sender.id);

    let first = message(&sender, &receiver.id, 1);
    let resp = app.request(sender.request_with_key("POST", "/v1/messages", Some(&first), &key));
    assert_eq!(resp.await.status(), StatusCode::OK);

    let other = message(&sender, &receiver.id, 2);
    let resp = app.request(sender.request_with_key("POST", "/v1/messages", Some(&other), &key));
    assert_eq!(resp.await.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(inbox_len(&app, &receiver).await, 1);
}

#[tokio::test]
async fn keys_are_scoped_to_the_verified_agent() {
    let app = TestApp::setup().await;
    let a = app.register_agent().await;
    let b = app.register_agent().await;
    let target = app.register_agent().await;
    let key = format!("shared-{}", a.id);

    let resp = app.request(a.request_with_key(
        "POST",
        "/v1/messages",
        Some(&message(&a, &target.id, 1)),
        &key,
    ));
    assert_eq!(resp.await.status(), StatusCode::OK);

    // Another agent using the same key string must execute, not get A's response.
    let resp = app.request(b.request_with_key(
        "POST",
        "/v1/messages",
        Some(&message(&b, &target.id, 1)),
        &key,
    ));
    let resp = resp.await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.headers().get(REPLAYED).is_none());
    assert_eq!(inbox_len(&app, &target).await, 2);
}

#[tokio::test]
async fn a_spoofed_agent_id_header_does_not_change_the_principal() {
    let app = TestApp::setup().await;
    let key = format!("anon-{}", TestAgent::nonce());
    let register = |agent: &TestAgent, spoof: &str| {
        let body = json!({ "agent_id": agent.id, "public_key": agent.public_key_hex() });
        Request::builder()
            .method("POST")
            .uri("/v1/agents/register")
            .header(header::CONTENT_TYPE, "application/json")
            .header("Idempotency-Key", &key)
            .header("X-Agent-Id", spoof)
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap()
    };
    let first = TestAgent::new();
    let second = TestAgent::new();

    assert_eq!(app.request(register(&first, "spoof-1")).await.status(), StatusCode::OK);
    // Different body, different X-Agent-Id: still the same (anonymous) principal.
    assert_eq!(
        app.request(register(&second, "spoof-2")).await.status(),
        StatusCode::UNPROCESSABLE_ENTITY
    );
}

#[tokio::test]
async fn failed_requests_are_not_stored() {
    let app = TestApp::setup().await;
    let sender = app.register_agent().await;
    let late = TestAgent::new(); // not registered yet
    let key = format!("k-{}", sender.id);
    let body = message(&sender, &late.id, 1);

    let resp = app.request(sender.request_with_key("POST", "/v1/messages", Some(&body), &key));
    assert!(resp.await.status().is_client_error());

    // The recipient registers; retrying the same request must now execute.
    let reg = json!({ "agent_id": late.id, "public_key": late.public_key_hex() });
    assert_eq!(TestApp::status(&app.post_json("/v1/agents/register", reg).await), StatusCode::OK);

    let resp = app.request(sender.request_with_key("POST", "/v1/messages", Some(&body), &key));
    let resp = resp.await;
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.headers().get(REPLAYED).is_none());
}

#[tokio::test]
async fn reads_are_never_cached() {
    let app = TestApp::setup().await;
    let sender = app.register_agent().await;
    let receiver = app.register_agent().await;
    let key = format!("k-{}", receiver.id);
    let path = format!("/v1/messages?agent_id={}&unacknowledged=false", receiver.id);

    let resp = app.request(receiver.request_with_key("GET", &path, None, &key));
    let empty: Value = TestApp::json_body(resp.await).await;
    assert_eq!(empty["messages"].as_array().map(Vec::len), Some(0));

    let sent = app
        .send_signed(&sender, "POST", "/v1/messages", Some(&message(&sender, &receiver.id, 1)))
        .await;
    assert_eq!(sent.status(), StatusCode::OK);

    let resp = app.request(receiver.request_with_key("GET", &path, None, &key));
    let resp = resp.await;
    assert!(resp.headers().get(REPLAYED).is_none());
    let now: Value = TestApp::json_body(resp).await;
    assert_eq!(now["messages"].as_array().map(Vec::len), Some(1));
}

#[tokio::test]
async fn an_oversized_body_with_a_key_is_413() {
    let app = TestApp::setup().await;
    let big = vec![b'a'; 2 * 1024 * 1024];
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents/register")
        .header(header::CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", format!("big-{}", TestAgent::nonce()))
        .body(Body::from(big))
        .unwrap();
    assert_eq!(app.request(req).await.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

#[tokio::test]
async fn an_overlong_key_is_400() {
    let app = TestApp::setup().await;
    let req = Request::builder()
        .method("POST")
        .uri("/v1/agents/register")
        .header(header::CONTENT_TYPE, "application/json")
        .header("Idempotency-Key", "k".repeat(300))
        .body(Body::from("{}"))
        .unwrap();
    assert_eq!(app.request(req).await.status(), StatusCode::BAD_REQUEST);
}

/// Two nested routers can expose the same relative path (`POST /`). The route
/// scope must come from the full request URI, not the prefix-stripped one, or
/// a key used on one router would replay (or 422) on the other.
#[tokio::test]
async fn the_route_scope_is_the_full_path_not_the_nested_one() {
    use axum::{Router, middleware::from_fn_with_state, routing::post};
    use tower::ServiceExt;

    let app = TestApp::setup().await;
    let shared = ac_db::pool::SharedPool::new(app.pool().clone());
    let guarded = |label: &'static str| {
        Router::new().route("/", post(move || async move { label })).route_layer(
            from_fn_with_state(
                shared.clone(),
                ac_server::middleware::idempotency::enforce_idempotency,
            ),
        )
    };
    let router: Router = Router::new().nest("/one", guarded("one")).nest("/two", guarded("two"));

    let key = format!("scope-{}", uuid::Uuid::new_v4());
    for (path, expected_replay) in [("/one", false), ("/two", false), ("/one", true)] {
        let req = Request::builder()
            .method("POST")
            .uri(path)
            .header("Idempotency-Key", &key)
            .body(Body::empty())
            .unwrap();
        let resp = router.clone().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK, "{path}");
        assert_eq!(resp.headers().contains_key(REPLAYED), expected_replay, "{path}");
    }
}
