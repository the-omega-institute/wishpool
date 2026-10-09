use thiserror::Error;

/// Every failure an application service can report. Layer 1 maps each variant
/// to exactly one protocol status; nothing below Layer 2 invents new kinds.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("not authenticated")]
    Unauthenticated,
    #[error("forbidden: {0}")]
    Forbidden(String),
    #[error("{kind} {id} not found")]
    NotFound { kind: &'static str, id: String },
    #[error("invalid input: {0}")]
    Invalid(String),
    /// The requested transition is not allowed from the current state.
    #[error("conflict: {0}")]
    Conflict(String),
    /// Optimistic concurrency failed: the stored revision moved.
    #[error("stale revision for {kind} {id}")]
    StaleRevision { kind: &'static str, id: String },
    #[error("dependency unavailable: {0}")]
    Unavailable(String),
}

pub type CoreResult<T> = Result<T, CoreError>;

impl CoreError {
    pub fn not_found(kind: &'static str, id: impl Into<String>) -> Self {
        Self::NotFound {
            kind,
            id: id.into(),
        }
    }

    pub fn invalid(message: impl Into<String>) -> Self {
        Self::Invalid(message.into())
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self::Forbidden(message.into())
    }

    pub fn conflict(message: impl Into<String>) -> Self {
        Self::Conflict(message.into())
    }
}
