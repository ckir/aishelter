use thiserror::Error;

/// Application-level errors for Agent Commons.
#[derive(Debug, Error)]
pub enum AcError {
    #[error("agent not found: {0}")]
    AgentNotFound(String),

    #[error("invalid signature: {0}")]
    InvalidSignature(String),

    #[error("duplicate registration: {0}")]
    DuplicateRegistration(String),

    #[error("task not found: {0}")]
    TaskNotFound(String),

    #[error("invalid task state transition: {from:?} -> {to:?}")]
    InvalidTaskTransition { from: String, to: String },

    #[error("insufficient validators: required {required}, got {got}")]
    InsufficientValidators { required: usize, got: usize },

    #[error("database error: {0}")]
    Database(String),

    #[error("internal error: {0}")]
    Internal(String),
}
