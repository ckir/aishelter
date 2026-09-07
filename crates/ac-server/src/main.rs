//! Agent Commons HTTP server binary.
//!
//! This is the `main` entry point for the `agent-commons` executable.
//! It initializes tracing, loads runtime configuration, creates the
//! database connection pool, and starts the axum HTTP server.
//!
//! # Environment Variables
//!
//! | Variable | Description | Default |
//! |---|---|---|
//! | `AC_DATABASE_URL` | PostgreSQL connection string | `postgresql://localhost/aishelter` |
//! | `AC_HOST` | HTTP bind address | `0.0.0.0` |
//! | `AC_PORT` | HTTP listen port | `3000` |
//! | `AC_RDS_IAM_AUTH` | Enable RDS IAM auth token refresh | `false` |

use ac_db::pool::SharedPool;
use ac_metrics::registry::Metrics;
use ac_server::config::Settings;
use ac_server::middleware::rate_limit::RateLimiter;
use ac_server::server::create_app;
use sqlx::postgres::PgPoolOptions;

/// Binary entry point for the Agent Commons server.
///
/// Performs the following startup sequence:
/// 1. Initialize the `tracing` subscriber for structured logging
/// 2. Load runtime [`Settings`] from environment variables
/// 3. Create a PostgreSQL connection pool (with optional RDS IAM auth)
/// 4. Run database migrations
/// 5. Build the axum [`Router`] with all feature sub-routes
/// 6. Bind to the configured address and serve requests
///
/// When `AC_RDS_IAM_AUTH=true` and the `rds-iam` feature is enabled,
/// the server generates an IAM auth token on startup and spawns a
/// background task to refresh the token every ~12 minutes.
///
/// The server shuts down gracefully on SIGINT (Ctrl+C).
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize structured logging
    tracing_subscriber::fmt::init();

    // Load configuration
    let settings = Settings::new().unwrap_or_else(|_| Settings::default());

    // Create database connection pool
    let shared_pool = create_pool(&settings).await?;

    // Run pending database migrations
    let pool = shared_pool.load();
    sqlx::migrate!("../../migrations").run(&pool).await?;
    drop(pool);

    // Create metrics registry
    let metrics = Metrics::new();

    // Create rate limiter
    let rate_limiter = RateLimiter::new(
        settings.rate_limit_global,
        settings.rate_limit_per_agent,
        settings.rate_limit_window_secs,
    );

    // Build the application router
    let app = create_app(shared_pool, metrics, rate_limiter);

    tracing::info!("Agent Commons starting on {}:{}", settings.host, settings.port);

    // Start the HTTP server
    let listener =
        tokio::net::TcpListener::bind(format!("{}:{}", settings.host, settings.port)).await?;
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;

    Ok(())
}

/// Create the shared pool, optionally with RDS IAM auth and background
/// token refresh.
async fn create_pool(settings: &Settings) -> anyhow::Result<SharedPool> {
    #[cfg(feature = "rds-iam")]
    if settings.rds_iam_auth {
        use ac_server::rds_iam;

        let (pool, params) = rds_iam::create_iam_pool(&settings.database_url).await?;
        let shared = SharedPool::new(pool);

        // Spawn background token refresh loop
        rds_iam::start_refresh_loop(shared.clone(), params, None);

        tracing::info!("RDS IAM auth enabled — token refresh loop started");
        return Ok(shared);
    }

    // Standard mode: static credentials, no refresh needed
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&settings.database_url)
        .await?;
    Ok(SharedPool::new(pool))
}

/// Wait for SIGINT (Ctrl+C) and return a future that resolves when received.
async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    ctrl_c.await.expect("failed to install Ctrl+C handler");
    tracing::info!("shutdown signal received, draining connections...");
}
