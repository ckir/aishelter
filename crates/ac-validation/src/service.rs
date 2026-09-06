//! Validation service with database access.
//!
//! [`ValidationService`] coordinates the full validation workflow:
//! recording validator votes, computing quorum, and updating task status.

use ac_types::error::AcError;
use ac_types::task::TaskStatus;
use sqlx::PgPool;

use crate::quorum::QuorumDecision;

/// A single validation decision recorded by a validator.
#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct Validation {
    /// Unique validation record ID.
    pub id: uuid::Uuid,
    /// The task this validation pertains to.
    pub task_id: String,
    /// The validator agent's ID.
    pub validator_agent_id: String,
    /// The validator's decision: `"approve"` or `"reject"`.
    pub decision: String,
    /// Optional reasoning provided by the validator.
    pub reasoning: Option<String>,
    /// When the validation was recorded.
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Service for managing task validations.
///
/// Handles the core validation workflow (§20-22): recording validator
/// votes, computing quorum decisions via [`QuorumResult`](crate::quorum::QuorumResult),
/// and transitioning the task to its final status when quorum is reached.
pub struct ValidationService {
    /// Database connection pool.
    pool: PgPool,
}

impl ValidationService {
    /// Create a new validation service with the given database pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Submit a validation decision for a task.
    ///
    /// This method:
    /// 1. Looks up the task's required validator count
    /// 2. Records the validator's vote (idempotent via `ON CONFLICT DO NOTHING`)
    /// 3. Counts total approvals and rejections
    /// 4. Computes the quorum decision
    /// 5. If quorum is reached, updates the task status
    ///
    /// # Arguments
    ///
    /// * `task_id` — The task being validated.
    /// * `validator_agent_id` — The validator's agent ID.
    /// * `decision` — `"approve"` or `"reject"`.
    /// * `reasoning` — Optional explanation for the decision.
    ///
    /// # Returns
    ///
    /// The [`QuorumDecision`] reflecting the current collective vote.
    pub async fn validate_task(
        &self,
        task_id: &str,
        validator_agent_id: &str,
        decision: &str,
        reasoning: Option<&str>,
    ) -> Result<QuorumDecision, AcError> {
        // Look up the required validator count for this task.
        let required_validators: i32 =
            sqlx::query_scalar("SELECT required_validators FROM tasks WHERE task_id = $1")
                .bind(task_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| match e {
                    sqlx::Error::RowNotFound => AcError::TaskNotFound(task_id.to_string()),
                    _ => AcError::Database(e.to_string()),
                })?;

        // Record the validator's vote (idempotent).
        sqlx::query(
            "INSERT INTO validations (task_id, validator_agent_id, decision, reasoning)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (task_id, validator_agent_id) DO NOTHING",
        )
        .bind(task_id)
        .bind(validator_agent_id)
        .bind(decision)
        .bind(reasoning)
        .execute(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        // Count current approvals.
        let approvals: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM validations WHERE task_id = $1 AND decision = 'approve'",
        )
        .bind(task_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        // Count current rejections.
        let rejections: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM validations WHERE task_id = $1 AND decision = 'reject'",
        )
        .bind(task_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        // Build the quorum result and compute the decision.
        let quorum = crate::quorum::QuorumResult {
            approved: approvals as usize,
            rejected: rejections as usize,
            required: required_validators as usize,
        };

        let decision_result = quorum.decision();

        // If quorum is reached, update the task status.
        if !matches!(decision_result, QuorumDecision::Pending) {
            let new_status = quorum.next_status();
            let status_str = match new_status {
                TaskStatus::Verified => "VERIFIED",
                TaskStatus::Rejected => "REJECTED",
                TaskStatus::Disputed => "DISPUTED",
                _ => "VERIFYING",
            };
            sqlx::query("UPDATE tasks SET status = $1, updated_at = NOW() WHERE task_id = $2")
                .bind(status_str)
                .bind(task_id)
                .execute(&self.pool)
                .await
                .map_err(|e| AcError::Database(e.to_string()))?;
        }

        Ok(decision_result)
    }

    /// Retrieve all validation decisions for a task.
    ///
    /// Returns the full vote history ordered by creation time,
    /// useful for auditing disputes (§22).
    pub async fn get_validations(&self, task_id: &str) -> Result<Vec<Validation>, AcError> {
        sqlx::query_as::<_, Validation>(
            "SELECT id, task_id, validator_agent_id, decision, reasoning, created_at
             FROM validations WHERE task_id = $1 ORDER BY created_at",
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))
    }
}
