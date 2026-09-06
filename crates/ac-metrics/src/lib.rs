//! Metrics collection and Prometheus exposition for Agent Commons.
//!
//! This crate will contain the Prometheus registry, metric definitions,
//! readiness check, and HTTP metrics middleware.
//!
//! # Status
//!
//! Stub — fully implemented in Task 3 of the v0.2 plan.

pub mod middleware;
pub mod readiness;
pub mod registry;
