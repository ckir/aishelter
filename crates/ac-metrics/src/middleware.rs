//! Axum middleware that records HTTP request metrics.
//!
//! Provides the [`record_metrics`] middleware function which observes
//! HTTP requests and records request count and duration to the
//! Prometheus [`Metrics`] registry.

use crate::registry::{MetricLabels, Metrics};
use axum::{body::Body, extract::Extension, http::Request, middleware::Next, response::Response};
use std::time::Instant;

/// Axum middleware that records HTTP request metrics.
///
/// Records request count and duration with method, path, and status
/// labels. Expects [`Metrics`] to be available via [`Extension`],
/// set by the server during router construction.
///
/// # Example
///
/// ```ignore
/// let app = Router::new()
///     .layer(Extension(metrics))
///     .layer(axum::middleware::from_fn(record_metrics));
/// ```
pub async fn record_metrics(
    Extension(metrics): Extension<Metrics>,
    req: Request<Body>,
    next: Next,
) -> Response {
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let start = Instant::now();

    let resp = next.run(req).await;

    let status = resp.status().as_u16().to_string();
    let duration = start.elapsed().as_secs_f64();

    let labels = MetricLabels { method, path, status };
    metrics.http_requests_total.get_or_create(&labels).inc();
    metrics.http_request_duration_seconds.get_or_create(&labels).observe(duration);

    resp
}
