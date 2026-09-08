use crate::service::{DirectoryService, ServiceError};
use ac_db::pool::SharedPool;
use ac_types::error::AcError;
use axum::{
    Json, Router,
    extract::{Path, State},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tracing::error;

/// Build the services router.
pub fn routes(pool: SharedPool) -> Router {
    Router::new()
        .route("/v1/services/register", post(register_service))
        .route("/v1/services/{id}", get(get_service))
        .route("/v1/services/{id}/refresh", post(refresh_service))
        .route("/v1/services/{id}/health", get(get_service_health))
        .with_state(pool)
}

#[derive(Deserialize)]
pub struct ServiceRegistrationRequest {
    pub manifest_url: String,
}

#[derive(Serialize)]
pub struct ServiceRegistrationResponse {
    pub service_id: String,
    pub status: String,
}

pub async fn register_service(
    State(pool): State<SharedPool>,
    Json(req): Json<ServiceRegistrationRequest>,
) -> Result<Json<ServiceRegistrationResponse>, AcError> {
    let service = DirectoryService::new(pool);
    let (service_id, status) = service.register_service(&req.manifest_url).await?;

    Ok(Json(ServiceRegistrationResponse { service_id, status }))
}

pub async fn get_service(
    State(pool): State<SharedPool>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AcError> {
    let service = DirectoryService::new(pool);
    let row = service.get_service(&id).await?;

    // Return the manifest JSON directly
    Ok(Json(row.manifest_json))
}

pub async fn refresh_service(
    State(pool): State<SharedPool>,
    Path(id): Path<String>,
) -> Result<axum::http::StatusCode, AcError> {
    let service = DirectoryService::new(pool);
    service.refresh_service(&id).await?;
    Ok(axum::http::StatusCode::ACCEPTED)
}

pub async fn get_service_health(
    State(pool): State<SharedPool>,
    Path(id): Path<String>,
) -> Result<Json<crate::service::ServiceHealth>, AcError> {
    let service = DirectoryService::new(pool);
    let health = service.get_service_health(&id).await?;
    Ok(Json(health))
}
