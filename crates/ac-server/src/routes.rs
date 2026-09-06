//! OpenAPI specification definition and serving endpoint.
//!
//! This module defines the [`ApiDoc`] struct that aggregates all documented
//! API paths across the workspace into a single OpenAPI 3.0 document, and
//! provides the `openapi` handler to serve it as JSON.

use utoipa::OpenApi;

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
