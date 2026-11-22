//! Integration test suite for Agent Commons.
//!
//! Tests run against an in-process axum server with a testcontainers-managed
//! PostgreSQL database. No external server required.
//!
//! Run with: `cargo nextest run --test integration`

mod adversarial;
mod auth;
mod directory_authz;
mod discovery;
mod harness;
mod mailbox;
mod mailbox_authz;
mod middleware;
mod openapi;
mod registry;
mod registry_authz;
mod reputation;
mod tasks;
mod validation;
