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

pub fn routes(pool: PgPool) -> Router {
    Router::new()
        .route("/register", post(register_handler))
        .route("/{id}", get(get_agent_handler))
        .route("/{id}/card", put(update_card_handler))
        .with_state(pool)
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RegisterRequest {
    pub agent_id: String,
    pub public_key: String,
    pub profile: Option<ProfileFields>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ProfileFields {
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct RegisterResponse {
    pub agent_id: String,
    pub status: String,
    pub protocol: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CardUpdateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
}

/// Register a new agent.
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
    let service = RegistryService::new(pool);
    let profile_name = req.profile.as_ref().and_then(|p| p.name.clone());
    let profile_description = req.profile.as_ref().and_then(|p| p.description.clone());

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
