//! Agent Commons HTTP server crate.
//!
//! This crate wires together all feature sub-routes (registry, discovery,
//! mailbox, tasks, validation, reputation) into a single axum [`Router`]
//! and provides the configuration and entry point for the `agent-commons`
//! executable.
//!
//! # Modules
//!
//! - [`config`] — Runtime settings loaded from environment variables.
//! - [`middleware`] — HTTP middleware modules (rate limiting, metrics).
//! - [`routes`] — OpenAPI documentation and Scalar UI endpoint.
//! - [`server`] — Router construction and health/version handlers.

/// Server configuration management.
pub mod config;
/// HTTP middleware modules.
pub mod middleware;
/// OpenAPI documentation and Scalar UI.
pub mod routes;
/// Axum HTTP server construction and routing.
pub mod server;

pub use server::create_app;
