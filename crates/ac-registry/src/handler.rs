use ac_types::error::AcError;
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use axum::routing::{get, put, post};
use axum::Json;
use axum::Router;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::service::RegistryService;

#[derive(Clone)]
pub struct AppState {
    pub service: Arc<RegistryService>,
}

/// Request body for agent registration
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub agent_id: String,
    pub public_key: String,
    pub profile: Option<ProfileFields>,
}

#[derive(Debug, Deserialize)]
pub struct ProfileFields {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Response body for successful registration
#[derive(Debug, Serialize)]
pub struct RegisterResponse {
    pub agent_id: String,
    pub public_key: String,
    pub status: String,
    pub protocol: String,
}

/// Response body for agent info
#[derive(Debug, Serialize)]
pub struct AgentResponse {
    pub agent_id: String,
    pub public_key: String,
    pub profile_name: Option<String>,
    pub profile_description: Option<String>,
    pub status: String,
    pub protocol: String,
}

/// Request body for card update
#[derive(Debug, Deserialize)]
pub struct CardUpdateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Build the registry routes router
pub fn routes(service: RegistryService) -> Router {
    let state = AppState {
        service: Arc::new(service),
    };

    Router::new()
        .route("/register", post(register_handler))
        .route("/{id}", get(get_agent_handler))
        .route("/{id}/card", put(update_card_handler))
        .with_state(state)
}

/// POST /register — register a new agent
async fn register_handler(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> impl IntoResponse {
    let profile_name = req.profile.as_ref().and_then(|p| p.name.clone());
    let profile_description = req.profile.as_ref().and_then(|p| p.description.clone());

    match state
        .service
        .register_agent(&req.agent_id, &req.public_key, profile_name, profile_description)
        .await
    {
        Ok(agent) => (
            axum::http::StatusCode::OK,
            Json(RegisterResponse {
                agent_id: agent.agent_id,
                public_key: agent.public_key,
                status: agent.status,
                protocol: "acp/1".to_string(),
            }),
        )
            .into_response(),
        Err(AcError::DuplicateRegistration(_)) => (
            axum::http::StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": "public_key already registered",
                "protocol": "acp/1"
            })),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": e.to_string(),
                "protocol": "acp/1"
            })),
        )
            .into_response(),
    }
}

/// GET /{id} — get agent info
async fn get_agent_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.service.get_agent(&id).await {
        Ok(agent) => (
            axum::http::StatusCode::OK,
            Json(AgentResponse {
                agent_id: agent.agent_id,
                public_key: agent.public_key,
                profile_name: agent.profile_name,
                profile_description: agent.profile_description,
                status: agent.status,
                protocol: "acp/1".to_string(),
            }),
        )
            .into_response(),
        Err(AcError::AgentNotFound(_)) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "agent not found",
                "protocol": "acp/1"
            })),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": e.to_string(),
                "protocol": "acp/1"
            })),
        )
            .into_response(),
    }
}

/// PUT /{id}/card — update agent card
async fn update_card_handler(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<CardUpdateRequest>,
) -> impl IntoResponse {
    match state.service.update_card(&id, req.name, req.description).await {
        Ok(()) => (
            axum::http::StatusCode::OK,
            Json(serde_json::json!({
                "status": "updated",
                "protocol": "acp/1"
            })),
        )
            .into_response(),
        Err(AcError::AgentNotFound(_)) => (
            axum::http::StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": "agent not found",
                "protocol": "acp/1"
            })),
        )
            .into_response(),
        Err(e) => (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({
                "error": e.to_string(),
                "protocol": "acp/1"
            })),
        )
            .into_response(),
    }
}
