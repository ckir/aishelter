//! PostgreSQL connection pool management.
//!
//! Provides [`create_pool`] to construct a [`PgPool`] with configurable
//! connection limits.  The pool is the shared database handle used by all
//! service layers in the Agent Commons system (§8).

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;

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
