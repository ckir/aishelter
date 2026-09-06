//! Task state machine: valid transitions between [`ac_types::task::TaskStatus`] variants.
//!
//! The task lifecycle (§18) is a directed graph.  This module provides the
//! sole authority on which transitions are legal, preventing invalid state
//! changes at both the API and service layers.
//!
//! # Valid transitions
//!
//! ```text
//! CREATED → OFFERED → ACCEPTED → RUNNING → SUBMITTED → VERIFYING → VERIFIED → CLOSED
//!                   → REJECTED                                         → REJECTED → CLOSED
//!                                                                  → DISPUTED → CLOSED
//! ```

use ac_types::task::TaskStatus;

/// Check whether a transition from `from` to `to` is valid.
///
/// // Returns true only for transitions defined in the task protocol (§18).
pub fn is_valid_transition(from: TaskStatus, to: TaskStatus) -> bool {
    matches!(
        (from, to),
        // Normal forward path.
        (TaskStatus::Created, TaskStatus::Offered)
            | (TaskStatus::Offered, TaskStatus::Accepted)
            | (TaskStatus::Accepted, TaskStatus::Running)
            | (TaskStatus::Running, TaskStatus::Submitted)
            | (TaskStatus::Submitted, TaskStatus::Verifying)
            | (TaskStatus::Verifying, TaskStatus::Verified)
            // Rejection branch from OFFERED or VERIFYING.
            | (TaskStatus::Offered, TaskStatus::Rejected)
            | (TaskStatus::Verifying, TaskStatus::Rejected)
            // Dispute branch from VERIFYING.
            | (TaskStatus::Verifying, TaskStatus::Disputed)
            // Closure: all terminal states lead to CLOSED.
            | (TaskStatus::Verified, TaskStatus::Closed)
            | (TaskStatus::Rejected, TaskStatus::Closed)
            | (TaskStatus::Disputed, TaskStatus::Closed)
    )
}
