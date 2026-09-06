//! SQL migration runner.
//!
//! Provides [`run_migrations`] to apply all pending SQL migrations from the
//! `migrations/` directory to the connected PostgreSQL database.  Uses
//! `sqlx::migrate!` which embeds the migration files at compile time and
//! tracks applied migrations in the `sqlx_migrations` table.

use sqlx::PgPool;

/// Run SQL migrations from the embedded migrations directory.
///
/// Applies any migrations from `migrations/` that have not yet been
/// recorded in the database's `sqlx_migrations` tracking table.  Migrations
/// are applied in filename order within a single transaction per migration
/// file (§8 — Initial schema in `001_initial_schema.sql`).
///
/// # Arguments
///
/// * `pool` — Active PostgreSQL connection pool.
///
/// # Errors
///
/// Returns an error if any migration SQL is invalid, if the migration
/// directory is missing, or if a migration fails mid-transaction.
pub async fn run_migrations(
    // Active PostgreSQL connection pool.
    pool: &PgPool,
) -> anyhow::Result<()> {
    // Log that migrations are starting for observability.
    tracing::info!("Running database migrations");
    // Run embedded migrations from the ../../migrations directory.
    // The path is relative to this source file at compile time.
    sqlx::migrate!("../../migrations").run(pool).await?;
    Ok(())
}
