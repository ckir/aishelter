use ac_types::error::AcError;
use sqlx::PgPool;

/// A single result from a discovery search query.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiscoveryResult {
    pub agent_id: String,
    pub capability: String,
    pub capability_version: String,
    pub profile_name: Option<String>,
    pub reliability: Option<f64>,
    pub reliability_confidence: Option<f64>,
}

type DiscoveryRow = (String, String, String, Option<String>, Option<f64>, Option<f64>);

/// Query parameters for agent discovery search (§13).
#[derive(Debug, Default)]
pub struct SearchQuery {
    pub capability: Option<String>,
    pub min_reliability: Option<f64>,
    pub protocol: Option<String>,
    pub status: Option<String>,
    pub limit: i64,
}

/// The discovery service handles agent search and ranking.
pub struct DiscoveryService {
    pool: PgPool,
}

impl DiscoveryService {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Search for agents by capability and optional filters.
    pub async fn search_agents(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<DiscoveryResult>, AcError> {
        let limit = if query.limit > 0 { query.limit } else { 20 };
        let status = query.status.as_deref().unwrap_or("ACTIVE");

        let results: Vec<DiscoveryRow> = if let Some(ref cap) = query.capability {
            sqlx::query_as(
                r#"
                SELECT
                    a.agent_id,
                    ac.capability_name,
                    ac.capability_version,
                    a.profile_name,
                    rs.reliability,
                    rs.reliability_confidence
                FROM agents a
                JOIN agent_capabilities ac ON a.agent_id = ac.agent_id
                LEFT JOIN reputation_snapshots rs ON a.agent_id = rs.agent_id
                WHERE ac.capability_name = $1
                  AND a.status = ANY($2)
                ORDER BY rs.reliability DESC NULLS LAST
                LIMIT $3
                "#,
            )
            .bind(cap)
            .bind(status)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AcError::Database(e.to_string()))?
        } else {
            sqlx::query_as(
                r#"
                SELECT
                    a.agent_id,
                    ac.capability_name,
                    ac.capability_version,
                    a.profile_name,
                    rs.reliability,
                    rs.reliability_confidence
                FROM agents a
                JOIN agent_capabilities ac ON a.agent_id = ac.agent_id
                LEFT JOIN reputation_snapshots rs ON a.agent_id = rs.agent_id
                WHERE a.status = ANY($1)
                ORDER BY rs.reliability DESC NULLS LAST
                LIMIT $2
                "#,
            )
            .bind(status)
            .bind(limit)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| AcError::Database(e.to_string()))?
        };

        let mut discovery_results: Vec<DiscoveryResult> = results
            .into_iter()
            .map(|(agent_id, cap_name, cap_ver, profile, reliability, rel_conf)| {
                DiscoveryResult {
                    agent_id,
                    capability: cap_name,
                    capability_version: cap_ver,
                    profile_name: profile,
                    reliability,
                    reliability_confidence: rel_conf,
                }
            })
            .collect();

        if let Some(min_rel) = query.min_reliability {
            discovery_results
                .retain(|r| r.reliability.is_some_and(|v| v >= min_rel));
        }

        Ok(discovery_results)
    }
}
