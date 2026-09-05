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

        // Build the base query with optional filters.
        let mut sql = r#"
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
            WHERE 1=1
        "#
        .to_string();

        if let Some(ref _cap) = query.capability {
            sql.push_str(" AND ac.capability_name = $1");
        }
        if let Some(ref _status) = query.status {
            sql.push_str(" AND a.status = $");
            sql.push_str(&(if query.capability.is_some() { "2" } else { "1" }).to_string());
        }
        if let Some(_min_rel) = query.min_reliability {
            sql.push_str(" AND (rs.reliability IS NULL OR rs.reliability >= $");
            // Determine the next positional parameter index.
            let idx = if query.capability.is_some() { 3 } else { 2 };
            sql.push_str(&idx.to_string());
            sql.push_str(&format!(")",));
            // We'll use a simpler approach below with sqlx query builders.
        }

        sql.push_str(&format!(" ORDER BY rs.reliability DESC NULLS LAST LIMIT {}", limit));

        // Use a simpler approach: build separate queries per filter combination.
        // For v0.1, we use a direct sqlx query with optional filtering done in Rust
        // for flexibility, and let PostgreSQL handle the heavy lifting for the capability filter.

        let results: Vec<(String, String, String, Option<String>, Option<f64>, Option<f64>)> =
            if let Some(ref cap) = query.capability {
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
                .bind(&query.status.as_deref().unwrap_or("ACTIVE"))
                .bind(limit)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| AcError::Database(e.to_string()))?
            } else {
                // No capability filter — return all discoverable agents.
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
                .bind(&query.status.as_deref().unwrap_or("ACTIVE"))
                .bind(limit)
                .fetch_all(&self.pool)
                .await
                .map_err(|e| AcError::Database(e.to_string()))?
            };

        let mut discovery_results: Vec<DiscoveryResult> = results
            .into_iter()
            .map(|(agent_id, cap_name, cap_ver, profile, reliability, rel_conf)| DiscoveryResult {
                agent_id,
                capability: cap_name,
                capability_version: cap_ver,
                profile_name: profile,
                reliability,
                reliability_confidence: rel_conf,
            })
            .collect();

        // Apply min_reliability filter in Rust (LEFT JOIN means some agents have NULL).
        if let Some(min_rel) = query.min_reliability {
            discovery_results.retain(|r| r.reliability.map_or(false, |v| v >= min_rel));
        }

        Ok(discovery_results)
    }
}
