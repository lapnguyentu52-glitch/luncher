//! antares-mods — B15.1d, parity `services/mods/` (mục 11 Mod Manager).
//!
//! Phase A (đã làm — khung + health/quarantine):
//! - `deflate` — RFC 1951 decompressor thuần (stored/fixed/dynamic + LZ77) — golden
//!   vectors từ Python `zlib` (payload thật + zip-bomb guard)
//! - `zipread` — ZIP reader: central directory + method 0/8 + entry cap 64MB
//!   (zip-bomb) — đọc metadata jar không phụ thuộc crate ngoài
//! - `health` — ModInfo + `read_mod_info` (fabric/quilt/mods.toml/neoforge/
//!   mcmod.info) + `check_health` (unreadable/wrong_loader/missing_dependency/
//!   broken_dependency) + `version_newer` semantic-ish
//! - `quarantine` — vault `<cache>/quarantine/` + manifest atomic (quarantine/
//!   list/restore/delete) — mục 368 safe-disable (move, không delete)
//! - `list_installed`/`uninstall` parity `ModService` (sorted *.jar / unlink)
//!
//! Phase B (B15.1d tiếp theo — ĐÃ LÀM): `jar_reader` decode class file JVMS thuần
//! (constant pool + Code attribute walk + BootstrapMethods) + `scanner` 10 rules
//! heuristic (download_exec/reverse_shell/credential/ransomware/registry/obfuscation/
//! network_beacon/invokedynamic custom-bootstrap + abuse/callgraph BFS depth-4/
//! reflection_abuse) với scoring SAFE/SUSPICIOUS/DANGEROUS 25/60.
//! Phase C (B15.1d — ĐÃ LÀM): `modrinth` search/versions URL builder + pick_file
//! + fetch qua antares-downloads (https seam) + `build_fix_plan` parity KNOWN_DEPS;
//! `mrpack` read info/detect_loader/filter_files/ensure_inside/install plan
//! (overrides skip 0-byte, traversal reject mục 76); `auto_fix` flow scan→plan→
//! download với callbacks inject. Còn lại: check_outdated network (dùng cùng
//! fetch_versions pattern).
//!
//! ## Parity notes
//!
//! - Legacy quarantine file-missing dùng code `INSTANCE_NOT_FOUND` (đúng behavior
//!   sidecar) — giữ nguyên, đừng đổi thành FILE_NOT_FOUND khi wiring
//! - Health: `minecraft` + `java` tự coi là đã cài (đừng báo missing_dependency)

pub mod auto_fix;
pub mod deflate;
pub mod health;
pub mod jar_reader;
pub mod modrinth;
pub mod mrpack;
pub mod quarantine;
pub mod scanner;
pub mod zipread;

pub use deflate::InflateError;
pub use health::{check_health, read_mod_info, version_newer, HealthIssue, ModInfo};
pub use jar_reader::{decode_class, jar_structure, scan_jar_classes, ClassFileError, ClassInfo};
pub use modrinth::{
    build_fix_plan, fetch_search, fetch_versions, known_dep_slug, parse_search_hits, pick_file,
    search_url, urlencode, versions_url, FixPlanItem, ModrinthError,
};
pub use mrpack::{
    build_install_plan, detect_loader, ensure_inside, filter_files, loader_version,
    read_mrpack_info, DownloadItem, InstallPlan, MrpackError, MrpackInfo, OverrideItem,
};
pub use auto_fix::{apply_fix_plan as apply_fix_plan_io, auto_fix_instance, AutoFixResult};
pub use scanner::{scan_mod, Finding, ScanReport, DANGEROUS_THRESHOLD, SUSPICIOUS_THRESHOLD};
pub use quarantine::{
    is_safe_name, Quarantine, QuarantineEntry, QuarantineError,
};
pub use zipread::{zip_entries, zip_read_entry, zip_read_entry_with_limit, ZipEntry, ZipError};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModsError {
    #[error("file not found: {0}")]
    FileNotFound(String),
    #[error("invalid filename: {0}")]
    InvalidFilename(String),
    #[error("io: {0}")]
    Io(String),
}

impl ModsError {
    pub fn code(&self) -> &'static str {
        match self {
            ModsError::FileNotFound(_) => "FILE_NOT_FOUND",
            ModsError::InvalidFilename(_) => "VALIDATION_FAILED",
            ModsError::Io(_) => "APP_INTERNAL",
        }
    }
}

/// Parity `ModService.list_installed` — sorted *.jar names; dir không có → [].
pub fn list_installed(mods_dir: &std::path::Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|e| e.path().is_file())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "jar"))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// Parity `ModService.uninstall` — unlink; file không có → Ok(false).
/// Filename chỉ là tên (không path) — traversal reject (mục 76).
pub fn uninstall(mods_dir: &std::path::Path, filename: &str) -> Result<bool, ModsError> {
    if !is_safe_name(filename) {
        return Err(ModsError::InvalidFilename(filename.to_string()));
    }
    let target = mods_dir.join(filename);
    if !target.is_file() {
        return Ok(false);
    }
    std::fs::remove_file(&target).map_err(|err| ModsError::Io(err.to_string()))?;
    Ok(true)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallResult {
    pub removed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("antares-mods-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn list_installed_sorted_jars_only() {
        let root = temp_root("list");
        let mods = root.join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("b.jar"), b"x").unwrap();
        std::fs::write(mods.join("a.jar"), b"x").unwrap();
        std::fs::write(mods.join("notes.txt"), b"x").unwrap();
        std::fs::create_dir_all(mods.join("sub.jar")).unwrap(); // dir không đếm

        let names = list_installed(&mods);
        assert_eq!(names, vec!["a.jar", "b.jar"]);
        // dir không tồn tại → []
        assert!(list_installed(&root.join("nope")).is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn uninstall_rejects_traversal_and_missing() {
        let root = temp_root("uninstall");
        let mods = root.join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("target.jar"), b"x").unwrap();

        // traversal reject
        assert_eq!(
            uninstall(&mods, "../escape.jar").unwrap_err().code(),
            "VALIDATION_FAILED"
        );
        assert_eq!(
            uninstall(&mods, "a/b.jar").unwrap_err().code(),
            "VALIDATION_FAILED"
        );
        // file không có → Ok(false)
        assert!(!uninstall(&mods, "absent.jar").unwrap());
        // xoá thành công
        assert!(uninstall(&mods, "target.jar").unwrap());
        assert!(!mods.join("target.jar").exists());
        // xoá lần 2 → false
        assert!(!uninstall(&mods, "target.jar").unwrap());
        let _ = std::fs::remove_dir_all(&root);
    }
}
