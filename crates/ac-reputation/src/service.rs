use sqlx::PgPool;
use uuid::Uuid;
use chrono::Utc;
use ac_types::error::AcError;
use ac_types::reputation::{ReputationEvent, ReputationDimension, ReputationSnapshot};
use crate::scoring::Scorer;

/// Statistics about an agent's contribution history.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContributionStats {
    pub vwu_total: i64,
    pub verified_tasks: i64,
    pub validation_tasks: i64,
    pub validation_accuracy: f64,
}

/// The reputation service handles all reputation-related database operations.
pub struct ReputationService {
    pool: PgPool,
}

impl ReputationService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Record a new reputation event for an agent.
    pub async fn record_event(
        &self,
        agent_id: &str,
        event_type: &str,
        task_id: Option<&str>,
        dimension: &str,
        value: f64,
    ) -> Result<ReputationEvent, AcError> {
        let event_id = Uuid::new_v4();
        let timestamp = Utc::now();

        sqlx::query!(
            r#"
            INSERT INTO reputation_events (event_id, agent_id, event_type, task_id, dimension, value, timestamp)
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            "#,
            event_id,
            agent_id,
            event_type,
            task_id,
            dimension,
            value,
            timestamp
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        Ok(ReputationEvent {
            event_id,
            agent_id: ac_types::agent::AgentId(agent_id.to_string()),
            r#type: event_type.to_string(),
            task_id: task_id.map(Uuid::parse_str).transpose()
                .map_err(|e| AcError::Internal(e.to_string()))?,
            dimension: dimension.parse()
                .map_err(|_| AcError::Internal(format!("unknown dimension: {}", dimension)))?,
            value,
            timestamp,
        })
    }

    /// Get the latest reputation scores for an agent.
    pub async fn get_reputation(&self, agent_id: &str) -> Result<ReputationSnapshot, AcError> {
        let row = sqlx::query!(
            r#"
            SELECT
                reliability,
                reliability_confidence,
                task_success,
                task_success_confidence,
                verification_accuracy,
                verification_accuracy_confidence,
                responsiveness,
                responsiveness_confidence,
                vwu_total
            FROM reputation_snapshots
            WHERE agent_id = $1
            "#,
            agent_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        match row {
            Some(row) => Ok(ReputationSnapshot {
                agent_id: ac_types::agent::AgentId(agent_id.to_string()),
                reliability: row.reliability.unwrap_or(0.0),
                reliability_confidence: row.reliability_confidence.unwrap_or(0.0),
                task_success: row.task_success.unwrap_or(0.0),
                task_success_confidence: row.task_success_confidence.unwrap_or(0.0),
                verification_accuracy: row.verification_accuracy.unwrap_or(0.0),
                verification_accuracy_confidence: row.verification_accuracy_confidence.unwrap_or(0.0),
                responsiveness: row.responsiveness.unwrap_or(0.0),
                responsiveness_confidence: row.responsiveness_confidence.unwrap_or(0.0),
                vwu_total: row.vwu_total.unwrap_or(0) as u64,
            }),
            None => Ok(ReputationSnapshot {
                agent_id: ac_types::agent::AgentId(agent_id.to_string()),
                reliability: 0.0,
                reliability_confidence: 0.0,
                task_success: 0.0,
                task_success_confidence: 0.0,
                verification_accuracy: 0.0,
                verification_accuracy_confidence: 0.0,
                responsiveness: 0.0,
                responsiveness_confidence: 0.0,
                vwu_total: 0,
            }),
        }
    }

    /// Get contribution stats for an agent.
    pub async fn get_contributions(&self, agent_id: &str) -> Result<ContributionStats, AcError> {
        let vwu_row = sqlx::query!(
            r#"
            SELECT COALESCE(COUNT(*), 0) as count
            FROM work_receipts
            WHERE agent_id = $1 AND status = 'verified'
            "#,
            agent_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        let validation_row = sqlx::query!(
            r#"
            SELECT
                COALESCE(COUNT(*), 0) as count,
                COALESCE(
                    AVG(CASE WHEN decision = 'approve' THEN 1.0 ELSE 0.0 END),
                    0.0
                ) as accuracy
            FROM validations
            WHERE validator_agent_id = $1
            "#,
            agent_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        Ok(ContributionStats {
            vwu_total: vwu_row.count,
            verified_tasks: vwu_row.count,
            validation_tasks: validation_row.count,
            validation_accuracy: validation_row.accuracy,
        })
    }

    /// Recompute and update reputation scores from all events for an agent.
    pub async fn compute_and_update_scores(&self, agent_id: &str) -> Result<(), AcError> {
        let events = sqlx::query!(
            r#"
            SELECT event_id, agent_id, event_type, task_id, dimension, value, timestamp
            FROM reputation_events
            WHERE agent_id = $1
            ORDER BY timestamp ASC
            "#,
            agent_id
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        let scorer = Scorer::default();
        let n_events = events.len() as u64;
        let confidence = scorer.confidence(n_events);

        // Compute scores per dimension
        let reliability = scorer.compute_dimension_score(
            &events.iter().map(|e| ReputationEvent {
                event_id: e.event_id,
                agent_id: ac_types::agent::AgentId(e.agent_id.clone()),
                r#type: e.event_type.clone(),
                task_id: e.task_id,
                dimension: e.dimension.parse().unwrap_or(ReputationDimension::Reliability),
                value: e.value,
                timestamp: e.timestamp,
            }).collect::<Vec<_>>(),
            ReputationDimension::Reliability,
        );

        let task_success = scorer.compute_dimension_score(
            &events.iter().map(|e| ReputationEvent {
                event_id: e.event_id,
                agent_id: ac_types::agent::AgentId(e.agent_id.clone()),
                r#type: e.event_type.clone(),
                task_id: e.task_id,
                dimension: e.dimension.parse().unwrap_or(ReputationDimension::TaskSuccess),
                value: e.value,
                timestamp: e.timestamp,
            }).collect::<Vec<_>>(),
            ReputationDimension::TaskSuccess,
        );

        let verification_accuracy = scorer.compute_dimension_score(
            &events.iter().map(|e| ReputationEvent {
                event_id: e.event_id,
                agent_id: ac_types::agent::AgentId(e.agent_id.clone()),
                r#type: e.event_type.clone(),
                task_id: e.task_id,
                dimension: e.dimension.parse().unwrap_or(ReputationDimension::VerificationAccuracy),
                value: e.value,
                timestamp: e.timestamp,
            }).collect::<Vec<_>>(),
            ReputationDimension::VerificationAccuracy,
        );

        let responsiveness = scorer.compute_dimension_score(
            &events.iter().map(|e| ReputationEvent {
                event_id: e.event_id,
                agent_id: ac_types::agent::AgentId(e.agent_id.clone()),
                r#type: e.event_type.clone(),
                task_id: e.task_id,
                dimension: e.dimension.parse().unwrap_or(ReputationDimension::Responsiveness),
                value: e.value,
                timestamp: e.timestamp,
            }).collect::<Vec<_>>(),
            ReputationDimension::Responsiveness,
        );

        // Get VWU total
        let vwu_row = sqlx::query!(
            r#"
            SELECT COALESCE(COUNT(*), 0) as count
            FROM work_receipts
            WHERE agent_id = $1 AND status = 'verified'
            "#,
            agent_id
        )
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        let vwu_total = vwu_row.count;

        // Upsert the snapshot
        sqlx::query!(
            r#"
            INSERT INTO reputation_snapshots (
                agent_id, reliability, reliability_confidence,
                task_success, task_success_confidence,
                verification_accuracy, verification_accuracy_confidence,
                responsiveness, responsiveness_confidence,
                vwu_total, snapshot_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, NOW())
            ON CONFLICT (agent_id) DO UPDATE SET
                reliability = EXCLUDED.reliability,
                reliability_confidence = EXCLUDED.reliability_confidence,
                task_success = EXCLUDED.task_success,
                task_success_confidence = EXCLUDED.task_success_confidence,
                verification_accuracy = EXCLUDED.verification_accuracy,
                verification_accuracy_confidence = EXCLUDED.verification_accuracy_confidence,
                responsiveness = EXCLUDED.responsiveness,
                responsiveness_confidence = EXCLUDED.responsiveness_confidence,
                vwu_total = EXCLUDED.vwu_total,
                snapshot_at = NOW()
            "#,
            agent_id,
            reliability,
            confidence,
            task_success,
            confidence,
            verification_accuracy,
            confidence,
            responsiveness,
            confidence,
            vwu_total
        )
        .execute(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        tracing::info!(
            agent_id,
            reliability,
            task_success,
            verification_accuracy,
            responsiveness,
            "Updated reputation scores"
        );

        Ok(())
    }
}
