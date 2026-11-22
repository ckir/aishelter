//! Expired replay-protection and idempotency rows are purged; live ones stay.

use ac_db::pool::SharedPool;

use crate::harness::TestApp;

async fn count(app: &TestApp, sql: &str, bind: &str) -> i64 {
    sqlx::query_scalar(sql).bind(bind).fetch_one(app.pool()).await.expect("count query")
}

#[tokio::test]
async fn purge_removes_only_expired_rows() {
    let app = TestApp::setup().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();

    for (nonce, offset) in [("old", "-1 hour"), ("live", "+1 hour")] {
        sqlx::query(
            "INSERT INTO nonce_replay_cache (agent_id, nonce, expires_at) \
             VALUES ($1, $2, now() + $3::interval)",
        )
        .bind(&tag)
        .bind(nonce)
        .bind(offset)
        .execute(app.pool())
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO idempotency_keys (principal_id, route, idempotency_key, request_hash, expires_at) \
             VALUES ($1, 'POST /x', $2, 'h', now() + $3::interval)",
        )
        .bind(&tag)
        .bind(nonce)
        .bind(offset)
        .execute(app.pool())
        .await
        .unwrap();
    }

    let purged = ac_server::maintenance::purge_expired(&SharedPool::new(app.pool().clone()))
        .await
        .expect("purge succeeds");
    assert!(purged.nonces >= 1 && purged.idempotency_keys >= 1, "{purged:?}");

    let nonces =
        count(&app, "SELECT count(*) FROM nonce_replay_cache WHERE agent_id = $1", &tag).await;
    let keys =
        count(&app, "SELECT count(*) FROM idempotency_keys WHERE principal_id = $1", &tag).await;
    assert_eq!((nonces, keys), (1, 1), "only the live row of each table remains");
    let live: String =
        sqlx::query_scalar("SELECT nonce FROM nonce_replay_cache WHERE agent_id = $1")
            .bind(&tag)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(live, "live");
    let live_key: String =
        sqlx::query_scalar("SELECT idempotency_key FROM idempotency_keys WHERE principal_id = $1")
            .bind(&tag)
            .fetch_one(app.pool())
            .await
            .unwrap();
    assert_eq!(live_key, "live");
}
