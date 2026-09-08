//! A2A (Agent-to-Agent) protocol adapter for Agent Commons v1.0.
//!
//! Translates incoming A2A requests into ACP/1 internal calls and
//! translates responses back. A2A adapters MUST NOT become the canonical
//! storage model — ACP/1 + HTTP/OpenAPI remain canonical.

pub mod handler;

pub use handler::a2a_router;
