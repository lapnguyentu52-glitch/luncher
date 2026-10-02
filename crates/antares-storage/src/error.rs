use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("path escapes scoped root: {0}")]
    PathEscape(String),

    #[error("io error at {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("json error at {path}: {source}")]
    Json {
        path: String,
        #[source]
        source: serde_json::Error,
    },
}

impl StorageError {
    pub fn code(&self) -> &'static str {
        match self {
            StorageError::PathEscape(_) => "STORAGE_PATH_ESCAPED",
            StorageError::Io { .. } => "STORAGE_WRITE_FAILED",
            StorageError::Json { .. } => "CONFIG_INVALID",
        }
    }
}

pub type StorageResult<T> = Result<T, StorageError>;
