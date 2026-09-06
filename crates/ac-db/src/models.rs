//! Database row types mapped via sqlx.
//!
//! This module re-exports the `FromRow` structs used by service layers to
//! read typed rows from PostgreSQL.  Each struct's field names must match
//! the column names in the database schema (§8).
//!
//! Currently the services use tuple-based `query_as` calls directly, so
//! this module serves as a placeholder for future typed row structs.

// Placeholder: typed row structs will be added here as the schema stabilises.
// Example:
//
// /// Row from the `agents` table.
// #[derive(Debug, sqlx::FromRow)]
// pub struct AgentRow {
//     /// Unique agent identifier.
//     pub agent_id: String,
//     /// Ed25519 public key in hex.
//     pub public_key: String,
//     // ... remaining columns ...
// }
