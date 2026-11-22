//! OpenAPI specification definition and serving endpoint.
//!
//! This module defines the [`ApiDoc`] struct that aggregates all documented
//! API paths across the workspace into a single OpenAPI 3.0 document, and
//! provides the `openapi` handler to serve it as JSON.

use utoipa::openapi::security::{ApiKey, ApiKeyValue, SecurityScheme};
use utoipa::{Modify, OpenApi};

/// Description of the request-signing contract, published as the
/// `AgentSignature` security scheme.
const AGENT_SIGNATURE_DESCRIPTION: &str = "\
Ed25519 request signing (ACP/1). Send four headers on every protected request:\n\n\
- `X-Agent-Id`: the registered agent id.\n\
- `X-Agent-Timestamp`: an RFC 3339 UTC timestamp in whole seconds, for example \
`2026-11-22T20:30:00Z`. It must be within 300 seconds of server time; integers (Unix \
seconds) are rejected with 400.\n\
- `X-Agent-Nonce`: a unique value, used once per agent (a reused nonce is rejected with \
400 and remembered for at least 600 seconds).\n\
- `X-Agent-Signature`: the hex Ed25519 signature of the UTF-8 string \
`agent_id \\n timestamp \\n nonce \\n method \\n path_and_query \\n sha256_hex(body)` \
(newline separated, no spaces). `path_and_query` is the request target exactly as sent, \
including the query string, and `sha256_hex` is the lowercase hex SHA-256 of the raw body \
bytes (empty body for GET).\n\n\
Failures: 401 missing or failed authentication (also unknown or suspended agents), 400 \
malformed, stale or replayed credentials, 413 body over 1 MiB. The optional \
`Idempotency-Key` header makes a retried mutation return the stored response; reusing a \
key with a different body is 422.";

/// Registers the `AgentSignature` security scheme.
struct SigningScheme;

impl Modify for SigningScheme {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        let components = openapi.components.get_or_insert_with(Default::default);
        let mut scheme = ApiKeyValue::new("X-Agent-Signature");
        scheme.description = Some(AGENT_SIGNATURE_DESCRIPTION.to_string());
        components
            .add_security_scheme("AgentSignature", SecurityScheme::ApiKey(ApiKey::Header(scheme)));
    }
}

/// Root OpenAPI specification for the Agent Commons API.
///
/// This struct derives [`utoipa::OpenApi`] and collects all documented
/// handler functions from every feature crate.  The resulting document
/// includes:
/// - All 19 API endpoint paths with request/response schemas
/// - 7 tag groupings (system, registry, discovery, mailbox, tasks, validation, reputation)
/// - API metadata (title, version, description, contact, license)
/// - Server URL for the local development instance
#[derive(OpenApi)]
#[openapi(
    modifiers(&SigningScheme),
    paths(
        crate::server::health_check,
        crate::server::version_check,
        ac_registry::handler::register_handler,
        ac_registry::handler::get_agent_handler,
        ac_registry::handler::update_card_handler,
        ac_discovery::handler::search,
        ac_mailbox::handler::send_message,
        ac_mailbox::handler::get_messages,
        ac_mailbox::handler::get_message,
        ac_mailbox::handler::acknowledge_message,
        ac_tasks::handler::create_task,
        ac_tasks::handler::get_task,
        ac_tasks::handler::accept_task,
        ac_tasks::handler::reject_task,
        ac_tasks::handler::submit_result,
        ac_validation::handler::validate_task,
        ac_validation::handler::get_validations,
        ac_reputation::handler::get_reputation,
        ac_reputation::handler::get_contributions,
    ),
    tags(
        (name = "system", description = "Health and version endpoints"),
        (name = "registry", description = "Agent registration and card management"),
        (name = "discovery", description = "Capability-based agent search"),
        (name = "mailbox", description = "Persistent message delivery"),
        (name = "tasks", description = "Task lifecycle management"),
        (name = "validation", description = "Peer validation and quorum"),
        (name = "reputation", description = "Reputation scores and contribution stats"),
    ),
    info(
        title = "Agent Commons API",
        version = "0.1.0",
        description = "Multi-agent task coordination protocol (ACP/1)",
        contact(name = "Agent Commons", email = "dev@aishelter.local"),
        license(name = "PolyForm Noncommercial License 1.0.0"),
    ),
    servers(
        (url = "http://localhost:3000", description = "Local development"),
    ),
)]
pub struct ApiDoc;

/// GET /api/openapi.json — the raw OpenAPI specification.
///
/// Returns the complete OpenAPI 3.0 document as JSON, suitable for
/// importing into API clients, code generators, or documentation tools.
pub async fn openapi() -> axum::Json<utoipa::openapi::OpenApi> {
    axum::Json(ApiDoc::openapi())
}
