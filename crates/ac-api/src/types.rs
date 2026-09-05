use serde::{Deserialize, Serialize};

/// Standard API response wrapper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub data: Option<T>,
    pub error: Option<ErrorDetail>,
    pub protocol: String,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn ok(data: T) -> Self {
        Self {
            data: Some(data),
            error: None,
            protocol: "acp/1".to_string(),
        }
    }

    pub fn err(code: u16, message: String) -> Self {
        Self {
            data: None,
            error: Some(ErrorDetail { code, message }),
            protocol: "acp/1".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorDetail {
    pub code: u16,
    pub message: String,
}
