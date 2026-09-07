//! PostgreSQL connection pool management.
//!
//! Provides [`create_pool`] to construct a [`PgPool`] with configurable
//! connection limits, and [`SharedPool`] for runtime-swappable pools
//! (used by RDS IAM auth token refresh).
//!
//! The pool is the shared database handle used by all service layers in
//! the Agent Commons system (§8).

use arc_swap::ArcSwap;
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;

/// A runtime-swappable database connection pool.
///
/// Wraps [`PgPool`] in an atomic reference ([`ArcSwap`]) that can be
/// replaced at runtime without interrupting in-flight requests. Designed
/// for RDS IAM authentication where credentials expire every 15 minutes
/// and the pool must be refreshed.
///
/// # Usage
///
/// ```ignore
/// let shared = SharedPool::new(pool);
///
/// // In request handlers — load a snapshot of the current pool:
/// let pool = shared.load();
/// let row = sqlx::query("SELECT 1").fetch_one(&pool).await?;
///
/// // In background refresh task — atomically swap the pool:
/// let new_pool = PgPoolOptions::new().connect(&new_url).await?;
/// shared.swap(new_pool);
/// ```
///
/// Existing handlers holding a loaded `PgPool` snapshot continue to work
/// even after a swap — the old pool is dropped only when all references
/// are released.
#[derive(Clone)]
pub struct SharedPool {
    inner: Arc<ArcSwap<PgPool>>,
}

impl SharedPool {
    /// Create a new `SharedPool` from an existing [`PgPool`].
    pub fn new(pool: PgPool) -> Self {
        Self {
            inner: Arc::new(ArcSwap::from_pointee(pool)),
        }
    }

    /// Load a snapshot of the current pool.
    ///
    /// Returns a cloned `PgPool` handle suitable for the lifetime of a
    /// single request. `PgPool` is internally `Arc`-backed, so this
    /// clone is cheap (pointer bump, not a deep copy).
    pub fn load(&self) -> PgPool {
        let guard = self.inner.load();
        PgPool::clone(&*guard)
    }

    /// Atomically replace the underlying pool.
    ///
    /// The previous pool is **not** explicitly closed — existing
    /// connections continue to serve in-flight queries. The old pool
    /// will be dropped (and its idle connections closed) once all
    /// `PgPool` clones from prior [`load`](Self::load) calls are
    /// released.
    pub fn swap(&self, new_pool: PgPool) {
        self.inner.store(Arc::new(new_pool));
    }
}

/// Create a PostgreSQL connection pool.
///
/// Opens a pool of up to `max_connections` connections to the database
/// specified by `database_url`.  The pool lazily establishes connections
/// on first use — calling this function does not immediately open any
/// database connections.
///
/// # Arguments
///
/// * `database_url` — PostgreSQL connection string
///   (e.g. `postgresql://user:pass@host:5432/dbname`).
/// * `max_connections` — Maximum number of concurrent connections in the pool.
///
/// # Errors
///
/// Returns an error if the connection string is invalid or the initial
/// pool configuration fails.
pub async fn create_pool(
    // PostgreSQL connection string.
    database_url: &str,
    // Maximum number of concurrent connections.
    max_connections: u32,
) -> anyhow::Result<PgPool> {
    // Configure the pool with the specified connection limit.
    let pool = PgPoolOptions::new().max_connections(max_connections).connect(database_url).await?;
    Ok(pool)
}

