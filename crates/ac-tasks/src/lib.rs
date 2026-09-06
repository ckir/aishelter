//! Task lifecycle management (§18-20).
//!
//! This crate implements the task protocol: creating task contracts,
//! accepting/rejecting task offers, submitting results, and validating
//! them through a quorum of peer validators.  The state machine governing
//! valid transitions lives in [`lifecycle`].

/// Axum HTTP route handlers for task operations.
pub mod handler;
/// Task state machine: valid transitions between [`ac_types::task::TaskStatus`] variants.
pub mod lifecycle;
/// Core task service with database access.
pub mod service;
