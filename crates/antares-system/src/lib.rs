//! antares-system — B15.1a, parity `services/system/` (mục 4.1–4.3 Disk/Power/Process).
//!
//! Sơ đồ tương đương legacy (SystemOptimizationService gộp 3 module con):
//!
//! ```text
//! SystemOptimizationService      crates/antares-system
//! ├── DiskCleaner        →  cleaner.rs  DiskCleaner (SAFE_RULES/PROTECTED/trash+manifest)
//! ├── PowerService       →  power.rs    PowerService (powercfg parity, hint-first)
//! └── ProcessService     →  (không port — antares-process §113 đã bọc process)
//! ```
//!
//! ## DiskCleaner — mục 4.3 Disk + 37 (Safe-first, có Undo)
//!
//! - Chỉ quét/đụng thư mục **launcher sở hữu** (`_owned` chống path escape — mục 76)
//! - `saves/screenshots/mods/resourcepacks/accounts` = PROTECTED — chỉ hiển thị kích
//!   thước, không có action xoá
//! - Clean KHÔNG delete vĩnh viễn: move vào `<data>/trash/<cleanId>/` + manifest →
//!   undo 1 click; xoá hẳn là action riêng có confirm (empty_trash)
//! - 4 SAFE_RULES parity: download_parts (*.part, 1d), old_logs (7d),
//!   old_snapshots (opt-*.json, 30d), trash (7d)
//!
//! ## PowerService — mục 4.3 Power + 35 (hint-first, không tự sửa)
//!
//! - Đọc plan hiện tại qua `powercfg /getactivescheme` (Windows) — read-only;
//!   OS khác → `{supported: false, plan: None, recommendation: None}` (không fail app)
//! - Đề xuất High performance khi đang Power Saver — chỉ HINT; đổi plan là action
//!   user bấm (requiresAdmin), KHÔNG tự áp (mục 74)
//! - set_plan: plan lạ → `Ok(false)` — handler bên trên đổi thành CONFIG_INVALID
//!   (parity BridgeMethodError)

mod cleaner;
mod power;

pub use cleaner::{CleanupGroup, CleanupItem, DiskCleaner, ProtectedDir, ScanReport};
pub use power::{PowerPlan, PowerPlanId, PowerService, PowerStatus, PowerRecommendation};

use serde::Serialize;

/// Data root của launcher — mọi path clean/scan phải nằm dưới đây (mục 76).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanResult {
    pub clean_id: String,
    pub moved: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UndoResult {
    pub restored: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SystemError {
    #[error("refusing to clean outside launcher data: {0}")]
    OutsideData(String),
    #[error("cleanup manifest not found: {0}")]
    ManifestNotFound(String),
    #[error("io: {0}")]
    Io(String),
}

impl SystemError {
    /// Code taxonomy §117 — parity `codes.VALIDATION_FAILED` / `codes.FILE_NOT_FOUND`.
    pub fn code(&self) -> &'static str {
        match self {
            SystemError::OutsideData(_) => "VALIDATION_FAILED",
            SystemError::ManifestNotFound(_) => "FILE_NOT_FOUND",
            SystemError::Io(_) => "APP_INTERNAL",
        }
    }
}

/// Atomic write helper dùng chung (mục 62 — manifest.json không để lại rác).
pub(crate) fn write_json_atomic(path: &std::path::Path, value: &serde_json::Value) -> Result<(), SystemError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|err| SystemError::Io(err.to_string()))?;
    }
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, value.to_string()).map_err(|err| SystemError::Io(err.to_string()))?;
    std::fs::rename(&tmp, path).map_err(|err| SystemError::Io(err.to_string()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_parity() {
        assert_eq!(
            SystemError::OutsideData("/etc".into()).code(),
            "VALIDATION_FAILED"
        );
        assert_eq!(
            SystemError::ManifestNotFound("trash/abc".into()).code(),
            "FILE_NOT_FOUND"
        );
    }

    #[test]
    fn write_json_atomic_no_tmp_leftover() {
        let root = std::env::temp_dir().join(format!("antares-sys-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let path = root.join("sub/manifest.json");
        write_json_atomic(&path, &serde_json::json!({"id": "x"})).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            r#"{"id":"x"}"#.to_string()
        );
        assert!(!root.join("sub/manifest.json.tmp").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
