use crate::scoring::Scorer;
use ac_types::agent::AgentId;
use ac_types::error::AcError;
use ac_types::reputation::ReputationSnapshot;
use chrono::Utc;
use sqlx::PgPool;
use uuid::Uuid;

/// Statistics about an agent's contribution history.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContributionStats {
    pub vwu_total: i64,
    pub verified_tasks: i64,
    pub validation_tasks: i64,
    pub validation_accuracy: f64,
}

pub struct ReputationService {
    pool: PgPool,
}

impl ReputationService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn record_event(
        &self,
        agent_id: &str,
        event_type: &str,
        task_id: Option<&str>,
        dimension: &str,
        value: f64,
    ) -> Result<(), AcError> {
        let event_id = Uuid::new_v4();
        let timestamp = Utc::now();

        sqlx::query(
            "INSERT INTO reputation_events (event_id, agent_id, event_type, task_id, dimension, value, timestamp)
             VALUES ($1, $2, $3, $4, $5, $6, $7)"
        )
        .bind(event_id)
        .bind(agent_id)
        .bind(event_type)
        .bind(task_id)
        .bind(dimension)
        .bind(value)
        .bind(timestamp)
        .execute(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        Ok(())
    }

    pub async fn get_reputation(&self, agent_id: &str) -> Result<ReputationSnapshot, AcError> {
        let row: Option<(f64, f64, f64, f64, f64, f64, f64, f64, i64)> = sqlx::query_as(
            "SELECT reliability, reliability_confidence, task_success, task_success_confidence,
                    verification_accuracy, verification_accuracy_confidence,
                    responsiveness, responsiveness_confidence, vwu_total
             FROM reputation_snapshots WHERE agent_id = $1",
        )
        .bind(agent_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        match row {
            Some((rel, rel_c, ts, ts_c, va, va_c, resp, resp_c, vwu)) => Ok(ReputationSnapshot {
                agent_id: AgentId(agent_id.to_string()),
                reliability: rel,
                reliability_confidence: rel_c,
                task_success: ts,
                task_success_confidence: ts_c,
                verification_accuracy: va,
                verification_accuracy_confidence: va_c,
                responsiveness: resp,
                responsiveness_confidence: resp_c,
                vwu_total: vwu as u64,
            }),
            None => Ok(ReputationSnapshot {
                agent_id: AgentId(agent_id.to_string()),
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

    pub async fn get_contributions(&self, agent_id: &str) -> Result<ContributionStats, AcError> {
        let vwu_total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM work_receipts WHERE agent_id = $1 AND status = 'verified'",
        )
        .bind(agent_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        let (validation_tasks, validation_accuracy): (i64, f64) = sqlx::query_as(
            "SELECT COUNT(*), COALESCE(AVG(CASE WHEN decision = 'approve' THEN 1.0 ELSE 0.0 END), 0.0)
             FROM validations WHERE validator_agent_id = $1"
        )
        .bind(agent_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        Ok(ContributionStats {
            vwu_total,
            verified_tasks: vwu_total,
            validation_tasks,
            validation_accuracy,
        })
    }

    pub async fn compute_and_update_scores(&self, agent_id: &str) -> Result<(), AcError> {
        let events: Vec<(
            Uuid,
            String,
            String,
            Option<String>,
            String,
            f64,
            chrono::DateTime<Utc>,
        )> = sqlx::query_as(
            "SELECT event_id, agent_id, event_type, task_id, dimension, value, timestamp
             FROM reputation_events WHERE agent_id = $1 ORDER BY timestamp ASC",
        )
        .bind(agent_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        let scorer = Scorer::default();
        let n_events = events.len() as u64;
        let confidence = scorer.confidence(n_events);

        let reliability = Scorer::compute_score_from_events(
            &events
                .iter()
                .filter(|(_, _, _, _, dim, _, _)| dim == "reliability")
                .map(|(_, _, _, _, _, val, _)| *val)
                .collect::<Vec<_>>(),
        );
        let task_success = Scorer::compute_score_from_events(
            &events
                .iter()
                .filter(|(_, _, _, _, dim, _, _)| dim == "task_success")
                .map(|(_, _, _, _, _, val, _)| *val)
                .collect::<Vec<_>>(),
        );
        let verification_accuracy = Scorer::compute_score_from_events(
            &events
                .iter()
                .filter(|(_, _, _, _, dim, _, _)| dim == "verification_accuracy")
                .map(|(_, _, _, _, _, val, _)| *val)
                .collect::<Vec<_>>(),
        );
        let responsiveness = Scorer::compute_score_from_events(
            &events
                .iter()
                .filter(|(_, _, _, _, dim, _, _)| dim == "responsiveness")
                .map(|(_, _, _, _, _, val, _)| *val)
                .collect::<Vec<_>>(),
        );

        let vwu_total: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM work_receipts WHERE agent_id = $1 AND status = 'verified'",
        )
        .bind(agent_id)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        sqlx::query(
            "INSERT INTO reputation_snapshots (
                agent_id, reliability, reliability_confidence,
                task_success, task_success_confidence,
                verification_accuracy, verification_accuracy_confidence,
                responsiveness, responsiveness_confidence,
                vwu_total, snapshot_at
            ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,NOW())
            ON CONFLICT (agent_id) DO UPDATE SET
                reliability=$2, reliability_confidence=$3,
                task_success=$4, task_success_confidence=$5,
                verification_accuracy=$6, verification_accuracy_confidence=$7,
                responsiveness=$8, responsiveness_confidence=$9,
                vwu_total=$10, snapshot_at=NOW()",
        )
        .bind(agent_id)
        .bind(reliability)
        .bind(confidence)
        .bind(task_success)
        .bind(confidence)
        .bind(verification_accuracy)
        .bind(confidence)
        .bind(responsiveness)
        .bind(confidence)
        .bind(vwu_total)
        .execute(&self.pool)
        .await
        .map_err(|e| AcError::Database(e.to_string()))?;

        Ok(())
    }
}
