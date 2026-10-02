//! antares-diagnostics — B15.1b, parity `services/diagnostics/log_analyzer.py` +
//! `services/repair/service.py` (§41 insights, §39 repair, evidence export).
//!
//! ## LogAnalyzer (mục 41 — KHÔNG LLM, mục 24)
//!
//! - 3 nguồn log: `launcher` (ring buffer — caller inject snapshot), `minecraft`
//!   (`<inst>/game/logs/latest.log`), `crash` (`<inst>/game/crash-reports/*.txt`,
//!   3 file mới nhất theo mtime)
//! - 11 blueprint pattern → id/seed/recommend/action — match từng dòng, gộp insights:
//!   count/severity/seed/firstSeen/lastSeen/sources[]/lines[] (tối đa 5, cắt 240 ký tự)
//! - severity (parity `_severity` legacy): error = oom/java_class_version/
//!   java_missing/mod_conflict/disk_full/gpu_driver/asset_corrupt/version_json_bad ·
//!   warning = auth_failed/connection_refused/connection_timeout
//! - sort: count giảm dần, rồi severity rank (error trước)
//! - read() cho console ảo hoá (mục 16): limit + truncated, text cắt 400
//!
//! ## RepairService (§39 — dry-run first)
//!
//! - 7 ACTIONS: instance_metadata/missing_dirs/option_files/launcher_config/
//!   downloads/caches/resource_packs
//! - scan() → findings + planned, KHÔNG ghi gì; run() thực thi planned:
//!   mkdir/write_json/rewrite_lines/trash (mọi file đổi qua trash — undo được)
//! - resource_packs: ZIP test parity "pack.mcmeta missing"/"corrupt member"

mod analyzer;
mod repair;

pub use analyzer::{
    analyze, analyzer_read_source, find_blueprint, Blueprint, Insight, LogAnalysis, BLUEPRINT_COUNT,
};
pub use repair::{
    plan_option_files_dedupe, RepairAction, RepairError, RepairFinding, RepairPlannedStep,
    RepairResult, RepairScan, RepairService, ACTIONS, REQUIRED_DIRS,
};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DiagnosticsError {
    #[error("unknown log source: {0}")]
    UnknownSource(String),
    #[error("unknown repair action: {0}")]
    UnknownAction(String),
    #[error("refusing to trash outside data: {0}")]
    OutsideData(String),
    #[error("io: {0}")]
    Io(String),
}

impl DiagnosticsError {
    /// Code taxonomy §117 — parity codes.VALIDATION_FAILED / APP_INTERNAL.
    pub fn code(&self) -> &'static str {
        match self {
            DiagnosticsError::UnknownSource(_) | DiagnosticsError::UnknownAction(_) => {
                "VALIDATION_FAILED"
            }
            DiagnosticsError::OutsideData(_) => "VALIDATION_FAILED",
            DiagnosticsError::Io(_) => "APP_INTERNAL",
        }
    }
}

/// Paths cấu trúc launcher — caller (Tauri command) map từ scoped roots.
#[derive(Debug, Clone)]
pub struct DiagPaths {
    /// `<root>/data/instances` (parity app/context.py: `data / "instances"`)
    pub instances: std::path::PathBuf,
    /// `<root>/data` (trash + instances)
    pub data: std::path::PathBuf,
    /// `<root>/cache` (downloads + manifests)
    pub cache: std::path::PathBuf,
    /// `<root>/app-data` (settings.json)
    pub config: std::path::PathBuf,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceInfo {
    pub id: &'static str,
    pub available: bool,
    pub bytes: u64,
    pub lines: usize,
    /// Chỉ nguồn crash có count (parity legacy đặt count khi crash reports).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
}

/// `console.sources` parity — thông tin 3 nguồn log.
pub fn sources(paths: &DiagPaths, instance_id: Option<&str>) -> Vec<SourceInfo> {
    let mut out = Vec::new();
    // launcher: ring buffer — caller inject; ở mức crate luôn unavailable.
    out.push(SourceInfo {
        id: "launcher",
        available: false,
        bytes: 0,
        lines: 0,
        count: None,
    });
    // minecraft: latest.log
    let mc_path = minecraft_log(paths, instance_id);
    if let Some(path) = mc_path.filter(|p| p.is_file()) {
        let bytes = path.metadata().map(|m| m.len()).unwrap_or(0);
        let lines = std::fs::read_to_string(&path)
            .map(|t| t.lines().count())
            .unwrap_or(0);
        out.push(SourceInfo {
            id: "minecraft",
            available: true,
            bytes,
            lines,
            count: None,
        });
    } else {
        out.push(SourceInfo {
            id: "minecraft",
            available: false,
            bytes: 0,
            lines: 0,
            count: None,
        });
    }
    // crash: crash-reports/*.txt
    let reports = crash_reports(paths, instance_id);
    if reports.is_empty() {
        out.push(SourceInfo {
            id: "crash",
            available: false,
            bytes: 0,
            lines: 0,
            count: None,
        });
    } else {
        let bytes = reports
            .iter()
            .map(|p| p.metadata().map(|m| m.len()).unwrap_or(0))
            .sum();
        out.push(SourceInfo {
            id: "crash",
            available: true,
            bytes,
            lines: 0,
            count: Some(reports.len()),
        });
    }
    out
}

pub(crate) fn minecraft_log(paths: &DiagPaths, instance_id: Option<&str>) -> Option<std::path::PathBuf> {
    let iid = instance_id?;
    Some(
        paths
            .instances
            .join(iid)
            .join("game")
            .join("logs")
            .join("latest.log"),
    )
}

pub(crate) fn crash_reports(paths: &DiagPaths, instance_id: Option<&str>) -> Vec<std::path::PathBuf> {
    let Some(iid) = instance_id else {
        return Vec::new();
    };
    let dir = paths.instances.join(iid).join("game").join("crash-reports");
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut files: Vec<(std::time::SystemTime, std::path::PathBuf)> = Vec::new();
    for entry in std::fs::read_dir(&dir).into_iter().flatten() {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if path.is_file() && path.extension().is_some_and(|ext| ext == "txt") {
            let mtime = path
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            files.push((mtime, path));
        }
    }
    // 3 file mới nhất (sort mtime giảm dần)
    files.sort_by(|a, b| b.0.cmp(&a.0));
    files.into_iter().take(3).map(|(_, p)| p).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("antares-diag-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn paths(root: &std::path::Path) -> DiagPaths {
        DiagPaths {
            instances: root.join("instances"),
            data: root.join("data"),
            cache: root.join("cache"),
            config: root.join("app-data"),
        }
    }

    #[test]
    fn sources_shape_parity() {
        let root = temp_root("sources");
        let p = paths(&root);
        // Chưa có gì: cả 3 unavailable
        let info = sources(&p, None);
        assert_eq!(info.len(), 3);
        assert_eq!(info[0].id, "launcher");
        assert!(info.iter().all(|i| !i.available));

        // minecraft log có → available + bytes/lines
        let log = p.instances.join("i1/game/logs/latest.log");
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        std::fs::write(&log, "line1\nline2\nline3\n").unwrap();
        let info = sources(&p, Some("i1"));
        let mc = info.iter().find(|i| i.id == "minecraft").unwrap();
        assert!(mc.available);
        assert_eq!(mc.lines, 3);
        assert!(mc.bytes > 0);

        // crash reports: chỉ đếm .txt, tối đa 3
        let cr = p.instances.join("i1/game/crash-reports");
        std::fs::create_dir_all(&cr).unwrap();
        for i in 0..5 {
            std::fs::write(cr.join(format!("crash-{i}.txt")), b"boom").unwrap();
        }
        std::fs::write(cr.join("notes.md"), b"not txt").unwrap();
        let info = sources(&p, Some("i1"));
        let crash = info.iter().find(|i| i.id == "crash").unwrap();
        assert!(crash.available);
        assert_eq!(crash.count, Some(3), "chỉ lấy 3 report mới nhất");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn error_codes_parity() {
        assert_eq!(
            DiagnosticsError::UnknownSource("x".into()).code(),
            "VALIDATION_FAILED"
        );
        assert_eq!(
            DiagnosticsError::UnknownAction("x".into()).code(),
            "VALIDATION_FAILED"
        );
        assert_eq!(
            DiagnosticsError::OutsideData("/etc".into()).code(),
            "VALIDATION_FAILED"
        );
    }
}
