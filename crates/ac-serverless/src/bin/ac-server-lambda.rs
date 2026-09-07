//! AWS Lambda serverless binary.
//!
//! Startup sequence:
//! 1. Load Settings from AC_ env vars
//! 2. Create PgPool via PgDbAdapter (cold start)
//! 3. Run migrations (cold start only)
//! 4. Create Metrics and RateLimiter
//! 5. Build Router via create_app
//! 6. Serve via LambdaHttpAdapter (lambda_http event loop)

use ac_metrics::registry::Metrics;
use ac_runtime::adapters::lambda::LambdaHttpAdapter;
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

    // Create database connection pool (cold start)
    #[cfg(feature = "rds-iam")]
    let pool = if settings.rds_iam_auth {
        let (pool, _) = ac_server::rds_iam::create_iam_pool(&settings.database_url).await?;
        pool
    } else {
        PgDbAdapter::create_pool(&settings.database_url).await?
    };

    #[cfg(not(feature = "rds-iam"))]
    let pool = PgDbAdapter::create_pool(&settings.database_url).await?;

    // Run pending database migrations (cold start only)
    // Path is relative to the workspace root (the expected CWD for deployments).
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

    // Start serving via Lambda adapter
    let adapter = LambdaHttpAdapter::new();
    tracing::info!("Lambda adapter starting");

    adapter.serve(app).await?;
    Ok(())
}
