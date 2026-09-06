//! Peer validation and quorum logic for task results.
//!
//! When a task result is submitted, independent validator agents review it
//! and cast approve/reject votes.  Once the required quorum (§20-22) is
//! reached, the task transitions to [`Verified`], [`Rejected`], or [`Disputed`].
//!
//! [`Verified`]: ac_types::task::TaskStatus::Verified
//! [`Rejected`]: ac_types::task::TaskStatus::Rejected
//! [`Disputed`]: ac_types::task::TaskStatus::Disputed

/// Axum route handlers for validation endpoints.
pub mod handler;
/// Quorum computation and decision types.
pub mod quorum;
/// Validation service with database access.
pub mod service;
