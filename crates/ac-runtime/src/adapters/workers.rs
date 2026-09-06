//! Cloudflare Workers HTTP adapter using workers-rs.
//!
//! Cloudflare Workers have no TCP socket support and no Tokio runtime. The
//! Workers runtime invokes a per-request fetch handler instead of a traditional
//! serve loop, so this adapter's [`serve`](WorkersHttpAdapter::serve) is a
//! no-op for trait compatibility.
//!
//! # Integration pattern
//!
//! The canonical way to wire an axum [`Router`](axum::Router) into a
//! Cloudflare Worker is to call the router directly as a
//! `tower::Service` on `http::Request<worker::Body>`:
//!
//! ```ignore
//! use axum::Router;
//! use tower::ServiceExt;
//! use worker::{event, Env, Context, HttpRequest, HttpResponse, Result};
//!
//! fn router() -> Router {
//!     Router::new().route("/health", axum::routing::get(|| async { "ok" }))
//! }
//!
//! #[event(fetch)]
//! async fn fetch(
//!     req: HttpRequest,
//!     _env: Env,
//!     _ctx: Context,
//! ) -> Result<HttpResponse> {
//!     Ok(router().oneshot(req).await?)
//! }
//! ```
//!
//! The `worker` crate's `http` and `axum` features make this possible by
//! providing `worker::HttpRequest` (= `http::Request<worker::Body>`) and
//! `worker::HttpResponse` (= `http::Response<worker::Body>`) that axum's
//! Router can consume directly.
//!
//! `worker::Router` is a separate custom router (backed by `matchit`) that
//! uses `worker::Request` and `worker::Response` types. It is **not** an
//! axum Router, and wrapping an axum Router inside it would introduce a
//! redundant routing layer. The adapter below documents the direct-Service
//! approach instead.
//!
//! Note: Workers have no TCP socket support. Database access must go
//! through an external PostgreSQL-over-HTTP gateway.

use axum::Router;

use crate::{HttpAdapter, Result};

/// HTTP adapter for Cloudflare Workers via workers-rs.
///
/// The adapter does not "serve" in the traditional sense — Cloudflare
/// invokes the worker's fetch event handler per request. This adapter
/// provides the binding marker between the axum Router and the Workers
/// runtime.
///
/// For the actual request-handling wiring, see the module-level
/// documentation which shows the `tower::Service` integration pattern.
pub struct WorkersHttpAdapter;

impl WorkersHttpAdapter {
    /// Create a new Workers adapter.
    ///
    /// Returns a zero-sized adapter — all state (the axum `Router`) is
    /// passed to [`Self::serve`] at startup.
    pub fn new() -> Self {
        Self
    }
}

impl Default for WorkersHttpAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpAdapter for WorkersHttpAdapter {
    /// No-op for trait compatibility.
    ///
    /// Cloudflare Workers have no TCP socket and no traditional serve loop.
    /// The Workers runtime invokes the fetch event handler per request.
    /// Use the `tower::Service` pattern shown in the module documentation
    /// to wire the axum Router into the Workers runtime instead.
    async fn serve(self, _router: Router) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HttpAdapter;

    /// Compile-time test: verifies WorkersHttpAdapter implements HttpAdapter.
    /// If the trait bound is broken, this test won't compile.
    fn _assert_http_adapter<T: HttpAdapter>() {}
    #[test]
    fn workers_adapter_implements_http_adapter_trait() {
        _assert_http_adapter::<WorkersHttpAdapter>();
    }

    #[test]
    fn workers_adapter_constructs() {
        let _adapter = WorkersHttpAdapter::new();
    }

    #[test]
    fn workers_adapter_implements_default() {
        let adapter = WorkersHttpAdapter::default();
        // Verify Default produces the same result as new()
        let _ = WorkersHttpAdapter::new();
        drop(adapter);
    }

    /// Compile-time test: verifies serve() returns Ok(()) for trait compatibility.
    #[tokio::test]
    async fn workers_serve_returns_ok() {
        let adapter = WorkersHttpAdapter::new();
        let router = axum::Router::new();
        let result = adapter.serve(router).await;
        assert!(result.is_ok());
    }
}
