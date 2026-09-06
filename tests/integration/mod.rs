//! Integration test suite for Agent Commons.
//!
//! Tests run against an in-process axum server with a testcontainers-managed
//! PostgreSQL database. No external server required.
//!
//! Run with: `cargo nextest run --test integration`

mod adversarial;
mod discovery;
mod harness;
mod mailbox;
mod openapi;
mod registry;
mod reputation;
mod tasks;
mod validation;
