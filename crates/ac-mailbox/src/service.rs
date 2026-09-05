use sqlx::PgPool;
use ac_types::message::Message;
use ac_types::error::AcError;
use chrono::Utc;
use uuid::Uuid;

pub struct MailboxService {
    pool: PgPool,
}

impl MailboxService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn send_message(
        &self,
        from_agent_id: &str,
        to_agent_id: &str,
        message_type: &str,
        payload: serde_json::Value,
        expires_at: Option<chrono::DateTime<Utc>>,
    ) -> Result<Message, AcError> {
        // Verify both agents exist
        let from_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM agents WHERE agent_id = $1)"
        )
        .bind(from_agent_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        if !from_exists {
            return Err(AcError::AgentNotFound(from_agent_id.to_string()));
        }

        let to_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM agents WHERE agent_id = $1)"
        )
        .bind(to_agent_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        if !to_exists {
            return Err(AcError::AgentNotFound(to_agent_id.to_string()));
        }

        let message_id = Uuid::new_v4();
        let created_at = Utc::now();

        let message = sqlx::query_as::<_, Message>(
            r#"INSERT INTO messages (message_id, from_agent_id, to_agent_id, message_type, payload, expires_at, created_at, acknowledged)
               VALUES ($1, $2, $3, $4, $5, $6, $7, false)
               RETURNING *"#
        )
        .bind(message_id)
        .bind(from_agent_id)
        .bind(to_agent_id)
        .bind(message_type)
        .bind(&payload)
        .bind(expires_at)
        .bind(created_at)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        Ok(message)
    }

    pub async fn get_messages(
        &self,
        agent_id: &str,
        only_unacknowledged: bool,
    ) -> Result<Vec<Message>, AcError> {
        let query = if only_unacknowledged {
            "SELECT * FROM messages WHERE to_agent_id = $1 AND acknowledged = false AND (expires_at IS NULL OR expires_at > NOW()) ORDER BY created_at DESC"
        } else {
            "SELECT * FROM messages WHERE to_agent_id = $1 AND (expires_at IS NULL OR expires_at > NOW()) ORDER BY created_at DESC"
        };

        let messages = sqlx::query_as::<_, Message>(query)
            .bind(agent_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AcError::Database(e.to_string()))?;

        Ok(messages)
    }

    pub async fn get_message(&self, message_id: &str) -> Result<Message, AcError> {
        let message_id = Uuid::parse_str(message_id)
            .map_err(|e| AcError::Internal(format!("invalid message_id: {}", e)))?;

        sqlx::query_as::<_, Message>("SELECT * FROM messages WHERE message_id = $1")
            .bind(message_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AcError::Database(e.to_string()))?
            .ok_or_else(|| AcError::TaskNotFound(message_id.to_string()))
    }

    pub async fn acknowledge_message(&self, message_id: &str) -> Result<(), AcError> {
        let message_id = Uuid::parse_str(message_id)
            .map_err(|e| AcError::Internal(format!("invalid message_id: {}", e)))?;

        let result = sqlx::query("UPDATE messages SET acknowledged = true, acknowledged_at = NOW() WHERE message_id = $1")
            .bind(message_id)
            .execute(&self.pool)
            .await
            .map_err(|e| AcError::Database(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(AcError::TaskNotFound(message_id.to_string()));
        }

        Ok(())
    }
}
