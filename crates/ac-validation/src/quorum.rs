use ac_types::task::TaskStatus;

/// Quorum decision from validator votes.
pub struct QuorumResult {
    pub approved: usize,
    pub rejected: usize,
    pub required: usize,
}

impl QuorumResult {
    pub fn is_verified(&self) -> bool {
        self.approved >= self.required
    }

    pub fn is_disputed(&self) -> bool {
        self.approved > 0 && self.rejected > 0 && !self.is_verified()
    }

    pub fn is_rejected(&self) -> bool {
        self.rejected >= self.required
    }

    pub fn next_status(&self) -> TaskStatus {
        if self.is_verified() {
            TaskStatus::Verified
        } else if self.is_rejected() {
            TaskStatus::Rejected
        } else if self.is_disputed() {
            TaskStatus::Disputed
        } else {
            TaskStatus::Verifying
        }
    }
}
