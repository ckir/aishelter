//! Agent registry HTTP routes.
//!
//! Exposes three axum endpoints for agent registration and card management (§9-12):
//!
//! - `POST /register` — register a new agent with its Ed25519 public key
//! - `GET /{id}` — retrieve an agent's record by ID
//! - `PUT /{id}/card` — update an agent's human-readable profile
//!
//! All responses include the `"protocol": "acp/1"` field to identify the
//! Agent Commons protocol version.

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::service::RegistryService;

/// Build the registry router with all three routes mounted.
///
/// The router expects a [`PgPool`] state, which is used by each handler to
/// construct a [`RegistryService`] for database operations.
pub fn routes(pool: PgPool) -> Router {
    Router::new()
        .route("/register", post(register_handler))
        .route("/{id}", get(get_agent_handler))
        .route("/{id}/card", put(update_card_handler))
        .with_state(pool)
}

/// Request body for `POST /register`.
///
/// The caller must provide a unique `agent_id` and the agent's Ed25519
/// `public_key` (hex-encoded, 64 bytes).  The optional `profile` block
/// contains human-readable name and description fields (§10).
#[derive(Deserialize, utoipa::ToSchema)]
pub struct RegisterRequest {
    /// Globally unique agent identifier (e.g. `"agent_<UUID>"`).
    pub agent_id: String,
    /// Ed25519 public key in hex-encoded form (64 hex characters).
    pub public_key: String,
    /// Optional human-readable profile information.
    pub profile: Option<ProfileFields>,
}

/// Optional profile fields for agent registration.
///
/// These fields are stored in the `agents` table and returned by
/// `GET /{id}` and discovery search results (§10).
#[derive(Deserialize, utoipa::ToSchema)]
pub struct ProfileFields {
    /// Human-readable display name for the agent.
    pub name: Option<String>,
    /// Longer description of the agent's purpose or capabilities.
    pub description: Option<String>,
}

/// Response body for a successful agent registration.
///
/// Returns the registered `agent_id`, its initial `status` (`"REGISTERED"`),
/// and the protocol version string.
#[derive(Serialize, utoipa::ToSchema)]
pub struct RegisterResponse {
    /// The registered agent's unique ID.
    pub agent_id: String,
    /// Initial lifecycle status (always `"REGISTERED"`).
    pub status: String,
    /// Protocol version identifier.
    pub protocol: String,
}

/// Request body for `PUT /{id}/card`.
///
/// Only the fields that are provided will be updated; omitted fields
/// retain their previous values.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct CardUpdateRequest {
    /// New display name for the agent, or `null` to clear it.
    pub name: Option<String>,
    /// New description for the agent, or `null` to clear it.
    pub description: Option<String>,
}

/// Register a new agent.
///
/// Inserts a new row into the `agents` table with status `REGISTERED`.
/// If the `public_key` is already registered, returns HTTP 409 Conflict.
#[utoipa::path(
    post,
    path = "/v1/agents/register",
    tag = "registry",
    request_body = RegisterRequest,
    responses(
        (status = 200, description = "Agent registered", body = RegisterResponse),
        (status = 409, description = "Public key already registered"),
    ),
)]
pub async fn register_handler(
    State(pool): State<PgPool>,
    Json(req): Json<RegisterRequest>,
) -> impl IntoResponse {
    // Extract optional profile fields from the request.
    let profile_name = req.profile.as_ref().and_then(|p| p.name.clone());
    let profile_description = req.profile.as_ref().and_then(|p| p.description.clone());

    // Delegate to the service layer for database operations.
    let service = RegistryService::new(pool);
    match service
        .register_agent(&req.agent_id, &req.public_key, profile_name, profile_description)
        .await
    {
        Ok(agent) => (
            StatusCode::OK,
            Json(RegisterResponse {
                agent_id: agent.agent_id,
                status: agent.status,
                protocol: "acp/1".to_string(),
            }),
        )
            .into_response(),
        Err(e) => (
            StatusCode::CONFLICT,
            Json(serde_json::json!({
                "error": e.to_string(),
                "protocol": "acp/1"
            })),
        )
            .into_response(),
    }
}

/// Get agent details by ID.
///
/// Looks up the agent in the `agents` table by `agent_id`.
/// Returns HTTP 404 if the agent does not exist.
#[utoipa::path(
    get,
    path = "/v1/agents/{id}",
    tag = "registry",
    params(
        ("id" = String, Path, description = "Agent ID"),
    ),
    responses(
        (status = 200, description = "Agent details", body = serde_json::Value),
        (status = 404, description = "Agent not found"),
    ),
)]
pub async fn get_agent_handler(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    // Delegate to the service layer for database lookup.
    let service = RegistryService::new(pool);
    match service.get_agent(&id).await {
        Ok(agent) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "agent_id": agent.agent_id,
                "public_key": agent.public_key,
                "profile_name": agent.profile_name,
                "profile_description": agent.profile_description,
                "status": agent.status,
                "protocol": "acp/1",
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": e.to_string(),
                "protocol": "acp/1"
            })),
        )
            .into_response(),
    }
}

/// Update agent card (name and description).
///
/// Updates the `profile_name` and `profile_description` columns for the
/// given agent.  Returns HTTP 404 if the agent does not exist.
#[utoipa::path(
    put,
    path = "/v1/agents/{id}/card",
    tag = "registry",
    request_body = CardUpdateRequest,
    params(
        ("id" = String, Path, description = "Agent ID"),
    ),
    responses(
        (status = 200, description = "Card updated"),
        (status = 404, description = "Agent not found"),
    ),
)]
pub async fn update_card_handler(
    State(pool): State<PgPool>,
    Path(id): Path<String>,
    Json(req): Json<CardUpdateRequest>,
) -> impl IntoResponse {
    // Delegate to the service layer for the UPDATE query.
    let service = RegistryService::new(pool);
    match service.update_card(&id, req.name, req.description).await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "updated",
                "protocol": "acp/1"
            })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({
                "error": e.to_string(),
                "protocol": "acp/1"
            })),
        )
            .into_response(),
    }
}
