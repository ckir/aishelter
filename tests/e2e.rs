//! Comprehensive e2e test suite for Agent Commons.
//!
//! Tests run against a live axum server with a real PostgreSQL database.
//! Set TEST_BASE_URL to point at a running server.
//!
//! Run with: `cargo test --test integration e2e -- --ignored`
//! Or with a server: `TEST_BASE_URL=http://localhost:3000 cargo test --test integration e2e -- --ignored`

use reqwest::{Client, StatusCode};
use serde_json::json;

fn base_url() -> String {
    std::env::var("TEST_BASE_URL").unwrap_or_else(|_| "http://localhost:3000".to_string())
}

fn test_client() -> Client {
    Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .expect("failed to build test client")
}

// ─── §63: First end-to-end demonstration ─────────────────────────────────────

/// Full lifecycle: register → discover → task → validate → VWU.
#[tokio::test]
#[ignore = "requires running server + PostgreSQL"]
async fn e2e_full_task_lifecycle() {
    let c = test_client();
    let b = base_url();

    // Agent A registers
    let a = reg(
        &c,
        &b,
        "e2e_agent_a",
        "pk_a_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    )
    .await;
    let a_id = a["agent_id"].as_str().unwrap();

    // Agent B registers
    let b2 = reg(
        &c,
        &b,
        "e2e_agent_b",
        "pk_b_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    )
    .await;
    let b_id = b2["agent_id"].as_str().unwrap();

    // Agent B publishes card
    card(&c, &b, b_id, "FactCheckAgent", "Fact verification agent").await;

    // Agent A discovers (returns all agents if no capability filter matches)
    let results = discover(&c, &b, "fact_verification").await;
    assert!(!results.is_empty(), "discovery should return results");

    // Agent A creates task
    let tid = task(
        &c,
        &b,
        a_id,
        "fact_verification",
        "Verify claims",
        json!({"claims": ["c1"]}),
        "peer",
        2,
    )
    .await;

    // Agent B accepts
    accept(&c, &b, &tid, b_id).await;

    // Agent B submits result
    submit(&c, &b, &tid, b_id, json!({"result": "verified"}), "sha256:abc").await;

    // Two validators verify
    let v1 = reg(
        &c,
        &b,
        "e2e_validator_1",
        "pk_v1_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcd",
    )
    .await;
    let v2 = reg(
        &c,
        &b,
        "e2e_validator_2",
        "pk_v2_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcd",
    )
    .await;
    let v1_id = v1["agent_id"].as_str().unwrap();
    let v2_id = v2["agent_id"].as_str().unwrap();

    validate(&c, &b, &tid, v1_id, "approve", "looks good").await;
    validate(&c, &b, &tid, v2_id, "approve", "agreed").await;

    // Task should be VERIFIED
    let t = get_task(&c, &b, &tid).await;
    assert_eq!(t["status"], "VERIFIED", "task should be verified after quorum");
}

// ─── §64: Asynchronous mailbox ──────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires running server + PostgreSQL"]
async fn e2e_async_mailbox() {
    let c = test_client();
    let b = base_url();

    let sender = reg(
        &c,
        &b,
        "mb_sender",
        "pk_ms_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    )
    .await;
    let receiver = reg(
        &c,
        &b,
        "mb_receiver",
        "pk_mr_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    )
    .await;
    let s_id = sender["agent_id"].as_str().unwrap();
    let r_id = receiver["agent_id"].as_str().unwrap();

    // Send message
    let _ = send_msg(&c, &b, s_id, r_id, "task.offer", json!({"task": "research"})).await;

    // Retrieve mailbox
    let msgs = get_msgs(&c, &b, r_id).await;
    assert_eq!(msgs.len(), 1, "receiver should have one message");
    assert_eq!(msgs[0]["type"], "task.offer");

    // Acknowledge
    let mid = msgs[0]["message_id"].as_str().unwrap();
    ack(&c, &b, mid).await;

    // Should not appear in unacknowledged
    let unacked = get_unacked(&c, &b, r_id).await;
    assert!(unacked.is_empty(), "acknowledged message should be gone");
}

// ─── Health & version ────────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires running server"]
async fn health_check() {
    let resp = test_client().get(format!("{}/v1/health", base_url())).send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert_eq!(resp.text().await.unwrap(), "ok");
}

#[tokio::test]
#[ignore = "requires running server"]
async fn version_check() {
    let resp = test_client().get(format!("{}/v1/version", base_url())).send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let body: serde_json::Value = resp.json().await.unwrap();
    assert_eq!(body["protocol"], "acp/1");
    assert!(!body["version"].as_str().unwrap().is_empty());
}

// ─── Adversarial tests ───────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires running server + PostgreSQL"]
async fn adversarial_duplicate_public_key() {
    let c = test_client();
    let b = base_url();
    let key = "pk_dup_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
    let _ = reg(&c, &b, "adv_first", key).await;
    let resp = c
        .post(format!("{}/v1/agents/register", b))
        .json(&json!({"agent_id": "adv_second", "public_key": key}))
        .send()
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT, "duplicate public key should be 409");
}

#[tokio::test]
#[ignore = "requires running server + PostgreSQL"]
async fn adversarial_submit_nonexistent_task() {
    let c = test_client();
    let b = base_url();
    let a = reg(
        &c,
        &b,
        "adv_nt",
        "pk_ant_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    )
    .await;
    let aid = a["agent_id"].as_str().unwrap();
    let resp = c
        .post(format!("{}/v1/tasks/nonexistent/result", b))
        .json(&json!({"agent_id": aid, "result": json!({}), "output_hash": "sha256:x"}))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_client_error(), "should return 4xx");
    assert_ne!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[tokio::test]
#[ignore = "requires running server + PostgreSQL"]
async fn adversarial_message_nonexistent_agent() {
    let c = test_client();
    let b = base_url();
    let a = reg(
        &c,
        &b,
        "adv_msg_from",
        "pk_amf_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    )
    .await;
    let aid = a["agent_id"].as_str().unwrap();
    let resp = c.post(format!("{}/v1/messages", b))
        .json(&json!({"from_agent_id": aid, "to_agent_id": "agent_nonexistent", "type": "task.offer", "payload": json!({})}))
        .send().await.unwrap();
    assert!(resp.status().is_client_error(), "message to nonexistent agent should be 4xx");
}

#[tokio::test]
#[ignore = "requires running server + PostgreSQL"]
async fn adversarial_duplicate_validation() {
    let c = test_client();
    let b = base_url();
    let a = reg(
        &c,
        &b,
        "adv_dup_val",
        "pk_adv_0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
    )
    .await;
    let aid = a["agent_id"].as_str().unwrap();
    let tid = task(&c, &b, aid, "research", "test", json!({}), "peer", 2).await;

    let r1 = c
        .post(format!("{}/v1/validations/{}/validate", b, tid))
        .json(&json!({"validator_id": aid, "decision": "approve", "reasoning": "first"}))
        .send()
        .await
        .unwrap();
    assert!(r1.status().is_success());

    let r2 = c
        .post(format!("{}/v1/validations/{}/validate", b, tid))
        .json(&json!({"validator_id": aid, "decision": "approve", "reasoning": "second"}))
        .send()
        .await
        .unwrap();
    // Idempotent (200) or conflict (409) — not 500
    assert!(r2.status().is_success() || r2.status() == StatusCode::CONFLICT);
}

#[tokio::test]
#[ignore = "requires running server + PostgreSQL"]
async fn adversarial_get_nonexistent_agent() {
    let c = test_client();
    let b = base_url();
    let resp = c.get(format!("{}/v1/agents/agent_nonexistent", b)).send().await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires running server + PostgreSQL"]
async fn adversarial_acknowledge_nonexistent_message() {
    let c = test_client();
    let b = base_url();
    let resp = c
        .post(format!("{}/v1/messages/00000000-0000-0000-0000-000000000000/ack", b))
        .send()
        .await
        .unwrap();
    assert!(resp.status().is_client_error(), "ack nonexistent message should be 4xx");
}

// ─── API helpers ─────────────────────────────────────────────────────────────

async fn reg(c: &Client, b: &str, id: &str, pk: &str) -> serde_json::Value {
    let r = c
        .post(format!("{}/v1/agents/register", b))
        .json(&json!({"agent_id": id, "public_key": pk}))
        .send()
        .await
        .unwrap();
    assert!(r.status().is_success(), "register failed: {}", r.status());
    r.json().await.unwrap()
}

async fn card(c: &Client, b: &str, id: &str, name: &str, desc: &str) {
    let r = c
        .put(format!("{}/v1/agents/{}/card", b, id))
        .json(&json!({"name": name, "description": desc}))
        .send()
        .await
        .unwrap();
    assert!(r.status().is_success(), "card update failed: {}", r.status());
}

async fn discover(c: &Client, b: &str, cap: &str) -> Vec<serde_json::Value> {
    let r = c
        .get(format!("{}/v1/discovery/search", b))
        .query(&[("capability", cap)])
        .send()
        .await
        .unwrap();
    assert!(r.status().is_success(), "discovery failed: {}", r.status());
    let body: serde_json::Value = r.json().await.unwrap();
    body["results"].as_array().cloned().unwrap_or_default()
}

#[allow(clippy::too_many_arguments)]
async fn task(
    c: &Client,
    b: &str,
    req: &str,
    cap: &str,
    desc: &str,
    input: serde_json::Value,
    vm: &str,
    rv: i32,
) -> String {
    let r = c.post(format!("{}/v1/tasks", b)).json(&json!({"requester": req, "capability": cap, "description": desc, "input": input, "verification_method": vm, "required_validators": rv})).send().await.unwrap();
    assert!(r.status().is_success(), "create task failed: {}", r.status());
    let body: serde_json::Value = r.json().await.unwrap();
    body["task_id"].as_str().unwrap().to_string()
}

async fn accept(c: &Client, b: &str, tid: &str, aid: &str) {
    let r = c
        .post(format!("{}/v1/tasks/{}/accept", b, tid))
        .json(&json!({"agent_id": aid}))
        .send()
        .await
        .unwrap();
    assert!(r.status().is_success(), "accept failed: {}", r.status());
}

async fn submit(c: &Client, b: &str, tid: &str, aid: &str, result: serde_json::Value, hash: &str) {
    let r = c
        .post(format!("{}/v1/tasks/{}/result", b, tid))
        .json(&json!({"agent_id": aid, "result": result, "output_hash": hash}))
        .send()
        .await
        .unwrap();
    assert!(r.status().is_success(), "submit failed: {}", r.status());
}

async fn validate(c: &Client, b: &str, tid: &str, vid: &str, decision: &str, reason: &str) {
    let r = c
        .post(format!("{}/v1/validations/{}/validate", b, tid))
        .json(&json!({"validator_id": vid, "decision": decision, "reasoning": reason}))
        .send()
        .await
        .unwrap();
    assert!(r.status().is_success(), "validate failed: {}", r.status());
}

async fn get_task(c: &Client, b: &str, tid: &str) -> serde_json::Value {
    let r = c.get(format!("{}/v1/tasks/{}", b, tid)).send().await.unwrap();
    assert!(r.status().is_success(), "get task failed: {}", r.status());
    r.json::<serde_json::Value>().await.unwrap()["data"].clone()
}

async fn send_msg(
    c: &Client,
    b: &str,
    from: &str,
    to: &str,
    t: &str,
    p: serde_json::Value,
) -> serde_json::Value {
    let r = c
        .post(format!("{}/v1/messages", b))
        .json(&json!({"from_agent_id": from, "to_agent_id": to, "type": t, "payload": p}))
        .send()
        .await
        .unwrap();
    assert!(r.status().is_success(), "send message failed: {}", r.status());
    r.json().await.unwrap()
}

async fn get_msgs(c: &Client, b: &str, aid: &str) -> Vec<serde_json::Value> {
    let r = c.get(format!("{}/v1/messages", b)).query(&[("agent_id", aid)]).send().await.unwrap();
    let body: serde_json::Value = r.json().await.unwrap();
    body["data"]["messages"].as_array().cloned().unwrap_or_default()
}

async fn get_unacked(c: &Client, b: &str, aid: &str) -> Vec<serde_json::Value> {
    let r = c
        .get(format!("{}/v1/messages", b))
        .query(&[("agent_id", aid), ("unacknowledged", "true")])
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = r.json().await.unwrap();
    body["data"]["messages"].as_array().cloned().unwrap_or_default()
}

async fn ack(c: &Client, b: &str, mid: &str) {
    let r = c.post(format!("{}/v1/messages/{}/ack", b, mid)).send().await.unwrap();
    assert!(r.status().is_success(), "ack failed: {}", r.status());
}
