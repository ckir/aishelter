//! Application-level error types.
//!
//! [`AcError`] is the unified error enum used throughout the Agent Commons
//! codebase.  Each variant carries a human-readable message suitable for
//! logging and API responses.

use thiserror::Error;

/// Application-level errors for Agent Commons.
///
/// This enum covers the full range of expected failure modes: missing
/// entities, cryptographic failures, constraint violations, database
/// errors, and unexpected internal failures.
#[derive(Debug, Error)]
pub enum AcError {
    /// The requested agent ID does not exist in the registry.
    #[error("agent not found: {0}")]
    AgentNotFound(
        /// The agent ID that was looked up.
        String,
    ),

    /// A cryptographic signature verification failed.
    #[error("invalid signature: {0}")]
    InvalidSignature(
        /// Details about the verification failure.
        String,
    ),

    /// An agent tried to register a public key that is already in use.
    #[error("duplicate registration: {0}")]
    DuplicateRegistration(
        /// The public key that caused the conflict.
        String,
    ),

    /// The caller is authenticated but not allowed to do this.
    #[error("forbidden: {0}")]
    Forbidden(
        /// Why the action was refused.
        String,
    ),

    /// The requested message does not exist, or the caller is not a party to it.
    #[error("message not found: {0}")]
    MessageNotFound(
        /// The message ID that was looked up.
        String,
    ),

    /// The request was well-formed JSON/HTTP but carried an invalid value.
    #[error("invalid request: {0}")]
    InvalidRequest(
        /// What was wrong with the request.
        String,
    ),

    /// The requested task ID does not exist.
    #[error("task not found: {0}")]
    TaskNotFound(
        /// The task ID that was looked up.
        String,
    ),

    /// An attempt was made to transition a task to an invalid state.
    #[error("invalid task state transition: {from:?} -> {to:?}")]
    InvalidTaskTransition {
        /// The current task status string.
        from: String,
        /// The requested target status string.
        to: String,
    },

    /// Fewer validators responded than the task's required quorum.
    #[error("insufficient validators: required {required}, got {got}")]
    InsufficientValidators {
        /// The number of validators the task required.
        required: usize,
        /// The number of validators that actually responded.
        got: usize,
    },

    /// A wrapped database error from sqlx or the PostgreSQL driver.
    #[error("database error: {0}")]
    Database(
        /// The underlying database error message.
        String,
    ),

    /// An unexpected internal error occurred.
    #[error("internal error: {0}")]
    Internal(
        /// Description of the internal failure.
        String,
    ),
}

impl axum::response::IntoResponse for AcError {
    fn into_response(self) -> axum::response::Response {
        let status = match &self {
            AcError::AgentNotFound(_) | AcError::TaskNotFound(_) | AcError::MessageNotFound(_) => {
                axum::http::StatusCode::NOT_FOUND
            }
            AcError::Forbidden(_) => axum::http::StatusCode::FORBIDDEN,
            AcError::InvalidRequest(_) => axum::http::StatusCode::BAD_REQUEST,
            AcError::DuplicateRegistration(_) => axum::http::StatusCode::CONFLICT,
            AcError::InvalidTaskTransition { .. } => axum::http::StatusCode::BAD_REQUEST,
            AcError::InvalidSignature(_) => axum::http::StatusCode::UNAUTHORIZED,
            AcError::InsufficientValidators { .. } => axum::http::StatusCode::BAD_REQUEST,
            _ => axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, axum::Json(serde_json::json!({ "error": self.to_string() }))).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;
    use axum::response::IntoResponse;

    /// A failed signature is an authentication failure (401), the same status
    /// the HTTP layer in `ac-api` documents, not a generic 400.
    #[test]
    fn invalid_signature_is_unauthorized() {
        let resp = AcError::InvalidSignature("bad".into()).into_response();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }
}
