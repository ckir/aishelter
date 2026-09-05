use ac_types::error::AcError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// Convert application errors into HTTP responses.
pub fn app_error_to_response(err: AcError) -> Response {
    let (status, message) = match &err {
        AcError::AgentNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        AcError::InvalidSignature(_) => (StatusCode::UNAUTHORIZED, err.to_string()),
        AcError::DuplicateRegistration(_) => (StatusCode::CONFLICT, err.to_string()),
        AcError::TaskNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        AcError::InvalidTaskTransition { .. } => (StatusCode::BAD_REQUEST, err.to_string()),
        AcError::InsufficientValidators { .. } => (StatusCode::BAD_REQUEST, err.to_string()),
        AcError::Database(_) => {
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
        }
        AcError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    };

    (status, axum::Json(json!({ "error": message }))).into_response()
}
