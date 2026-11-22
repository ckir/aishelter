//! Authenticated caller identity.
//!
//! Lives in `ac-types` so feature crates (mailbox, registry, ...) can read the
//! verified identity without depending on the server crate that produces it.

/// The agent whose request signature verified for this request.
///
/// Inserted into request extensions by the server's authentication
/// middleware. Handlers on protected routes extract it with
/// `Extension<AuthenticatedAgent>` and must use it, never the body, the query
/// string or the `X-Agent-Id` header, as the caller's identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthenticatedAgent(
    /// The verified agent id.
    pub String,
);
