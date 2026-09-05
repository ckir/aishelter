use sqlx::PgPool;

/// Run SQL migrations from the embedded migrations directory.
pub async fn run_migrations(pool: &PgPool) -> anyhow::Result<()> {
    tracing::info!("Running database migrations");
    sqlx::migrate!("../../migrations").run(pool).await?;
    Ok(())
}
