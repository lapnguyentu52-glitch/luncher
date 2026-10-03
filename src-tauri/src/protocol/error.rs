use serde::{Deserialize, Serialize};

/// §117 — catalog集中 (mirror TS `ErrorCodes` trong types/protocol.ts):
/// nguồn là `antares-app::error::codes` (composition root), re-export đây để
/// giữ contract docs/protocol/README.md — `codes::*` nằm ở `protocol/error.rs`.
pub use antares_app::error::codes;

impl AntaresError {
    /// §117 `APP_INTERNAL` — lỗi tầng Tauri (join/spawn_blocking) không thuộc
    /// service nào; dùng catalog thay vì hardcode string ở call site.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(
            codes::APP_INTERNAL,
            message,
            codes::retryable(codes::APP_INTERNAL),
        )
    }
}

impl From<antares_app::AppError> for AntaresError {
    /// Typed service error (§117) → envelope §96 — giữ nguyên
    /// code/message/retryable/action, không hardcode string ở call site.
    fn from(err: antares_app::AppError) -> Self {
        let mut mapped = AntaresError::new(err.code, err.message, err.retryable);
        if let Some(action) = err.action {
            mapped = mapped.with_action(&action);
        }
        mapped
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use antares_app::AppError;

    #[test]
    fn app_error_maps_to_envelope_error() {
        let typed = AppError::new(codes::INSTANCE_LOCKED, "instance running").with_action("OPEN_INSTANCE");
        let mapped = AntaresError::from(typed);
        assert_eq!(mapped.code, "INSTANCE_LOCKED");
        assert_eq!(mapped.message, "instance running");
        assert!(mapped.retryable);
        assert_eq!(mapped.action.as_deref(), Some("OPEN_INSTANCE"));
    }

    #[test]
    fn app_internal_helper_maps_retryable_true() {
        let mapped = AntaresError::from(AppError::internal("join failed"));
        assert_eq!(mapped.code, codes::APP_INTERNAL);
        assert!(mapped.retryable);
    }
}
