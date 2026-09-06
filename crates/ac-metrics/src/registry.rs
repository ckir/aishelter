//! Prometheus registry and metric definitions.
//!
//! Defines the [`Metrics`] struct that holds a shared Prometheus registry
//! and all pre-defined metrics for the Agent Commons server.

use prometheus_client::{
    encoding::{EncodeLabelKey, EncodeLabelSet, EncodeLabelValue, LabelSetEncoder},
    metrics::{counter::Counter, family::Family, gauge::Gauge, histogram::Histogram},
    registry::Registry,
};
use std::sync::Arc;

/// Metric label key-value pairs for HTTP request metrics.
///
/// Used as the label type for `Family` metrics that track HTTP requests
/// and request duration, broken down by method, path, and response status.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct MetricLabels {
    /// HTTP method (GET, POST, etc.).
    pub method: String,
    /// Request path (e.g. `/v1/agents`).
    pub path: String,
    /// Response status code (e.g. `200`).
    pub status: String,
}

impl EncodeLabelSet for MetricLabels {
    fn encode(&self, mut encoder: LabelSetEncoder) -> Result<(), std::fmt::Error> {
        // Encode method="...",path="...",status="..."
        let mut label = encoder.encode_label();
        let mut key_enc = label.encode_label_key()?;
        EncodeLabelKey::encode(&"method", &mut key_enc)?;
        let mut val_enc = key_enc.encode_label_value()?;
        EncodeLabelValue::encode(&self.method, &mut val_enc)?;
        val_enc.finish()?;

        let mut label = encoder.encode_label();
        let mut key_enc = label.encode_label_key()?;
        EncodeLabelKey::encode(&"path", &mut key_enc)?;
        let mut val_enc = key_enc.encode_label_value()?;
        EncodeLabelValue::encode(&self.path, &mut val_enc)?;
        val_enc.finish()?;

        let mut label = encoder.encode_label();
        let mut key_enc = label.encode_label_key()?;
        EncodeLabelKey::encode(&"status", &mut key_enc)?;
        let mut val_enc = key_enc.encode_label_value()?;
        EncodeLabelValue::encode(&self.status, &mut val_enc)?;
        val_enc.finish()?;

        Ok(())
    }
}

/// Label for task status metrics.
///
/// Wraps a single string value (e.g. `"running"`, `"completed"`, `"failed"`)
/// used as the label for the `active_tasks` gauge family.
#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub struct TaskStatusLabel(pub String);

impl EncodeLabelSet for TaskStatusLabel {
    fn encode(&self, mut encoder: LabelSetEncoder) -> Result<(), std::fmt::Error> {
        let mut label = encoder.encode_label();
        let mut key_enc = label.encode_label_key()?;
        EncodeLabelKey::encode(&"status", &mut key_enc)?;
        let mut val_enc = key_enc.encode_label_value()?;
        EncodeLabelValue::encode(&self.0, &mut val_enc)?;
        val_enc.finish()?;
        Ok(())
    }
}

/// Histogram bucket boundaries in seconds.
///
/// Returns buckets from 1ms to 10s, suitable for measuring both fast
/// database queries and slower HTTP request latencies.
fn standard_buckets() -> impl Iterator<Item = f64> {
    [0.001, 0.005, 0.01, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0, 10.0].into_iter()
}

/// Shared metrics holder.
///
/// Holds a Prometheus registry and all pre-defined metrics used by the
/// Agent Commons server. Cloning is cheap (uses `Arc` internally for the
/// registry; individual metrics use `Arc` within `Family` and `Histogram`).
#[derive(Clone)]
pub struct Metrics {
    /// Prometheus registry shared via `Arc`.
    pub registry: Arc<Registry>,
    /// Total HTTP requests, labeled by [`MetricLabels`].
    pub http_requests_total: Family<MetricLabels, Counter>,
    /// HTTP request duration in seconds, labeled by [`MetricLabels`].
    pub http_request_duration_seconds: Family<MetricLabels, Histogram>,
    /// Currently active database pool connections.
    pub db_pool_connections_active: Gauge,
    /// Database query duration in seconds.
    pub db_query_duration_seconds: Histogram,
    /// Active tasks grouped by status, labeled by [`TaskStatusLabel`].
    pub active_tasks: Family<TaskStatusLabel, Gauge>,
    /// Unacknowledged messages pending in the mailbox.
    pub messages_pending: Gauge,
}

impl Metrics {
    /// Create a new [`Metrics`] with a fresh registry and all metrics
    /// initialized to zero.
    ///
    /// Registers six metrics:
    /// - `http_requests_total` — counter labeled by method, path, status
    /// - `http_request_duration_seconds` — histogram labeled by method, path
    /// - `db_pool_connections_active` — gauge for active DB connections
    /// - `db_query_duration_seconds` — histogram for SQL query latency
    /// - `active_tasks` — gauge family labeled by task status
    /// - `messages_pending` — gauge for pending mailbox messages
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let mut registry = Registry::default();

        let http_requests_total = Family::<MetricLabels, Counter>::default();
        registry.register(
            "http_requests_total",
            "Total HTTP requests",
            http_requests_total.clone(),
        );

        let http_request_duration_seconds =
            Family::<MetricLabels, Histogram>::new_with_constructor(|| {
                Histogram::new(standard_buckets())
            });
        registry.register(
            "http_request_duration_seconds",
            "HTTP request latency",
            http_request_duration_seconds.clone(),
        );

        let db_pool_connections_active = Gauge::default();
        registry.register(
            "db_pool_connections_active",
            "Active DB pool connections",
            db_pool_connections_active.clone(),
        );

        let db_query_duration_seconds = Histogram::new(standard_buckets());
        registry.register(
            "db_query_duration_seconds",
            "SQL query latency",
            db_query_duration_seconds.clone(),
        );

        let active_tasks = Family::<TaskStatusLabel, Gauge>::default();
        registry.register("active_tasks", "Tasks by status", active_tasks.clone());

        let messages_pending = Gauge::default();
        registry.register("messages_pending", "Unacknowledged messages", messages_pending.clone());

        Self {
            registry: Arc::new(registry),
            http_requests_total,
            http_request_duration_seconds,
            db_pool_connections_active,
            db_query_duration_seconds,
            active_tasks,
            messages_pending,
        }
    }

    /// Encode all registered metrics to Prometheus text exposition format.
    ///
    /// Returns a `String` suitable for serving on the `/metrics` endpoint.
    pub fn encode(&self) -> String {
        let mut buffer = String::new();
        prometheus_client::encoding::text::encode(&mut buffer, &self.registry)
            .expect("failed to encode metrics");
        buffer
    }
}
