//! HTTP error response conversion.
//!
//! Provides [`app_error_to_response`], which maps [`AcError`] variants to
//! appropriate HTTP status codes and JSON error bodies.

use ac_types::error::AcError;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// Convert an application-level error into an HTTP response.
///
/// Each [`AcError`] variant is mapped to a specific [`StatusCode`]:
///
/// | Variant                      | Status       |
/// |------------------------------|--------------|
/// | [`AgentNotFound`]            | 404          |
/// | [`InvalidSignature`]         | 401          |
/// | [`DuplicateRegistration`]    | 409          |
/// | [`TaskNotFound`]             | 404          |
/// | [`InvalidTaskTransition`]    | 400          |
/// | [`InsufficientValidators`]   | 400          |
/// | [`Database`]                 | 500          |
/// | [`Internal`]                 | 500          |
///
/// The response body is a JSON object with a single `"error"` key.
///
/// [`AgentNotFound`]: AcError::AgentNotFound
/// [`InvalidSignature`]: AcError::InvalidSignature
/// [`DuplicateRegistration`]: AcError::DuplicateRegistration
/// [`TaskNotFound`]: AcError::TaskNotFound
/// [`InvalidTaskTransition`]: AcError::InvalidTaskTransition
/// [`InsufficientValidators`]: AcError::InsufficientValidators
/// [`Database`]: AcError::Database
/// [`Internal`]: AcError::Internal
pub fn app_error_to_response(err: AcError) -> Response {
    // Map each error variant to an HTTP status code.
    let (status, message) = match &err {
        AcError::AgentNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        AcError::InvalidSignature(_) => (StatusCode::UNAUTHORIZED, err.to_string()),
        AcError::DuplicateRegistration(_) => (StatusCode::CONFLICT, err.to_string()),
        AcError::TaskNotFound(_) => (StatusCode::NOT_FOUND, err.to_string()),
        AcError::InvalidTaskTransition { .. } => (StatusCode::BAD_REQUEST, err.to_string()),
        AcError::InsufficientValidators { .. } => (StatusCode::BAD_REQUEST, err.to_string()),
        AcError::Database(_) => {
            // Hide internal database details from the client.
            (StatusCode::INTERNAL_SERVER_ERROR, "Internal server error".to_string())
        }
        AcError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()),
    };

    // Build the JSON response.
    (status, axum::Json(json!({ "error": message }))).into_response()
}
