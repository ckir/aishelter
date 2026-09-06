//! Quorum computation and decision types.
//!
//! Defines the [`QuorumDecision`] enum and [`QuorumResult`] struct that
//! determine whether a task result passes, fails, or enters a dispute
//! based on validator votes (§20-22).

use ac_types::task::TaskStatus;
use serde::{Deserialize, Serialize};

/// The outcome of a quorum vote on a task result.
///
/// The decision is determined by comparing approval/rejection counts
/// against the task's required validator threshold.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QuorumDecision {
    /// The result passed validation (approvals >= required).
    Verified,
    /// The result failed validation (rejections >= required).
    Rejected,
    /// Validators disagreed with both approvals and rejections present.
    Disputed,
    /// Not enough votes to reach a conclusion yet.
    Pending,
}

/// Aggregated quorum result from validator votes.
///
/// This struct holds the raw counts and provides the [`decision`](QuorumResult::decision)
/// and [`next_status`](QuorumResult::next_status) methods that determine
/// the task's fate.
pub struct QuorumResult {
    /// Number of validators who approved the result.
    pub approved: usize,
    /// Number of validators who rejected the result.
    pub rejected: usize,
    /// Number of approvals required to pass validation.
    pub required: usize,
}

impl QuorumResult {
    /// Compute the quorum decision from the current vote counts.
    ///
    /// # Decision logic
    ///
    /// 1. If `approved >= required` → [`Verified`](QuorumDecision::Verified)
    /// 2. If `rejected >= required` → [`Rejected`](QuorumDecision::Rejected)
    /// 3. If total votes >= required AND both sides have votes → [`Disputed`](QuorumDecision::Disputed)
    /// 4. Otherwise → [`Pending`](QuorumDecision::Pending)
    pub fn decision(&self) -> QuorumDecision {
        if self.approved >= self.required {
            QuorumDecision::Verified
        } else if self.rejected >= self.required {
            QuorumDecision::Rejected
        } else if (self.approved + self.rejected) >= self.required
            && self.approved > 0
            && self.rejected > 0
        {
            QuorumDecision::Disputed
        } else {
            QuorumDecision::Pending
        }
    }

    /// Map the quorum decision to the corresponding task status.
    ///
    /// Used to transition the task to its final state once quorum is reached.
    pub fn next_status(&self) -> TaskStatus {
        match self.decision() {
            QuorumDecision::Verified => TaskStatus::Verified,
            QuorumDecision::Rejected => TaskStatus::Rejected,
            QuorumDecision::Disputed => TaskStatus::Disputed,
            QuorumDecision::Pending => TaskStatus::Verifying,
        }
    }
}

/// Compute a quorum decision from raw vote counts.
///
/// This is a convenience function that constructs a [`QuorumResult`]
/// and returns its [`decision`](QuorumResult::decision).
pub fn compute_quorum(required: usize, approvals: usize, rejections: usize) -> QuorumDecision {
    let result = QuorumResult { approved: approvals, rejected: rejections, required };
    result.decision()
}
