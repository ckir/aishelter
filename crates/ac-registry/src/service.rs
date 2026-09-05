use ac_types::error::AcError;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AgentRow {
    pub agent_id: String,
    pub public_key: String,
    pub profile_name: Option<String>,
    pub profile_description: Option<String>,
    pub status: String,
    pub created_at: chrono::DateTime<Utc>,
    pub updated_at: chrono::DateTime<Utc>,
}

/// Agent registration and Agent Card CRUD service.
pub struct RegistryService {
    pool: PgPool,
}

impl RegistryService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Register a new agent. Returns error if public_key is already registered.
    pub async fn register_agent(
        &self,
        agent_id: &str,
        public_key: &str,
        profile_name: Option<String>,
        profile_description: Option<String>,
    ) -> Result<AgentRow, AcError> {
        let now = Utc::now();

        sqlx::query_as::<_, AgentRow>(
            r#"
            INSERT INTO agents (agent_id, public_key, profile_name, profile_description, status, created_at, updated_at)
            VALUES ($1, $2, $3, $4, 'REGISTERED', $5, $6)
            ON CONFLICT (public_key) DO NOTHING
            RETURNING agent_id, public_key, profile_name, profile_description, status, created_at, updated_at
            "#,
        )
        .bind(agent_id)
        .bind(public_key)
        .bind(profile_name)
        .bind(profile_description)
        .bind(now)
        .bind(now)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?
        .ok_or_else(|| AcError::DuplicateRegistration(public_key.to_string()))
    }

    /// Retrieve an agent by ID.
    pub async fn get_agent(&self, agent_id: &str) -> Result<AgentRow, AcError> {
        sqlx::query_as::<_, AgentRow>(
            r#"
            SELECT agent_id, public_key, profile_name, profile_description, status, created_at, updated_at
            FROM agents
            WHERE agent_id = $1
            "#,
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?
        .ok_or_else(|| AcError::AgentNotFound(agent_id.to_string()))
    }

    /// Update an agent's card (name and description).
    pub async fn update_card(
        &self,
        agent_id: &str,
        name: Option<String>,
        description: Option<String>,
    ) -> Result<(), AcError> {
        let now = Utc::now();

        let rows_affected = sqlx::query(
            r#"
            UPDATE agents
            SET profile_name = $1, profile_description = $2, updated_at = $3
            WHERE agent_id = $4
            "#,
        )
        .bind(name)
        .bind(description)
        .bind(now)
        .bind(agent_id)
        .execute(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?
        .rows_affected();

        if rows_affected == 0 {
            return Err(AcError::AgentNotFound(agent_id.to_string()));
        }

        Ok(())
    }
}
