//! Google Cloud Run HTTP adapter.
//!
//! Runs a minimal Tokio runtime and binds to the `$PORT` env var
//! provided by Cloud Run at deploy time.

use axum::Router;
use tokio::net::TcpListener;

use crate::{HttpAdapter, Result};

/// HTTP adapter for Google Cloud Run.
///
/// Binds to the configured port (from `$PORT` environment variable
/// or a caller-supplied value, defaulting to 3000) and serves the
/// axum `Router` via `axum::serve`.
pub struct CloudRunHttpAdapter {
    host: String,
    port: u16,
}

impl CloudRunHttpAdapter {
    /// Create a new Cloud Run adapter.
    ///
    /// If `port` is `None`, reads from the `$PORT` environment variable
    /// (Cloud Run convention), defaulting to 3000.
    pub fn new(host: String, port: Option<u16>) -> Self {
        let port = port.unwrap_or_else(|| {
            std::env::var("PORT")
                .ok()
                .and_then(|p| p.parse().ok())
                .unwrap_or(3000)
        });
        Self { host, port }
    }
}

impl HttpAdapter for CloudRunHttpAdapter {
    async fn serve(self, router: Router) -> Result<()> {
        let addr = format!("{}:{}", self.host, self.port);
        let listener = TcpListener::bind(&addr).await?;
        tracing::info!("Cloud Run adapter listening on {}", addr);
        axum::serve(listener, router)
            .with_graceful_shutdown(shutdown_signal())
            .await?;
        Ok(())
    }
}

/// Wait for SIGTERM (Cloud Run shutdown) or SIGINT (local dev) and return
/// a future that resolves when either is received.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};
        let mut sigterm = signal(SignalKind::Term).expect("failed to install SIGTERM handler");
        let ctrl_c = tokio::signal::ctrl_c();
        tokio::select! {
            _ = sigterm.recv() => {
                tracing::info!("SIGTERM received, draining connections...");
            }
            _ = ctrl_c => {
                tracing::info!("SIGINT received, draining connections...");
            }
        }
    }
    #[cfg(not(unix))]
    {
        let ctrl_c = tokio::signal::ctrl_c();
        ctrl_c.await.expect("failed to install Ctrl+C handler");
        tracing::info!("shutdown signal received, draining connections...");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_port_is_used() {
        let adapter = CloudRunHttpAdapter::new("0.0.0.0".to_string(), Some(8080));
        assert_eq!(adapter.port, 8080);
        assert_eq!(adapter.host, "0.0.0.0");
    }

    #[test]
    fn default_port_when_no_env_var() {
        // Ensure PORT is not set
        // Safety: single-threaded test context, no concurrent env access
        unsafe { std::env::remove_var("PORT") };
        let adapter = CloudRunHttpAdapter::new("127.0.0.1".to_string(), None);
        assert_eq!(adapter.port, 3000);
    }

    #[test]
    fn port_env_var_is_read() {
        // Safety: single-threaded test context, no concurrent env access
        unsafe { std::env::set_var("PORT", "4567") };
        let adapter = CloudRunHttpAdapter::new("0.0.0.0".to_string(), None);
        assert_eq!(adapter.port, 4567);
        unsafe { std::env::remove_var("PORT") };
    }
}
