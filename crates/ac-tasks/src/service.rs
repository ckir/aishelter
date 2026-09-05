use ac_types::error::AcError;
use ac_types::task::{Task, TaskConstraints, TaskStatus, Verification};
use ac_types::agent::AgentId;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

pub struct TaskService {
    pool: PgPool,
}

impl TaskService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new task in the CREATED state.
    pub async fn create_task(
        &self,
        requester_id: String,
        capability: String,
        description: String,
        input: serde_json::Value,
        deadline: Option<chrono::DateTime<Utc>>,
        verification_method: String,
        required_validators: usize,
    ) -> Result<Task, AcError> {
        let task_id = Uuid::new_v4();
        let now = Utc::now();

        let row: (String, String, String, String, String, serde_json::Value, Option<chrono::DateTime<Utc>>, String, i32, String, chrono::DateTime<Utc>) =
            sqlx::query_as(
                r#"
                INSERT INTO tasks (
                    task_id, requester_agent_id, capability, description,
                    input, constraints_deadline, verification_method,
                    required_validators, status, created_at, updated_at
                ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $10)
                RETURNING
                    task_id, requester_agent_id, capability, description,
                    input, constraints_deadline, verification_method,
                    required_validators, status, created_at
                "#,
            )
            .bind(task_id.to_string())
            .bind(&requester_id)
            .bind(&capability)
            .bind(&description)
            .bind(&input)
            .bind(deadline)
            .bind(&verification_method)
            .bind(required_validators as i32)
            .bind("CREATED")
            .bind(now)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AcError::Database(e.to_string()))?;

        Ok(Task {
            task_id,
            requester: AgentId(requester_id),
            capability,
            description,
            input,
            constraints: TaskConstraints { deadline },
            verification: Verification {
                method: verification_method,
                required_validators,
            },
            status: TaskStatus::Created,
            created_at: now,
        })
    }

    /// Get a task by ID.
    pub async fn get_task(&self, task_id: &str) -> Result<Task, AcError> {
        let row: (String, String, String, String, serde_json::Value, Option<chrono::DateTime<Utc>>, String, i32, String, chrono::DateTime<Utc>) =
            sqlx::query_as(
                r#"
                SELECT task_id, requester_agent_id, capability, description,
                       input, constraints_deadline, verification_method,
                       required_validators, status, created_at
                FROM tasks WHERE task_id = $1
                "#,
            )
            .bind(task_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| match e {
                sqlx::Error::RowNotFound => AcError::TaskNotFound(task_id.to_string()),
                _ => AcError::Database(e.to_string()),
            })?;

        let task_uuid = Uuid::parse_str(&row.0)
            .map_err(|e| AcError::Internal(e.to_string()))?;
        let status: TaskStatus = row.8.parse()
            .map_err(|_| AcError::Internal(format!("invalid task status: {}", row.8)))?;

        Ok(Task {
            task_id: task_uuid,
            requester: AgentId(row.1),
            capability: row.2,
            description: row.3,
            input: row.4,
            constraints: TaskConstraints { deadline: row.5 },
            verification: Verification {
                method: row.6,
                required_validators: row.7 as usize,
            },
            status,
            created_at: row.9,
        })
    }

    /// Accept a task — transition to ACCEPTED.
    pub async fn accept_task(&self, task_id: &str, agent_id: &str) -> Result<(), AcError> {
        let current_status: String = sqlx::query_scalar(
            "SELECT status FROM tasks WHERE task_id = $1",
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?
        .ok_or_else(|| AcError::TaskNotFound(task_id.to_string()))?;

        let from: TaskStatus = current_status.parse()
            .map_err(|_| AcError::Internal(format!("invalid task status: {}", current_status)))?;

        if !crate::lifecycle::is_valid_transition(from, TaskStatus::Accepted) {
            return Err(AcError::InvalidTaskTransition {
                from: current_status,
                to: "ACCEPTED".to_string(),
            });
        }

        sqlx::query(
            "UPDATE tasks SET status = 'ACCEPTED', assigned_agent_id = $2, updated_at = NOW() WHERE task_id = $1",
        )
        .bind(task_id)
        .bind(agent_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        Ok(())
    }

    /// Reject a task — transition to REJECTED.
    pub async fn reject_task(&self, task_id: &str, _agent_id: &str) -> Result<(), AcError> {
        let current_status: String = sqlx::query_scalar(
            "SELECT status FROM tasks WHERE task_id = $1",
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?
        .ok_or_else(|| AcError::TaskNotFound(task_id.to_string()))?;

        let from: TaskStatus = current_status.parse()
            .map_err(|_| AcError::Internal(format!("invalid task status: {}", current_status)))?;

        if !crate::lifecycle::is_valid_transition(from, TaskStatus::Rejected) {
            return Err(AcError::InvalidTaskTransition {
                from: current_status,
                to: "REJECTED".to_string(),
            });
        }

        sqlx::query(
            "UPDATE tasks SET status = 'REJECTED', updated_at = NOW() WHERE task_id = $1",
        )
        .bind(task_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        Ok(())
    }

    /// Submit a result for a task — transition to SUBMITTED.
    pub async fn submit_result(
        &self,
        task_id: &str,
        agent_id: &str,
        result: serde_json::Value,
        output_hash: &str,
    ) -> Result<(), AcError> {
        let current_status: String = sqlx::query_scalar(
            "SELECT status FROM tasks WHERE task_id = $1",
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?
        .ok_or_else(|| AcError::TaskNotFound(task_id.to_string()))?;

        let from: TaskStatus = current_status.parse()
            .map_err(|_| AcError::Internal(format!("invalid task status: {}", current_status)))?;

        if !crate::lifecycle::is_valid_transition(from, TaskStatus::Submitted) {
            return Err(AcError::InvalidTaskTransition {
                from: current_status,
                to: "SUBMITTED".to_string(),
            });
        }

        sqlx::query(
            "UPDATE tasks SET status = 'SUBMITTED', updated_at = NOW() WHERE task_id = $1",
        )
        .bind(task_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        sqlx::query(
            "INSERT INTO task_results (task_id, agent_id, result_data, output_hash, submitted_at) VALUES ($1, $2, $3, $4, NOW())",
        )
        .bind(task_id)
        .bind(agent_id)
        .bind(&result)
        .bind(output_hash)
        .execute(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        Ok(())
    }
}
