//! Platform-specific adapter implementations.
//!
//! Each module in this directory provides an `HttpAdapter` implementation
//! for a specific deployment target (AWS Lambda, Cloudflare Workers, etc.).
//! Modules are gated behind feature flags so that only the adapter needed
//! for the current deployment target is compiled in.

#[cfg(feature = "lambda")]
pub mod lambda;
