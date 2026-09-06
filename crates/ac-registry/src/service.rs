//! Agent registry service layer.
//!
//! The [`RegistryService`] provides database operations for agent
//! registration and card management (§9-12).  Each method uses sqlx
//! parameterized queries to prevent SQL injection.
//!
//! The [`AgentRow`] struct mirrors the `agents` table schema and implements
//! [`sqlx::FromRow`] for type-safe row mapping.

use ac_types::error::AcError;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

/// Row mapping for the `agents` table.
///
/// This struct is used internally by [`RegistryService`] to map query
/// results.  It derives [`sqlx::FromRow`] for automatic column-to-field
/// binding.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AgentRow {
    /// Unique agent identifier.
    pub agent_id: String,
    /// Ed25519 public key (hex-encoded).
    pub public_key: String,
    /// Human-readable display name (nullable).
    pub profile_name: Option<String>,
    /// Human-readable description (nullable).
    pub profile_description: Option<String>,
    /// Lifecycle status string (e.g. `"REGISTERED"`).
    pub status: String,
    /// When the agent record was created.
    pub created_at: chrono::DateTime<Utc>,
    /// When the agent record was last updated.
    pub updated_at: chrono::DateTime<Utc>,
}

/// Agent registration and Agent Card CRUD service.
///
/// Wraps a [`PgPool`] and provides typed methods for inserting, selecting,
/// and updating rows in the `agents` table.  All errors are mapped to
/// [`AcError`] variants.
pub struct RegistryService {
    /// The PostgreSQL connection pool.
    pool: PgPool,
}

impl RegistryService {
    /// Create a new [`RegistryService`] from an existing connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Register a new agent.
    ///
    /// Inserts a row into the `agents` table with status `REGISTERED` and
    /// the current UTC timestamp.  Uses `ON CONFLICT (public_key) DO NOTHING`
    /// so that if the public key is already registered, the query returns
    /// zero rows and we map that to [`AcError::DuplicateRegistration`].
    pub async fn register_agent(
        &self,
        agent_id: &str,
        public_key: &str,
        profile_name: Option<String>,
        profile_description: Option<String>,
    ) -> Result<AgentRow, AcError> {
        // Capture the current UTC time for audit timestamps.
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
    ///
    /// Queries the `agents` table by `agent_id`.  Returns
    /// [`AcError::AgentNotFound`] if no matching row exists.
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
    ///
    /// Updates the `profile_name`, `profile_description`, and `updated_at`
    /// columns for the given `agent_id`.  Returns [`AcError::AgentNotFound`]
    /// if no row was updated (i.e. the agent does not exist).
    pub async fn update_card(
        &self,
        agent_id: &str,
        name: Option<String>,
        description: Option<String>,
    ) -> Result<(), AcError> {
        // Use current UTC time for the updated_at audit column.
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

        // Check if any row was actually updated.
        if rows_affected == 0 {
            return Err(AcError::AgentNotFound(agent_id.to_string()));
        }

        Ok(())
    }
}
