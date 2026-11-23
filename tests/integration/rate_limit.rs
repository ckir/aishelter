//! Rate limiting is keyed on the VERIFIED agent, never on a client header.
//!
//! The global bucket stays the outermost guard and covers everything. The
//! per-agent bucket sits behind authentication, so a caller can neither drain
//! another agent's allowance by naming it in `X-Agent-Id`, nor mint fresh
//! allowances by rotating fake ids.

use axum::body::Body;
use axum::http::{Request, StatusCode};

use crate::harness::{TestAgent, TestApp};

async fn signed_get(app: &TestApp, agent: &TestAgent) -> StatusCode {
    let path = format!("/v1/messages?agent_id={}", agent.id);
    TestApp::status(&app.send_signed(agent, "GET", &path, None).await)
}

async fn health_as(app: &TestApp, claimed_id: &str) -> StatusCode {
    let req = Request::builder()
        .uri("/v1/health")
        .header("X-Agent-Id", claimed_id)
        .body(Body::empty())
        .unwrap();
    app.request(req).await.status()
}

#[tokio::test]
async fn a_verified_agent_is_limited_to_its_own_allowance() {
    let app = TestApp::setup_with_limits(1000, 3, 60).await;
    let agent = app.register_agent().await;

    for i in 0..3 {
        assert_eq!(signed_get(&app, &agent).await, StatusCode::OK, "request {i}");
    }
    assert_eq!(signed_get(&app, &agent).await, StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn one_agents_exhaustion_does_not_affect_another() {
    let app = TestApp::setup_with_limits(1000, 2, 60).await;
    let busy = app.register_agent().await;
    let quiet = app.register_agent().await;

    for _ in 0..2 {
        assert_eq!(signed_get(&app, &busy).await, StatusCode::OK);
    }
    assert_eq!(signed_get(&app, &busy).await, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(signed_get(&app, &quiet).await, StatusCode::OK);
}

#[tokio::test]
async fn naming_a_victim_in_a_header_cannot_drain_their_allowance() {
    let app = TestApp::setup_with_limits(1000, 3, 60).await;
    let victim = app.register_agent().await;

    for _ in 0..10 {
        assert_eq!(health_as(&app, &victim.id).await, StatusCode::OK);
    }
    // The same header on a PROTECTED route, with no signature at all: refused
    // by authentication, and it must not touch the victim's bucket either.
    for _ in 0..10 {
        let req = Request::builder()
            .uri(format!("/v1/messages?agent_id={}", victim.id))
            .header("X-Agent-Id", &victim.id)
            .body(Body::empty())
            .unwrap();
        let status = app.request(req).await.status();
        assert!(
            status == StatusCode::BAD_REQUEST || status == StatusCode::UNAUTHORIZED,
            "unsigned request is refused by authentication, got {status}"
        );
    }
    for i in 0..3 {
        assert_eq!(signed_get(&app, &victim).await, StatusCode::OK, "victim request {i}");
    }
}

#[tokio::test]
async fn failed_signatures_do_not_consume_the_named_agents_allowance() {
    let app = TestApp::setup_with_limits(1000, 3, 60).await;
    let victim = app.register_agent().await;
    // Claims the victim's id but signs with a key the server does not hold.
    let forged = TestAgent::impostor_of(&victim.id);

    for _ in 0..10 {
        let path = format!("/v1/messages?agent_id={}", victim.id);
        let resp = app.send_signed(&forged, "GET", &path, None).await;
        assert_eq!(TestApp::status(&resp), StatusCode::UNAUTHORIZED);
    }
    for i in 0..3 {
        assert_eq!(signed_get(&app, &victim).await, StatusCode::OK, "victim request {i}");
    }
}

#[tokio::test]
async fn the_global_limit_still_applies_whatever_ids_are_claimed() {
    let app = TestApp::setup_with_limits(5, 100, 60).await;

    let mut limited = 0;
    for i in 0..10 {
        if health_as(&app, &format!("rotating-{i}")).await == StatusCode::TOO_MANY_REQUESTS {
            limited += 1;
        }
    }
    assert_eq!(limited, 5, "5 allowed by the global bucket, 5 rejected");
}

#[tokio::test]
async fn rate_limited_responses_say_when_to_retry() {
    // Global bucket: 1 request per 60s window.
    let app = TestApp::setup_with_limits(1, 100, 60).await;
    let first = app.get("/v1/health").await;
    assert_eq!(first.status(), StatusCode::OK);
    assert!(first.headers().get("retry-after").is_none(), "only 429s carry Retry-After");
    let second = app.get("/v1/health").await;
    assert_eq!(second.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(second.headers().get("retry-after").unwrap(), "60");

    // Per-agent bucket: same contract, with the window it was configured with.
    let app = TestApp::setup_with_limits(1000, 1, 45).await;
    let agent = app.register_agent().await;
    let path = format!("/v1/messages?agent_id={}", agent.id);
    assert_eq!(app.send_signed(&agent, "GET", &path, None).await.status(), StatusCode::OK);
    let limited = app.send_signed(&agent, "GET", &path, None).await;
    assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(limited.headers().get("retry-after").unwrap(), "45");
}
