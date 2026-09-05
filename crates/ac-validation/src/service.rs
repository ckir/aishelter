use ac_types::error::AcError;
use ac_types::task::TaskStatus;
use ac_tasks::lifecycle::is_valid_transition;
use sqlx::PgPool;
use tracing::info;

use crate::quorum::{self, QuorumDecision};

/// A validation record from a validator.
#[derive(Debug, sqlx::FromRow)]
pub struct Validation {
    pub id: uuid::Uuid,
    pub task_id: String,
    pub validator_agent_id: String,
    pub decision: String,
    pub reasoning: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Service for task validation and quorum management.
pub struct ValidationService {
    pool: PgPool,
}

impl ValidationService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Submit a validation decision for a task.
    /// After inserting, checks quorum and updates task status if resolved.
    pub async fn validate_task(
        &self,
        task_id: &str,
        validator_agent_id: &str,
        decision: &str,
        reasoning: Option<&str>,
    ) -> Result<QuorumDecision, AcError> {
        // Verify task exists and is in Verifying state
        let current_status: String = sqlx::query_scalar(
            "SELECT status FROM tasks WHERE task_id = $1",
        )
        .bind(task_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| match e {
            sqlx::Error::RowNotFound => AcError::TaskNotFound(task_id.to_string()),
            _ => AcError::Database(e.to_string()),
        })?;

        if current_status != "VERIFYING" && current_status != "VERIFYING" {
            return Err(AcError::InvalidTaskTransition {
                from: current_status,
                to: "VALIDATING".to_string(),
            });
        }

        // Insert validation decision
        sqlx::query(
            "INSERT INTO validations (task_id, validator_agent_id, decision, reasoning)
             VALUES ($1, $2, $3, $4)
             ON CONFLICT (task_id, validator_agent_id)
             DO UPDATE SET decision = $3, reasoning = $4",
        )
        .bind(task_id)
        .bind(validator_agent_id)
        .bind(decision)
        .bind(reasoning)
        .execute(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        // Count votes
        let required_validators: i32 = sqlx::query_scalar(
            "SELECT required_validators FROM tasks WHERE task_id = $1",
        )
        .bind(task_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        let approvals: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM validations WHERE task_id = $1 AND decision = 'approve'",
        )
        .bind(task_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        let rejections: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM validations WHERE task_id = $1 AND decision = 'reject'",
        )
        .bind(task_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        let quorum = quorum::QuorumResult {
            approved: approvals as usize,
            rejected: rejections as usize,
            required: required_validators as usize,
        };

        let decision_result = quorum.decision();

        if matches!(
            decision_result,
            QuorumDecision::Verified | QuorumDecision::Rejected | QuorumDecision::Disputed
        ) {
            let new_status = quorum.next_status();
            sqlx::query(
                "UPDATE tasks SET status = $1, updated_at = NOW() WHERE task_id = $2",
            )
            .bind(task_status_str(&new_status))
            .bind(task_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AcError::Database(e.to_string()))?;

            info!(
                task_id,
                ?decision_result,
                ?new_status,
                "Task validation resolved"
            );
        }

        Ok(decision_result)
    }

    /// Get all validations for a task.
    pub async fn get_validations(&self, task_id: &str) -> Result<Vec<Validation>, AcError> {
        let validations = sqlx::query_as::<_, Validation>(
            "SELECT id, task_id, validator_agent_id, decision, reasoning, created_at
             FROM validations WHERE task_id = $1
             ORDER BY created_at",
        )
        .bind(task_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        Ok(validations)
    }
}

fn task_status_str(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Created => "CREATED",
        TaskStatus::Offered => "OFFERED",
        TaskStatus::Accepted => "ACCEPTED",
        TaskStatus::Running => "RUNNING",
        TaskStatus::Submitted => "SUBMITTED",
        TaskStatus::Verifying => "VERIFYING",
        TaskStatus::Verified => "VERIFIED",
        TaskStatus::Rejected => "REJECTED",
        TaskStatus::Disputed => "DISPUTED",
        TaskStatus::Closed => "CLOSED",
    }
}
