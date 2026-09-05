use ac_types::task::TaskStatus;

/// Valid state transitions for tasks.
pub fn is_valid_transition(from: TaskStatus, to: TaskStatus) -> bool {
    matches!(
        (from, to),
        (TaskStatus::Created, TaskStatus::Offered)
            | (TaskStatus::Offered, TaskStatus::Accepted)
            | (TaskStatus::Offered, TaskStatus::Rejected)
            | (TaskStatus::Accepted, TaskStatus::Running)
            | (TaskStatus::Running, TaskStatus::Submitted)
            | (TaskStatus::Submitted, TaskStatus::Verifying)
            | (TaskStatus::Verifying, TaskStatus::Verified)
            | (TaskStatus::Verifying, TaskStatus::Rejected)
            | (TaskStatus::Verifying, TaskStatus::Disputed)
            | (TaskStatus::Verified, TaskStatus::Closed)
            | (TaskStatus::Rejected, TaskStatus::Closed)
            | (TaskStatus::Disputed, TaskStatus::Closed)
    )
}
