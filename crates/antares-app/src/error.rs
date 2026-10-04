//! §117 — Typed command errors: catalog集中 cho mọi native service + command.
//!
//! Đồng bộ hai phía (docs/protocol/README.md): TS `ErrorCodes` trong
//! `apps/desktop/src/types/protocol.ts` mirror các const ở đây — thêm code
//! mới = thêm cả 2 phía + test (test `ts_error_codes_are_all_in_rust_catalog`
//! bắt sai lệch ngay khi build).

/// Catalog §117 — `&'static str` nên map lỗi không allocate code.
pub mod codes {
    pub const APP_INTERNAL: &str = "APP_INTERNAL";
    pub const APP_NOT_READY: &str = "APP_NOT_READY";
    pub const CONFIG_INVALID: &str = "CONFIG_INVALID";
    pub const STORAGE_WRITE_FAILED: &str = "STORAGE_WRITE_FAILED";
    /// §50 — path thoát scoped root (security; Rust-side catalog).
    pub const STORAGE_PATH_ESCAPED: &str = "STORAGE_PATH_ESCAPED";
    pub const NETWORK_UNAVAILABLE: &str = "NETWORK_UNAVAILABLE";
    pub const IPC_SESSION_STALE: &str = "IPC_SESSION_STALE";
    pub const INSTANCE_LOCKED: &str = "INSTANCE_LOCKED";
    pub const JAVA_NOT_FOUND: &str = "JAVA_NOT_FOUND";
    pub const MC_VERSION_UNKNOWN: &str = "MC_VERSION_UNKNOWN";
    /// M5 — `ANTA_RUST_ONLY=1`: lệnh legacy bị cắt ở tầng command (không spawn
    /// Python subprocess); UI nhận code này để chuyển sang flow native.
    pub const LEGACY_DISABLED: &str = "LEGACY_DISABLED";
    /// Batch 05 — tên instance không hợp lệ (parity codes.INSTANCE_NAME_INVALID).
    pub const INSTANCE_NAME_INVALID: &str = "INSTANCE_NAME_INVALID";
    /// Batch 06 — tài khoản Microsoft không tồn tại/không chọn được (parity AUTH_FAILED).
    pub const AUTH_FAILED: &str = "AUTH_FAILED";
    /// Batch 07a — instance không tồn tại, chặn trước khi chạm service
    /// (parity sidecar `INSTANCE_NOT_FOUND` — preflight/launch/repair…).
    pub const INSTANCE_NOT_FOUND: &str = "INSTANCE_NOT_FOUND";
    /// Batch 06 — tham số không hợp lệ, ví dụ loader lạ (parity VALIDATION_FAILED).
    pub const VALIDATION_FAILED: &str = "VALIDATION_FAILED";
    /// §113 — process record (Rust-side catalog, Batch 07 wiring).
    pub const PROCESS_NOT_FOUND: &str = "PROCESS_NOT_FOUND";
    pub const PROCESS_STILL_RUNNING: &str = "PROCESS_STILL_RUNNING";
    pub const PROCESS_EXECUTABLE_NOT_FOUND: &str = "PROCESS_EXECUTABLE_NOT_FOUND";
    pub const PROCESS_SPAWN_FAILED: &str = "PROCESS_SPAWN_FAILED";
    /// Batch 07c — version không có trong Mojang manifest (parity
    /// `codes.MINECRAFT_VERSION_NOT_FOUND` — MLL VersionNotFound).
    pub const MINECRAFT_VERSION_NOT_FOUND: &str = "MINECRAFT_VERSION_NOT_FOUND";
    /// Batch 07c — cài loader fabric/forge thất bại (parity
    /// `codes.LOADER_INSTALL_FAILED` — legacy retry ×3).
    pub const LOADER_INSTALL_FAILED: &str = "LOADER_INSTALL_FAILED";

    /// Mọi code đang có — test mirror TS + dùng cho fallback UI.
    pub const ALL: &[&str] = &[
        APP_INTERNAL,
        APP_NOT_READY,
        CONFIG_INVALID,
        STORAGE_WRITE_FAILED,
        STORAGE_PATH_ESCAPED,
        NETWORK_UNAVAILABLE,
        IPC_SESSION_STALE,
        INSTANCE_LOCKED,
        JAVA_NOT_FOUND,
        MC_VERSION_UNKNOWN,
        LEGACY_DISABLED,
        INSTANCE_NAME_INVALID,
        AUTH_FAILED,
        INSTANCE_NOT_FOUND,
        VALIDATION_FAILED,
        PROCESS_NOT_FOUND,
        PROCESS_STILL_RUNNING,
        PROCESS_EXECUTABLE_NOT_FOUND,
        PROCESS_SPAWN_FAILED,
        MINECRAFT_VERSION_NOT_FOUND,
        LOADER_INSTALL_FAILED,
    ];

    /// Chính sách retry theo code (§117 `retryable`).
    /// true = tạm thời (I/O chập chờn, chưa sẵn sàng, network, lock bận) — thử lại được;
    /// false = khẳng định (config sai, path escape, thiếu binary) — retry vô nghĩa.
    pub fn retryable(code: &str) -> bool {
        matches!(
            code,
            APP_INTERNAL
                | APP_NOT_READY
                | NETWORK_UNAVAILABLE
                | INSTANCE_LOCKED
                | STORAGE_WRITE_FAILED
                | PROCESS_SPAWN_FAILED
                // parity legacy ForgeProvider: install fail → retry ×3.
                | LOADER_INSTALL_FAILED
        )
    }
}

/// Typed error đủ §117: code + message + retryable + action.
/// Service trả `AppResult<T>`; Tauri layer map sang `AntaresError` (envelope §96).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{code}: {message}")]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
    pub retryable: bool,
    pub action: Option<String>,
}

impl AppError {
    /// Tự chốt `retryable` theo catalog — caller không nhớ policy.
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            retryable: codes::retryable(code),
            action: None,
        }
    }

    pub fn with_action(mut self, action: impl Into<String>) -> Self {
        self.action = Some(action.into());
        self
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(codes::APP_INTERNAL, message)
    }

    pub fn not_ready(message: impl Into<String>) -> Self {
        Self::new(codes::APP_NOT_READY, message)
    }
}

/// B07b — core task supervisor error (TaskRegistry) → typed §117.
/// code() đã là catalog string; retryable theo catalog (task codes → false —
/// khớp policy core_state command layer đang hardcode false).
impl From<antares_core::CoreError> for AppError {
    fn from(err: antares_core::CoreError) -> Self {
        Self::new(err.code(), err.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;

impl From<antares_storage::StorageError> for AppError {
    fn from(err: antares_storage::StorageError) -> Self {
        // code() đã map biến thể → catalog (PathEscape→STORAGE_PATH_ESCAPED…).
        Self::new(err.code(), err.to_string())
    }
}

impl From<antares_process::ProcessError> for AppError {
    fn from(err: antares_process::ProcessError) -> Self {
        Self::new(err.code(), err.to_string())
    }
}

impl From<antares_process::SpawnError> for AppError {
    fn from(err: antares_process::SpawnError) -> Self {
        Self::new(err.code(), err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn storage_error_maps_to_catalog_codes_with_policy() {
        let escape: AppError =
            antares_storage::StorageError::PathEscape("../x".into()).into();
        assert_eq!(escape.code, codes::STORAGE_PATH_ESCAPED);
        assert!(!escape.retryable, "path escape không bao giờ retry");

        let io: AppError = antares_storage::StorageError::Io {
            path: "/tmp/x".into(),
            source: std::io::Error::new(std::io::ErrorKind::PermissionDenied, "denied"),
        }
        .into();
        assert_eq!(io.code, codes::STORAGE_WRITE_FAILED);
        assert!(io.retryable, "I/O transient được phép thử lại");

        let json: AppError = antares_storage::StorageError::Json {
            path: "/tmp/x".into(),
            source: serde_json::from_str::<u32>("{").unwrap_err(),
        }
        .into();
        assert_eq!(json.code, codes::CONFIG_INVALID);
        assert!(!json.retryable);
    }

    #[test]
    fn process_errors_map_to_catalog_codes() {
        let unknown: AppError = antares_process::ProcessError::Unknown("proc_9".into()).into();
        assert_eq!(unknown.code, codes::PROCESS_NOT_FOUND);
        assert!(!unknown.retryable);

        let spawn: AppError =
            antares_process::SpawnError::ExecutableNotFound("nope".into()).into();
        assert_eq!(spawn.code, codes::PROCESS_EXECUTABLE_NOT_FOUND);
        assert!(!spawn.retryable);
    }

    #[test]
    fn constructor_helpers_and_action() {
        let err = AppError::internal("boom").with_action("RETRY");
        assert_eq!(err.code, codes::APP_INTERNAL);
        assert!(err.retryable, "APP_INTERNAL retryable (parity legacy)");
        assert_eq!(err.action.as_deref(), Some("RETRY"));

        let not_ready = AppError::not_ready("sidecar down");
        assert_eq!(not_ready.code, codes::APP_NOT_READY);
        assert!(not_ready.retryable);
        assert_eq!(not_ready.to_string(), "APP_NOT_READY: sidecar down");
    }

    #[test]
    fn catalog_has_no_duplicates() {
        let mut seen = std::collections::HashSet::new();
        for code in codes::ALL {
            assert!(seen.insert(code), "code trùng trong catalog: {code}");
        }
        assert!(seen.len() >= 9, "ít nhất bằng số TS ErrorCodes");
    }

    /// §117 "thêm code = thêm cả 2 phía" — parse TS ErrorCodes và đối chiếu
    /// ngược vào catalog Rust (thiếu là test fail ngay).
    #[test]
    fn ts_error_codes_are_all_in_rust_catalog() {
        let ts = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../apps/desktop/src/types/protocol.ts");
        let Ok(content) = std::fs::read_to_string(&ts) else {
            return; // checkout không có apps/desktop → bỏ qua (crate dùng riêng)
        };
        let block = content
            .split("export const ErrorCodes = {")
            .nth(1)
            .and_then(|rest| rest.split("} as const").next())
            .expect("ErrorCodes block trong protocol.ts");
        let mut checked = 0;
        for line in block.lines() {
            // Dòng dạng `  AppInternal: 'APP_INTERNAL',`
            let Some(code) = line.split('\'').nth(1) else {
                continue;
            };
            assert!(
                codes::ALL.contains(&code),
                "TS ErrorCodes có `{code}` nhưng thiếu trong antares_app::error::codes"
            );
            checked += 1;
        }
        assert!(checked >= 9, "mong đợi ≥9 code TS, parse được {checked}");
    }
}
