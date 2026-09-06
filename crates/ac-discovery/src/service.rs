//! Discovery service layer.
//!
//! The [`DiscoveryService`] provides capability-based agent search with
//! optional filtering by reliability score and status (§13-14).  Results
//! are joined from the `agents`, `agent_capabilities`, and
//! `reputation_snapshots` tables, ordered by reliability descending.

use ac_types::error::AcError;
use sqlx::PgPool;

/// A single result from a discovery search query.
///
/// Each result includes the agent's ID, the matched capability name and
/// version, optional profile name, and the agent's current reliability
/// score and confidence from the reputation snapshot.
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiscoveryResult {
    /// The agent's unique identifier.
    pub agent_id: String,
    /// The matched capability name.
    pub capability: String,
    /// Version string of the matched capability.
    pub capability_version: String,
    /// The agent's human-readable profile name (if set).
    pub profile_name: Option<String>,
    /// Reliability score from the reputation snapshot (0.0..1.0).
    pub reliability: Option<f64>,
    /// Confidence in the reliability score (0.0..1.0).
    pub reliability_confidence: Option<f64>,
}

/// Type alias for the raw database row tuple returned by the discovery query.
type DiscoveryRow = (String, String, String, Option<String>, Option<f64>, Option<f64>);

/// Query parameters for agent discovery search (§13).
///
/// This internal struct holds the parsed search criteria.  When `capability`
/// is `Some`, the query joins with `agent_capabilities` to filter by name.
/// When `min_reliability` is set, results below that threshold are removed
/// post-query.
#[derive(Debug, Default)]
pub struct SearchQuery {
    /// Filter by capability name.
    pub capability: Option<String>,
    /// Minimum reliability score (0.0..1.0).
    pub min_reliability: Option<f64>,
    /// Protocol filter (e.g. `"acp/1"`).
    pub protocol: Option<String>,
    /// Agent status filter (default: `"ACTIVE"`).
    pub status: Option<String>,
    /// Maximum number of results (default: 20).
    pub limit: i64,
}

/// The discovery service handles agent search and ranking.
///
/// Wraps a [`PgPool`] and provides [`search_agents`] for capability-based
/// discovery with optional reliability filtering.
pub struct DiscoveryService {
    /// The PostgreSQL connection pool.
    pool: PgPool,
}

impl DiscoveryService {
    /// Create a new [`DiscoveryService`] from an existing connection pool.
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Search for agents by capability and optional filters.
    ///
    /// When a `capability` is provided, the query joins `agents` with
    /// `agent_capabilities` and filters by capability name.  Without a
    /// capability, it returns all agents with at least one capability.
    /// Results are ordered by reliability score descending (NULLs last)
    /// and limited to the specified count.
    ///
    /// After fetching, `min_reliability` is applied as an in-memory filter
    /// to remove agents below the threshold.
    pub async fn search_agents(
        &self,
        query: &SearchQuery,
    ) -> Result<Vec<DiscoveryResult>, AcError> {
        // Use a sensible default limit if zero or negative.
        let limit = if query.limit > 0 { query.limit } else { 20 };
        // Default to ACTIVE status when no explicit status filter is given.
        let status = query.status.as_deref().unwrap_or("ACTIVE");

        // Execute the query — with or without capability filter.
        let results: Vec<DiscoveryRow> = if let Some(ref cap) = query.capability {
            // Capability-filtered query: join with agent_capabilities.
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
                  AND a.status = $2
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
            // Unfiltered query: return all agents with capabilities.
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
                WHERE a.status = $1
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

        // Map database rows to typed DiscoveryResult structs.
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

        // Apply post-query reliability threshold filter.
        if let Some(min_rel) = query.min_reliability {
            discovery_results.retain(|r| r.reliability.is_some_and(|v| v >= min_rel));
        }

        Ok(discovery_results)
    }
}
