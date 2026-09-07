//! Cloud Run serverless binary.
//!
//! Startup sequence:
//! 1. Load Settings from AC_ env vars
//! 2. Create PgPool via PgDbAdapter
//! 3. Run migrations
//! 4. Create Metrics and RateLimiter
//! 5. Build Router via create_app
//! 6. Serve via CloudRunHttpAdapter

use ac_metrics::registry::Metrics;
use ac_runtime::adapters::cloudrun::CloudRunHttpAdapter;
use ac_runtime::{DbAdapter, HttpAdapter, PgDbAdapter};
use ac_server::config::Settings;
use ac_server::middleware::rate_limit::RateLimiter;
use ac_server::server::create_app;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize structured logging
    tracing_subscriber::fmt::init();

    // Load configuration
    let settings = Settings::new().unwrap_or_else(|_| Settings::default());

    // Create database connection pool
    let pool = PgDbAdapter::create_pool(&settings.database_url).await?;

    // Run pending database migrations
    // Path is relative to the workspace root (the expected CWD for deployments).
    // For container deployments, set CWD to the workspace root.
    sqlx::migrate!("../../migrations").run(&pool).await?;

    // Create metrics registry
    let metrics = Metrics::new();

    // Create rate limiter
    let rate_limiter = RateLimiter::new(
        settings.rate_limit_global,
        settings.rate_limit_per_agent,
        settings.rate_limit_window_secs,
    );

    // Build the application router
    let shared_pool = ac_db::pool::SharedPool::new(pool);
    let app = create_app(shared_pool, metrics, rate_limiter);

    // Start serving via Cloud Run adapter
    tracing::info!("Cloud Run adapter starting on {}:{}", settings.host, settings.port);
    let adapter = CloudRunHttpAdapter::new(settings.host, Some(settings.port));

    adapter.serve(app).await?;
    Ok(())
}
