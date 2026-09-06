//! Persistent message mailbox for asynchronous agent communication.
//!
//! Implements the mailbox protocol (§15-16): agents send messages to each
//! other's mailboxes, retrieve them when coming online, and acknowledge
//! individual messages as read.  Messages support optional expiry for
//! time-sensitive offers.

/// Axum HTTP route handlers for mailbox operations.
pub mod handler;
/// Core mailbox service with database access.
pub mod service;
