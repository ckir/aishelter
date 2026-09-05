use ac_types::error::AcError;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

type MessageRow = (
    Uuid,
    String,
    String,
    String,
    serde_json::Value,
    Option<chrono::DateTime<Utc>>,
    chrono::DateTime<Utc>,
    bool,
);

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
    ) -> Result<Uuid, AcError> {
        for agent_id in [from_agent_id, to_agent_id] {
            let exists: bool =
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM agents WHERE agent_id = $1)")
                    .bind(agent_id)
                    .fetch_one(&self.pool)
                    .await
                    .map_err(|e| AcError::Database(e.to_string()))?;
            if !exists {
                return Err(AcError::AgentNotFound(agent_id.to_string()));
            }
        }

        let message_id = Uuid::new_v4();
        let created_at = Utc::now();

        sqlx::query(
            "INSERT INTO messages (message_id, from_agent_id, to_agent_id, message_type, payload, expires_at, created_at, acknowledged)
             VALUES ($1, $2, $3, $4, $5, $6, $7, false)"
        )
        .bind(message_id)
        .bind(from_agent_id)
        .bind(to_agent_id)
        .bind(message_type)
        .bind(&payload)
        .bind(expires_at)
        .bind(created_at)
        .execute(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        Ok(message_id)
    }

    pub async fn get_messages(
        &self,
        agent_id: &str,
        only_unacknowledged: bool,
    ) -> Result<Vec<serde_json::Value>, AcError> {
        let query = if only_unacknowledged {
            "SELECT message_id, from_agent_id, to_agent_id, message_type, payload, expires_at, created_at, acknowledged
             FROM messages WHERE to_agent_id = $1 AND acknowledged = false
             AND (expires_at IS NULL OR expires_at > NOW()) ORDER BY created_at DESC"
        } else {
            "SELECT message_id, from_agent_id, to_agent_id, message_type, payload, expires_at, created_at, acknowledged
             FROM messages WHERE to_agent_id = $1
             AND (expires_at IS NULL OR expires_at > NOW()) ORDER BY created_at DESC"
        };

        let rows: Vec<MessageRow> = sqlx::query_as(query)
            .bind(agent_id)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AcError::Database(e.to_string()))?;

        Ok(rows
            .into_iter()
            .map(|r| {
                serde_json::json!({
                    "message_id": r.0,
                    "from": r.1,
                    "to": r.2,
                    "type": r.3,
                    "payload": r.4,
                    "expires_at": r.5,
                    "created_at": r.6,
                    "acknowledged": r.7,
                })
            })
            .collect())
    }

    pub async fn get_message(&self, message_id: &str) -> Result<serde_json::Value, AcError> {
        let id = Uuid::parse_str(message_id)
            .map_err(|e| AcError::Internal(format!("invalid message_id: {}", e)))?;

        let row: Option<MessageRow> =
            sqlx::query_as(
                "SELECT message_id, from_agent_id, to_agent_id, message_type, payload, expires_at, created_at, acknowledged
                 FROM messages WHERE message_id = $1"
            )
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| AcError::Database(e.to_string()))?;

        match row {
            Some(r) => Ok(serde_json::json!({
                "message_id": r.0, "from": r.1, "to": r.2, "type": r.3,
                "payload": r.4, "expires_at": r.5, "created_at": r.6, "acknowledged": r.7,
            })),
            None => Err(AcError::TaskNotFound(message_id.to_string())),
        }
    }

    pub async fn acknowledge_message(&self, message_id: &str) -> Result<(), AcError> {
        let id = Uuid::parse_str(message_id)
            .map_err(|e| AcError::Internal(format!("invalid message_id: {}", e)))?;

        let result = sqlx::query(
            "UPDATE messages SET acknowledged = true, acknowledged_at = NOW() WHERE message_id = $1"
        )
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        if result.rows_affected() == 0 {
            return Err(AcError::TaskNotFound(message_id.to_string()));
        }
        Ok(())
    }
}
