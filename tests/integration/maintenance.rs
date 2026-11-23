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

async fn insert_expired_nonce(app: &TestApp, tag: &str) {
    sqlx::query(
        "INSERT INTO nonce_replay_cache (agent_id, nonce, expires_at) \
         VALUES ($1, 'old', now() - interval '1 hour')",
    )
    .bind(tag)
    .execute(app.pool())
    .await
    .unwrap();
}

async fn wait_until_purged(app: &TestApp, tag: &str) -> bool {
    for _ in 0..100 {
        let left: i64 =
            count(app, "SELECT count(*) FROM nonce_replay_cache WHERE agent_id = $1", tag).await;
        if left == 0 {
            return true;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    false
}

/// The background task purges on its own schedule, without being called.
#[tokio::test]
async fn the_purge_task_removes_expired_rows_by_itself() {
    let app = TestApp::setup().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    insert_expired_nonce(&app, &tag).await;

    let handle = ac_server::maintenance::spawn_purge_task(
        SharedPool::new(app.pool().clone()),
        std::time::Duration::from_millis(50),
    );
    let purged = wait_until_purged(&app, &tag).await;
    handle.abort();
    assert!(purged, "expired nonce was not purged by the background task");
}

/// A database outage must not kill the loop: once the database is back, the
/// next tick purges again.
#[tokio::test]
async fn the_purge_task_survives_a_database_outage() {
    let app = TestApp::setup().await;
    let tag = uuid::Uuid::new_v4().simple().to_string();
    insert_expired_nonce(&app, &tag).await;

    // A pool whose every query fails (nothing listens on port 1).
    let dead = sqlx::postgres::PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_millis(200))
        .connect_lazy("postgres://nobody:nothing@127.0.0.1:1/none")
        .unwrap();
    let shared = SharedPool::new(dead);
    let handle = ac_server::maintenance::spawn_purge_task(
        shared.clone(),
        std::time::Duration::from_millis(50),
    );

    // Several failing ticks: the row is still there and the task is still alive.
    tokio::time::sleep(std::time::Duration::from_millis(600)).await;
    assert!(!handle.is_finished(), "the task exited after a failed purge");
    let left: i64 =
        count(&app, "SELECT count(*) FROM nonce_replay_cache WHERE agent_id = $1", &tag).await;
    assert_eq!(left, 1);

    shared.swap(app.pool().clone());
    let purged = wait_until_purged(&app, &tag).await;
    handle.abort();
    assert!(purged, "the task did not recover after the database came back");
}
