//! HTTP request and response types.
//!
//! Defines the [`ApiResponse<T>`] wrapper that provides a consistent JSON
//! envelope for all API endpoints, along with the [`ErrorDetail`] type for
//! structured error responses.

use serde::{Deserialize, Serialize};

/// Standard API response wrapper.
///
/// All API endpoints return JSON in this envelope:
/// ```json
/// {"data": {...}, "error": null, "protocol": "acp/1"}
/// ```
/// or on error:
/// ```json
/// {"data": null, "error": {"code": 404, "message": "..."}, "protocol": "acp/1"}
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    /// The successful response payload (present when `error` is `None`).
    pub data: Option<T>,
    /// Error details (present when `data` is `None`).
    pub error: Option<ErrorDetail>,
    /// Protocol version string, always `"acp/1"`.
    pub protocol: String,
}

impl<T: Serialize> ApiResponse<T> {
    /// Build a successful response with the given data payload.
    ///
    /// Sets `error` to `None` and `protocol` to `"acp/1"`.
    pub fn ok(data: T) -> Self {
        Self { data: Some(data), error: None, protocol: "acp/1".to_string() }
    }

    /// Build an error response with the given code and message.
    ///
    /// Sets `data` to `None` and `protocol` to `"acp/1"`.
    pub fn err(code: u16, message: String) -> Self {
        Self {
            data: None,
            error: Some(ErrorDetail { code, message }),
            protocol: "acp/1".to_string(),
        }
    }
}

/// Structured error detail returned in the [`ApiResponse`] envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetail {
    /// HTTP status code (e.g. 400, 404, 409, 500).
    pub code: u16,
    /// Human-readable error message.
    pub message: String,
}
