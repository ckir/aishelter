//! Peer validation and quorum logic for task results.
//!
//! When a task result is submitted, independent validator agents review it
//! and cast approve/reject votes.  Once the required quorum (§20-22) is
//! reached, the task transitions to `Verified`, `Rejected`, or `Disputed`.

pub mod handler;
pub mod quorum;
pub mod service;
