//! Runtime adapter traits for serverless deployments.
//!
//! This crate defines the `HttpAdapter` and `DbAdapter` traits that
//! decouple the ac-server application from platform-specific HTTP
//! handling and database connection logic.
//!
//! Each serverless platform (AWS Lambda, Cloudflare Workers, Google
//! Cloud Run) has its own event loop and lifecycle. These traits let
//! ac-server hand off a pre-built `axum::Router` to whichever adapter
//! the deployment target requires, without conditional compilation.

use axum::Router;
use sqlx::PgPool;

/// Result type alias for adapter operations.
///
/// Used by all `HttpAdapter` and `DbAdapter` methods to surface errors
/// uniformly regardless of platform. Backed by `anyhow::Error` so
/// implementations can use `?` with any error type.
pub type Result<T> = std::result::Result<T, anyhow::Error>;

/// Abstracts platform-specific HTTP request/response handling.
///
/// Each implementation wraps the axum `Router` in a platform-specific
/// event loop — for Lambda this invokes `lambda_http::run`, for Workers
/// it calls into `worker::WorkerService`, and for Cloud Run it binds
/// to a TCP socket.
///
/// This trait is NOT dyn-object-safe — the `impl Future` return type prevents
/// `Box<dyn HttpAdapter>`. Use generics (`fn serve_with<T: HttpAdapter>(...)`)
/// to accept adapters polymorphically.
pub trait HttpAdapter: Send + Sync + 'static {
    /// Start serving requests through the given router.
    ///
    /// Blocks until the platform signals shutdown (SIGTERM, Lambda
    /// runtime exit, etc.) or an unrecoverable error occurs. Returns
    /// `Ok(())` on clean shutdown, `Err` otherwise.
    fn serve(
        self,
        router: Router,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
}

/// Abstracts platform-specific database connection creation.
///
/// For Lambda and Cloud Run the implementation returns a
/// `sqlx::PgPool` backed by a direct TCP connection. For Workers the
/// implementation may return an HTTP gateway client that proxies SQL
/// over HTTPS to a pooled edge service.
///
/// The associated `Pool` type lets each platform choose the most
/// appropriate connection representation while sharing the same trait
/// interface.
pub trait DbAdapter: Send + Sync + 'static {
    /// The connection pool type this adapter produces.
    ///
    /// Must be cloneable and thread-safe so that request handlers can
    /// share a single pool across concurrent invocations.
    type Pool: Clone + Send + Sync + 'static;

    /// Create a new connection pool from the given connection string.
    ///
    /// The connection string is expected to be a PostgreSQL URI of the
    /// form `postgresql://user:pass@host:port/dbname`. Implementations
    /// choose their own pooling strategy (max connections, timeouts,
    /// TLS) based on platform constraints.
    fn create_pool(
        connection_string: &str,
    ) -> impl std::future::Future<Output = Result<Self::Pool>> + Send;
}

/// A `DbAdapter` that returns a standard `sqlx::PgPool`.
///
/// Used by Lambda and Cloud Run deployments where a direct TCP
/// connection to PostgreSQL is available. Configures a conservative
/// `max_connections` of 10 to stay within serverless connection limits.
pub struct PgDbAdapter;

impl DbAdapter for PgDbAdapter {
    type Pool = PgPool;

    /// Create a `PgPool` from the connection string.
    ///
    /// Configures a maximum of 10 concurrent connections, suitable for
    /// most serverless workloads. Override per-deployment by providing
    /// a custom `DbAdapter` implementation.
    async fn create_pool(connection_string: &str) -> Result<Self::Pool> {
        // Use a conservative max_connections for serverless environments
        // where connection limits are typically low.
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(10)
            .connect(connection_string)
            .await?;
        Ok(pool)
    }
}

// Platform-specific adapter modules, compiled only when their feature
// flags are enabled. This keeps the dependency tree lean — a Cloud Run
// deployment doesn't pull in lambda_http, for example.

/// Adapter implementations for specific deployment platforms.
///
/// Each sub-module provides one or more `HttpAdapter` implementations
/// gated behind a feature flag. Enable only the adapter(s) needed for
/// the target deployment.
#[cfg(any(feature = "lambda", feature = "cloudrun", feature = "workers"))]
pub mod adapters;

/// Re-export the AWS Lambda HTTP adapter when the `lambda` feature is enabled.
///
/// Allows users to write `use ac_runtime::LambdaHttpAdapter` without
/// navigating through the adapters module path.
#[cfg(feature = "lambda")]
pub use adapters::lambda::LambdaHttpAdapter;

/// Re-export the Google Cloud Run HTTP adapter when the `cloudrun` feature is enabled.
///
/// Allows users to write `use ac_runtime::CloudRunHttpAdapter` without
/// navigating through the adapters module path.
#[cfg(feature = "cloudrun")]
pub use adapters::cloudrun::CloudRunHttpAdapter;

/// Re-export the Cloudflare Workers HTTP adapter when the `workers` feature is enabled.
///
/// Allows users to write `use ac_runtime::WorkersHttpAdapter` without
/// navigating through the adapters module path.
#[cfg(feature = "workers")]
pub use adapters::workers::WorkersHttpAdapter;
