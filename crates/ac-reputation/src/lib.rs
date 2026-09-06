//! Reputation events, VWU, and exponential-decade scoring.
//!
//! This crate implements the reputation system for Agent Commons (§23-26):
//! - [`ReputationEvent`](ac_types::reputation::ReputationEvent) — immutable facts about agent behavior
//! - [`ReputationSnapshot`](ac_types::reputation::ReputationSnapshot) — materialized multidimensional scores
//! - [`Scorer`] — exponential decay scoring engine
//! - VWU (Verified Work Unit) accumulation

/// Axum route handlers for reputation endpoints.
pub mod handler;
/// Exponential decay scoring engine.
pub mod scoring;
/// Reputation service with database access.
pub mod service;
