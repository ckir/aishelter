//! Integration test module gate.
//!
//! Declares all integration test sub-modules.  Each sub-module contains tests
//! that exercise the full application stack — in-process axum router backed by
//! a testcontainers PostgreSQL instance.

mod discovery;
mod harness;
mod registry;
