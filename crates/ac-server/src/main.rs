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

use ac_server::config::Settings;
use ac_server::server::create_app;
use sqlx::postgres::PgPoolOptions;

/// Binary entry point for the Agent Commons server.
///
/// Performs the following startup sequence:
/// 1. Initialize the `tracing` subscriber for structured logging
/// 2. Load runtime [`Settings`] from environment variables
/// 3. Create a PostgreSQL connection pool
/// 4. Run database migrations
/// 5. Build the axum [`Router`] with all feature sub-routes
/// 6. Bind to the configured address and serve requests
///
/// The server shuts down gracefully on SIGINT (Ctrl+C).
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize structured logging
    tracing_subscriber::fmt::init();

    // Load configuration
    let settings = Settings::new().unwrap_or_else(|_| Settings::default());

    // Create database connection pool
    let pool = PgPoolOptions::new().max_connections(10).connect(&settings.database_url).await?;

    // Run pending database migrations
    sqlx::migrate!("../../migrations").run(&pool).await?;

    // Build the application router
    let app = create_app(pool);

    tracing::info!("Agent Commons starting on {}:{}", settings.host, settings.port);

    // Start the HTTP server
    let listener =
        tokio::net::TcpListener::bind(format!("{}:{}", settings.host, settings.port)).await?;
    axum::serve(listener, app).with_graceful_shutdown(shutdown_signal()).await?;

    Ok(())
}

/// Wait for SIGINT (Ctrl+C) and return a future that resolves when received.
async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    ctrl_c.await.expect("failed to install Ctrl+C handler");
    tracing::info!("shutdown signal received, draining connections...");
}
