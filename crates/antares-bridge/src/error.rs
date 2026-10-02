use thiserror::Error;

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("sidecar binary not configured or not found: {0}")]
    NotFound(String),

    #[error("failed to spawn sidecar: {source}")]
    Spawn {
        #[source]
        source: std::io::Error,
    },

    #[error("bridge not running")]
    NotRunning,

    #[error("request timeout after {timeout_ms}ms: {method}")]
    Timeout { method: String, timeout_ms: u64 },

    #[error("protocol error: {0}")]
    Protocol(String),

    #[error("io error: {source}")]
    Io {
        #[source]
        source: std::io::Error,
    },

    #[error("sidecar returned error {code}: {message}")]
    Remote { code: String, message: String },
}

impl BridgeError {
    pub fn code(&self) -> &'static str {
        match self {
            BridgeError::NotFound(_) => "IPC_SIDECAR_NOT_FOUND",
            BridgeError::Spawn { .. } => "PROCESS_SPAWN_FAILED",
            BridgeError::NotRunning => "IPC_SESSION_STALE",
            BridgeError::Timeout { .. } => "IPC_TIMEOUT",
            BridgeError::Protocol(_) => "IPC_PROTOCOL",
            BridgeError::Io { .. } => "IPC_IO",
            BridgeError::Remote { .. } => "IPC_REMOTE_ERROR",
        }
    }

    pub fn retryable(&self) -> bool {
        matches!(self, BridgeError::Timeout { .. } | BridgeError::NotRunning)
    }
}

pub type BridgeResult<T> = Result<T, BridgeError>;
