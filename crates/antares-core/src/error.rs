use thiserror::Error;

#[derive(Debug, Error)]
pub enum CoreError {
    #[error("task not found: {0}")]
    TaskNotFound(String),

    #[error("task {task_id} invalid transition {from} -> {to}")]
    InvalidTransition {
        task_id: String,
        from: &'static str,
        to: &'static str,
    },

    #[error("storage error: {0}")]
    Storage(#[from] antares_storage::StorageError),

    #[error("{code}: {message}")]
    Other { code: &'static str, message: String },
}

impl CoreError {
    pub fn code(&self) -> &'static str {
        match self {
            CoreError::TaskNotFound(_) => "TASK_NOT_FOUND",
            CoreError::InvalidTransition { .. } => "TASK_INVALID_TRANSITION",
            CoreError::Storage(err) => err.code(),
            CoreError::Other { code, .. } => code,
        }
    }

    pub fn other(code: &'static str, message: impl Into<String>) -> Self {
        CoreError::Other {
            code,
            message: message.into(),
        }
    }
}

pub type CoreResult<T> = Result<T, CoreError>;
