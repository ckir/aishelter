//! Shared data types for the Agent Commons protocol.
//!
//! This crate defines the core domain types used across all Agent Commons
//! components: agent identities, capability cards, mailbox messages, task
//! contracts, work receipts, and reputation records.
//!
//! All types implement [`serde::Serialize`] and [`serde::Deserialize`] for
//! JSON transport, and most also derive [`Debug`] and [`Clone`] for ergonomic
//! use throughout the codebase.

/// Agent identity and lifecycle state.
pub mod agent;
/// Capability advertisement (Agent Card).
pub mod card;
/// Application-level error types.
pub mod error;
/// Mailbox message types and envelope.
pub mod message;
/// Signed work receipts and verified work units.
pub mod receipt;
/// Reputation scores, events, and scoring dimensions.
pub mod reputation;
/// Task lifecycle states and contracts.
pub mod task;
