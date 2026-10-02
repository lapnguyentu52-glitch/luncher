use serde::{Deserialize, Serialize};

/// §117 — Mỗi error có: code, severity, retryable, user_action, safe_message.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AntaresError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub action: Option<String>,
}

impl AntaresError {
    pub fn new(code: &str, message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: code.to_string(),
            message: message.into(),
            retryable,
            action: None,
        }
    }

    pub fn with_action(mut self, action: &str) -> Self {
        self.action = Some(action.to_string());
        self
    }
}

impl std::fmt::Display for AntaresError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}

impl std::error::Error for AntaresError {}
