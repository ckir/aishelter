//! Core task service with database access (§18-20).
//!
//! The [`TaskService`] owns all database interactions for the task
//! subsystem: creating task contracts, querying task state, transitioning
//! tasks through the lifecycle state machine, and recording submitted
//! results.

use ac_types::error::AcError;
use ac_types::task::TaskStatus;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

use crate::lifecycle::is_valid_transition;

/// Task service backed by a PostgreSQL connection pool.
pub struct TaskService {
    /// PostgreSQL connection pool.
    pool: PgPool,
}

impl TaskService {
    /// Create a new task service from a connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new task contract and store it in the database.
    ///
    /// // Generate a UUID-based task ID.
    /// // Insert the row with status = 'CREATED'.
    #[allow(clippy::too_many_arguments)]
    pub async fn create_task(
        &self,
        requester_id: &str,
        capability: &str,
        description: &str,
        input: serde_json::Value,
        deadline: Option<chrono::DateTime<Utc>>,
        verification_method: &str,
        required_validators: i32,
    ) -> Result<String, AcError> {
        let task_id = format!("task_{}", Uuid::new_v4());
        let now = Utc::now();

        sqlx::query(
            "INSERT INTO tasks (task_id, requester_agent_id, capability, description, input, constraints_deadline, verification_method, required_validators, status, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'CREATED', $9, $9)",
        )
        .bind(&task_id)
        .bind(requester_id)
        .bind(capability)
        .bind(description)
        .bind(&input)
        .bind(deadline)
        .bind(verification_method)
        .bind(required_validators)
        .bind(now)
        .execute(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        Ok(task_id)
    }

    /// Retrieve full task details by ID.
    ///
    /// // Read all task columns and return as a JSON object.
    pub async fn get_task(&self, task_id: &str) -> Result<serde_json::Value, AcError> {
        // Pre-declare the tuple type for the SELECT columns.
        type TaskRow = (
            String,                        // task_id
            String,                        // requester_agent_id
            Option<String>,                // assigned_agent_id
            String,                        // capability
            String,                        // description
            serde_json::Value,             // input
            Option<chrono::DateTime<Utc>>, // constraints_deadline
            String,                        // verification_method
            i32,                           // required_validators
            String,                        // status
            chrono::DateTime<Utc>,         // created_at
            chrono::DateTime<Utc>,         // updated_at
        );
        let row: Option<TaskRow> = sqlx::query_as(
            "SELECT task_id, requester_agent_id, assigned_agent_id, capability, description,
                        input, constraints_deadline, verification_method, required_validators,
                        status, created_at, updated_at
                 FROM tasks WHERE task_id = $1",
        )
        .bind(task_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        match row {
            Some(r) => Ok(serde_json::json!({
                "task_id": r.0, "requester": r.1, "assigned_agent_id": r.2,
                "capability": r.3, "description": r.4, "input": r.5,
                "deadline": r.6, "verification_method": r.7,
                "required_validators": r.8, "status": r.9,
                "created_at": r.10, "updated_at": r.11,
            })),
            None => Err(AcError::TaskNotFound(task_id.to_string())),
        }
    }

    /// Accept a task offer.
    ///
    /// // Validates that the current status is CREATED or OFFERED.
    /// // Sets assigned_agent_id and transitions to ACCEPTED.
    pub async fn accept_task(&self, task_id: &str, agent_id: &str) -> Result<(), AcError> {
        self.do_transition(
            task_id,
            agent_id,
            &[TaskStatus::Created, TaskStatus::Offered],
            TaskStatus::Accepted,
            true,
        )
        .await
    }

    /// Reject a task offer.
    ///
    /// // Only valid from the OFFERED state.
    pub async fn reject_task(&self, task_id: &str, _agent_id: &str) -> Result<(), AcError> {
        self.transition_task_simple(task_id, TaskStatus::Offered, TaskStatus::Rejected).await
    }

    /// Submit a task result for validation.
    ///
    /// // Checks that the current status allows a transition to SUBMITTED.
    /// // Inserts a row into task_results with the output hash.
    pub async fn submit_result(
        &self,
        task_id: &str,
        agent_id: &str,
        result: serde_json::Value,
        output_hash: &str,
    ) -> Result<(), AcError> {
        // Read the current task status to validate the transition.
        let current_status: String =
            sqlx::query_scalar("SELECT status FROM tasks WHERE task_id = $1")
                .bind(task_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| AcError::Database(e.to_string()))?
                .ok_or_else(|| AcError::TaskNotFound(task_id.to_string()))?;

        let from: TaskStatus = current_status
            .parse()
            .map_err(|_| AcError::Internal(format!("invalid task status: {}", current_status)))?;

        // Reject if the state machine doesn't allow this transition.
        if !is_valid_transition(from, TaskStatus::Submitted) {
            return Err(AcError::InvalidTaskTransition {
                from: current_status,
                to: "SUBMITTED".to_string(),
            });
        }

        // Transition the task to SUBMITTED.
        sqlx::query("UPDATE tasks SET status = 'SUBMITTED', updated_at = NOW() WHERE task_id = $1")
            .bind(task_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AcError::Database(e.to_string()))?;

        // Record the submitted result with its output hash.
        sqlx::query(
            "INSERT INTO task_results (task_id, agent_id, result_data, output_hash, submitted_at)
             VALUES ($1, $2, $3, $4, NOW())",
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

    /// Transition a task, optionally setting the assigned agent.
    ///
    /// // Validates the current status is in from_allowed.
    /// // Checks is_valid_transition for the state machine rule.
    /// // Updates status and optionally assigned_agent_id.
    async fn do_transition(
        &self,
        task_id: &str,
        agent_id: &str,
        from_allowed: &[TaskStatus],
        to: TaskStatus,
        set_assigned: bool,
    ) -> Result<(), AcError> {
        let current_status: String =
            sqlx::query_scalar("SELECT status FROM tasks WHERE task_id = $1")
                .bind(task_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| AcError::Database(e.to_string()))?
                .ok_or_else(|| AcError::TaskNotFound(task_id.to_string()))?;

        let from: TaskStatus = current_status
            .parse()
            .map_err(|_| AcError::Internal(format!("invalid task status: {}", current_status)))?;

        // Both the allowed-from list and the state machine must agree.
        if !from_allowed.contains(&from) || !is_valid_transition(from, to) {
            return Err(AcError::InvalidTaskTransition {
                from: current_status,
                to: format!("{:?}", to),
            });
        }

        if set_assigned {
            sqlx::query(
                "UPDATE tasks SET status = $1, assigned_agent_id = $2, updated_at = NOW() WHERE task_id = $3",
            )
            .bind(format!("{:?}", to))
            .bind(agent_id)
            .bind(task_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AcError::Database(e.to_string()))?;
        } else {
            sqlx::query("UPDATE tasks SET status = $1, updated_at = NOW() WHERE task_id = $2")
                .bind(format!("{:?}", to))
                .bind(task_id)
                .execute(&self.pool)
                .await
                .map_err(|e| AcError::Database(e.to_string()))?;
        }

        Ok(())
    }

    /// Transition a task without modifying assigned_agent_id.
    ///
    /// // Used for simple single-from transitions like OFFERED→REJECTED.
    async fn transition_task_simple(
        &self,
        task_id: &str,
        from: TaskStatus,
        to: TaskStatus,
    ) -> Result<(), AcError> {
        let current_status: String =
            sqlx::query_scalar("SELECT status FROM tasks WHERE task_id = $1")
                .bind(task_id)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| AcError::Database(e.to_string()))?
                .ok_or_else(|| AcError::TaskNotFound(task_id.to_string()))?;

        let cur: TaskStatus = current_status
            .parse()
            .map_err(|_| AcError::Internal(format!("invalid task status: {}", current_status)))?;

        // Both the expected-from state and the state machine must agree.
        if cur != from || !is_valid_transition(cur, to) {
            return Err(AcError::InvalidTaskTransition {
                from: current_status,
                to: format!("{:?}", to),
            });
        }

        sqlx::query("UPDATE tasks SET status = $1, updated_at = NOW() WHERE task_id = $2")
            .bind(format!("{:?}", to))
            .bind(task_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AcError::Database(e.to_string()))?;

        Ok(())
    }
}
