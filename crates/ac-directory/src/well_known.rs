//! Handlers for `/.well-known/` discovery resources.
//!
//! These endpoints serve machine-readable manifest documents that
//! autonomous agents use to discover and interact with this service.
//! The JSON structures match the schemas defined in the Agent Commons
//! v1.0 specification, and the HTTP paths match the well-known URI
//! pattern used across the web.

use ac_db::pool::SharedPool;
use axum::{Json, Router, extract::State, routing::get};
use serde::Serialize;

/// Build the well-known routes router.
pub fn routes(pool: SharedPool) -> Router {
    Router::new()
        .route("/.well-known/agent-service.json", get(agent_service_handler))
        .route("/.well-known/agent-directory.json", get(agent_directory_handler))
        .route("/.well-known/agent.json", get(agent_card_handler))
        .with_state(pool)
}

#[derive(Serialize)]
struct AgentServiceResponse {
    schema_version: String,
    id: String,
    name_for_human: String,
    name_for_model: String,
    description_for_human: String,
    description_for_model: String,
    protocol: String,
    version: String,
    api: ApiInfo,
    capabilities: Vec<serde_json::Value>,
}

#[derive(Serialize)]
struct ApiInfo {
    #[serde(rename = "type")]
    api_type: String,
    url: String,
}

/// GET `/.well-known/agent-service.json`.
///
/// Returns the Agent Service Manifest describing this Agent Commons
/// installation.
pub async fn agent_service_handler(_state: State<SharedPool>) -> Json<AgentServiceResponse> {
    Json(AgentServiceResponse {
        schema_version: "agent-service/v1".to_string(),
        id: "https://localhost/.well-known/agent-service.json".to_string(),
        name_for_human: "Agent Commons".to_string(),
        name_for_model: "agent_commons".to_string(),
        description_for_human: "Agent Commons service directory for agent discovery and registration".to_string(),
        description_for_model: "This is the Agent Commons control plane. Use it to register agents, search for agents by capability, create tasks, send messages, and validate task results. Protocol: acp/1.".to_string(),
        protocol: "acp/1".to_string(),
        version: "1.0".to_string(),
        api: ApiInfo {
            api_type: "openapi".to_string(),
            url: "/api/openapi.json".to_string(),
        },
        capabilities: vec![],
    })
}

#[derive(Serialize)]
struct AgentDirectoryResponse {
    schema_version: String,
    id: String,
    name: String,
    description: String,
    protocols: Vec<String>,
    search: SearchInfo,
    registration: RegistrationInfo,
}

#[derive(Serialize)]
struct SearchInfo {
    endpoint: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    semantic: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reputation: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    protocols: Option<bool>,
}

#[derive(Serialize)]
struct RegistrationInfo {
    endpoint: String,
}

/// GET `/.well-known/agent-directory.json`.
///
/// Returns the Agent Directory Manifest describing this registry.
pub async fn agent_directory_handler(_state: State<SharedPool>) -> Json<AgentDirectoryResponse> {
    Json(AgentDirectoryResponse {
        schema_version: "agent-directory/v1".to_string(),
        id: "https://localhost/.well-known/agent-directory.json".to_string(),
        name: "Agent Commons Directory".to_string(),
        description: "Agent Commons service directory for agent discovery and registration".to_string(),
        protocols: vec!["acp/1".to_string()],
        search: SearchInfo {
            endpoint: "/v1/discovery/search".to_string(),
            semantic: Some(false),
            reputation: Some(true),
            protocols: Some(true),
        },
        registration: RegistrationInfo {
            endpoint: "/v1/agents/register".to_string(),
        },
    })
}

#[derive(Serialize)]
struct AgentCardResponse {
    schema_version: String,
    agent_id: String,
    name: String,
    description: String,
    version: String,
    capabilities: Vec<serde_json::Value>,
    protocols: Vec<String>,
    service_url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    directory_url: Option<String>,
}

/// GET `/.well-known/agent.json`.
///
/// Returns the Agent Card for this Agent Commons installation itself.
///
/// This is a placeholder response.  In a production deployment the values
/// would be read from the agent's registration record or configuration.
pub async fn agent_card_handler(_state: State<SharedPool>) -> Json<AgentCardResponse> {
    Json(AgentCardResponse {
        schema_version: "agent/v1".to_string(),
        agent_id: "agent_local".to_string(),
        name: "Agent Commons".to_string(),
        description: "Agent Commons service directory for agent discovery and registration".to_string(),
        version: "1.0".to_string(),
        capabilities: vec![],
        protocols: vec!["acp/1".to_string()],
        service_url: "/.well-known/agent-service.json".to_string(),
        directory_url: Some("/.well-known/agent-directory.json".to_string()),
    })
}
