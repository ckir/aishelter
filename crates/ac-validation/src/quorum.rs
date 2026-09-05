use ac_types::task::TaskStatus;
use serde::{Deserialize, Serialize};

/// Quorum decision from validator votes.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum QuorumDecision {
    Verified,
    Rejected,
    Disputed,
    Pending,
}

/// Aggregated quorum result.
pub struct QuorumResult {
    pub approved: usize,
    pub rejected: usize,
    pub required: usize,
}

impl QuorumResult {
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

    pub fn next_status(&self) -> TaskStatus {
        match self.decision() {
            QuorumDecision::Verified => TaskStatus::Verified,
            QuorumDecision::Rejected => TaskStatus::Rejected,
            QuorumDecision::Disputed => TaskStatus::Disputed,
            QuorumDecision::Pending => TaskStatus::Verifying,
        }
    }
}

/// Compute quorum decision from vote counts.
pub fn compute_quorum(required: usize, approvals: usize, rejections: usize) -> QuorumDecision {
    let result = QuorumResult {
        approved: approvals,
        rejected: rejections,
        required,
    };
    result.decision()
}
