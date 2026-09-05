use ac_types::error::AcError;
use ac_types::task::TaskStatus;
use sqlx::PgPool;

use crate::quorum::QuorumDecision;

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct Validation {
    pub id: uuid::Uuid,
    pub task_id: String,
    pub validator_agent_id: String,
    pub decision: String,
    pub reasoning: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

pub struct ValidationService {
    pool: PgPool,
}

impl ValidationService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn validate_task(
        &self,
        task_id: &str,
        validator_agent_id: &str,
        decision: &str,
        reasoning: Option<&str>,
    ) -> Result<QuorumDecision, AcError> {
        let required_validators: i32 =
            sqlx::query_scalar("SELECT required_validators FROM tasks WHERE task_id = $1")
                .bind(task_id)
                .fetch_one(&self.pool)
                .await
                .map_err(|e| match e {
                    sqlx::Error::RowNotFound => AcError::TaskNotFound(task_id.to_string()),
                    _ => AcError::Database(e.to_string()),
                })?;

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

        let quorum = crate::quorum::QuorumResult {
            approved: approvals as usize,
            rejected: rejections as usize,
            required: required_validators as usize,
        };

        let decision_result = quorum.decision();

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
