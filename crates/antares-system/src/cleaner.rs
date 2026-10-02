//! DiskCleaner — parity 1:1 `services/system/cleaner.py` (mục 4.3 Disk, 37, 76).
//!
//! Parity chính:
//! - SAFE_RULES 4 rule (id/glob/maxAgeDays đúng legacy); `old_snapshots` quét riêng
//!   theo glob `*/optimization-snapshots/opt-*.json` (legacy nhánh đặc biệt)
//! - `_collect`: bỏ file `manifest.json`, tuổi < maxAgeDays bỏ qua, ageDays round 1
//! - PROTECTED_DIRS 5 nhóm — chỉ đếm bytes (dir = rglob files; base không tồn tại → 0)
//! - `scan()` trả `{safe, protected, totalCleanableBytes}`
//! - `clean()`: move vào `trash/<cleanId>/<8hex>`, chụp size TRƯỚC khi move, refuse
//!   path ngoài data dir (VALIDATION_FAILED), manifest.json ghi atomic
//! - `undo()`: đọc manifest, restore từng item (tạo parent), xoá thư mục trash
//! - `empty_trash()`: đếm số thư mục xoá được

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::{write_json_atomic, CleanResult, SystemError, UndoResult};

/// Rule quét "Safe" — chỉ file launcher-owned quá hạn hoặc phần download dở.
/// Parity `SAFE_RULES` (id/root/glob/maxAgeDays/labelKey giữ nguyên).
#[derive(Debug, Clone)]
pub struct SafeRule {
    pub id: &'static str,
    pub root: &'static str,
    pub glob: &'static str,
    pub max_age_days: f64,
    pub label_key: &'static str,
}

pub const SAFE_RULES: &[SafeRule] = &[
    SafeRule {
        id: "download_parts",
        root: "cache/downloads",
        glob: "**/*.part",
        max_age_days: 1.0,
        label_key: "downloadParts",
    },
    SafeRule {
        id: "old_logs",
        root: "logs",
        glob: "**/*",
        max_age_days: 7.0,
        label_key: "oldLogs",
    },
    SafeRule {
        id: "old_snapshots",
        root: "instances",
        glob: "*/optimization-snapshots/opt-*.json",
        max_age_days: 30.0,
        label_key: "oldSnapshots",
    },
    SafeRule {
        id: "trash",
        root: "trash",
        glob: "*/*",
        max_age_days: 7.0,
        label_key: "trash",
    },
];

/// Chỉ hiển thị kích thước — không bao giờ xoá (mục 37). Parity `PROTECTED_DIRS`.
pub const PROTECTED_DIRS: &[(&str, &str, &str)] = &[
    ("saves", "instances/*/game/saves", "saves"),
    ("screenshots", "instances/*/game/screenshots", "screenshots"),
    ("mods", "instances/*/game/mods", "mods"),
    ("resourcepacks", "instances/*/game/resourcepacks", "resourcepacks"),
    ("accounts", "accounts", "accounts"),
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupItem {
    pub path: String,
    pub bytes: u64,
    pub age_days: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupGroup {
    pub id: &'static str,
    pub label_key: &'static str,
    pub count: usize,
    pub bytes: u64,
    pub items: Vec<CleanupItem>,
    pub risk: &'static str,
    pub reversible: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtectedDir {
    pub id: &'static str,
    pub label_key: &'static str,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub safe: Vec<CleanupGroup>,
    pub protected: Vec<ProtectedDir>,
    pub total_cleanable_bytes: u64,
}

/// Disk cleaner — `data_dir` là `<root>/data` của launcher.
pub struct DiskCleaner {
    data_dir: PathBuf,
    /// Inject clock cho test (epoch seconds).
    now: Box<dyn Fn() -> f64 + Send + Sync>,
    /// Generator clean_id — inject cho test (legacy uuid4 hex 12).
    new_id: fn() -> String,
}

impl DiskCleaner {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            now: Box::new(|| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0)
            }),
            new_id: || {
                // 12 hex — legacy uuid.uuid4().hex[:12]; nanos đủ random cho id.
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos() as u64 ^ (d.as_secs() << 20))
                    .unwrap_or(0);
                format!("{nanos:012x}")
            },
        }
    }

    /// Inject clock/id cho test deterministic.
    pub fn with_clock(
        data_dir: impl Into<PathBuf>,
        now: impl Fn() -> f64 + Send + Sync + 'static,
        new_id: fn() -> String,
    ) -> Self {
        Self {
            data_dir: data_dir.into(),
            now: Box::new(now),
            new_id,
        }
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    // ------------------------------------------------------------------
    // Scan
    // ------------------------------------------------------------------

    pub fn scan(&self) -> ScanReport {
        let mut safe = Vec::new();
        for rule in SAFE_RULES {
            let (items, total) = if rule.id == "old_snapshots" {
                // Legacy nhánh đặc biệt: glob trên instances/, lọc tuổi sau stat.
                let base = self.data_dir.join(rule.root);
                let mut items = Vec::new();
                let mut total = 0u64;
                for path in glob_files(&base, rule.glob) {
                    if self.age_days(&path) >= rule.max_age_days {
                        total += path.metadata().map(|m| m.len()).unwrap_or(0);
                        items.push(self.item(&path));
                    }
                }
                (items, total)
            } else {
                let root = self.data_dir.join(rule.root);
                self.collect(&root, rule.glob, rule.max_age_days)
            };
            safe.push(CleanupGroup {
                id: rule.id,
                label_key: rule.label_key,
                count: items.len(),
                bytes: total,
                items,
                risk: "LOW",
                reversible: true,
            });
        }
        let total_cleanable_bytes = safe.iter().map(|g| g.bytes).sum();
        ScanReport {
            safe,
            protected: self.protected_dirs(),
            total_cleanable_bytes,
        }
    }

    /// Collect files theo glob: bỏ manifest.json, lọc tuổi, tính total bytes.
    fn collect(&self, root: &Path, pattern: &str, max_age_days: f64) -> (Vec<CleanupItem>, u64) {
        let mut items = Vec::new();
        let mut total = 0u64;
        for path in glob_files(root, pattern) {
            if !path.is_file() || path.file_name().is_some_and(|n| n == "manifest.json") {
                continue;
            }
            if self.age_days(&path) < max_age_days {
                continue;
            }
            total += path.metadata().map(|m| m.len()).unwrap_or(0);
            items.push(self.item(&path));
        }
        (items, total)
    }

    fn item(&self, path: &Path) -> CleanupItem {
        let age = self.age_days(path);
        CleanupItem {
            path: path.display().to_string(),
            bytes: path.metadata().map(|m| m.len()).unwrap_or(0),
            age_days: (age * 10.0).round() / 10.0,
        }
    }

    fn age_days(&self, path: &Path) -> f64 {
        match path.metadata().and_then(|m| m.modified()) {
            Ok(modified) => {
                let mtime = modified
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0);
                ((self.now)() - mtime).max(0.0) / 86_400.0
            }
            // Parity except OSError → 0.0
            Err(_) => 0.0,
        }
    }

    fn protected_dirs(&self) -> Vec<ProtectedDir> {
        let mut out = Vec::new();
        for &(id, pattern, label) in PROTECTED_DIRS {
            // base = phần trước "/*" đầu tiên (parity path.split("/*")[0])
            let base = self.data_dir.join(pattern.split("/*").next().unwrap_or(pattern));
            let mut total = 0u64;
            for path in glob_files(&self.data_dir, pattern) {
                if path.is_dir() {
                    for entry in walk_files(&path) {
                        total += entry.metadata().map(|m| m.len()).unwrap_or(0);
                    }
                } else if path.is_file() {
                    total += path.metadata().map(|m| m.len()).unwrap_or(0);
                }
            }
            let total = if base.exists() { total } else { 0 };
            out.push(ProtectedDir {
                id,
                label_key: label,
                bytes: total,
            });
        }
        out
    }

    // ------------------------------------------------------------------
    // Clean (move to trash) + Undo + Empty
    // ------------------------------------------------------------------

    /// Move các file user chọn vào trash + manifest (mục 37: không xoá silent).
    pub fn clean(&self, paths: &[String]) -> Result<CleanResult, SystemError> {
        let clean_id = (self.new_id)();
        let trash = self.data_dir.join("trash").join(&clean_id);
        std::fs::create_dir_all(&trash).map_err(|err| SystemError::Io(err.to_string()))?;
        let mut moved: Vec<serde_json::Value> = Vec::new();
        let mut freed = 0u64;
        for raw in paths {
            let src = PathBuf::from(raw);
            if !self.owned(&src) {
                return Err(SystemError::OutsideData(src.display().to_string()));
            }
            if !src.is_file() {
                continue;
            }
            // Chụp size TRƯỚC khi move (src mất sau move — parity comment legacy).
            let size = src.metadata().map(|m| m.len()).unwrap_or(0);
            let dest = trash.join((self.new_id)().chars().take(8).collect::<String>());
            std::fs::rename(&src, &dest).map_err(|err| SystemError::Io(err.to_string()))?;
            moved.push(serde_json::json!({"src": raw, "dest": dest.display().to_string()}));
            freed += size;
        }
        write_json_atomic(
            &trash.join("manifest.json"),
            &serde_json::json!({
                "id": clean_id,
                "ts": (self.now)(),
                "items": moved,
                "bytes": freed,
            }),
        )?;
        Ok(CleanResult {
            clean_id,
            moved: moved.len(),
            bytes: freed,
        })
    }

    /// Restore từ manifest — trả `restored`; manifest thiếu → FILE_NOT_FOUND.
    pub fn undo(&self, clean_id: &str) -> Result<UndoResult, SystemError> {
        let trash = self.data_dir.join("trash").join(clean_id);
        let manifest = trash.join("manifest.json");
        if !manifest.is_file() {
            return Err(SystemError::ManifestNotFound(manifest.display().to_string()));
        }
        let bytes = std::fs::read(&manifest).map_err(|err| SystemError::Io(err.to_string()))?;
        let data: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|err| SystemError::Io(err.to_string()))?;
        let mut restored = 0usize;
        if let Some(items) = data.get("items").and_then(|v| v.as_array()) {
            for item in items {
                let dest = item.get("dest").and_then(|v| v.as_str()).unwrap_or("");
                let src = item.get("src").and_then(|v| v.as_str()).unwrap_or("");
                if dest.is_empty() || src.is_empty() {
                    continue;
                }
                let (src_path, dest_path) = (PathBuf::from(dest), PathBuf::from(src));
                if src_path.is_file() {
                    if let Some(parent) = dest_path.parent() {
                        std::fs::create_dir_all(parent)
                            .map_err(|err| SystemError::Io(err.to_string()))?;
                    }
                    std::fs::rename(&src_path, &dest_path)
                        .map_err(|err| SystemError::Io(err.to_string()))?;
                    restored += 1;
                }
            }
        }
        let _ = std::fs::remove_dir_all(&trash);
        Ok(UndoResult { restored })
    }

    /// Xoá hẳn toàn bộ trash — trả số thư mục đã xoá (parity đếm `d.is_dir()`).
    pub fn empty_trash(&self) -> usize {
        let trash = self.data_dir.join("trash");
        let Ok(entries) = std::fs::read_dir(&trash) else {
            return 0;
        };
        let mut n = 0;
        for entry in entries.flatten() {
            if entry.path().is_dir() && std::fs::remove_dir_all(entry.path()).is_ok() {
                n += 1;
            }
        }
        n
    }

    /// File phải nằm trong data dir của launcher (chống path escape — mục 76).
    /// Parity `resolve().relative_to(resolve())` — path tuyệt đối + prefix thật.
    fn owned(&self, path: &Path) -> bool {
        let base = std::fs::canonicalize(&self.data_dir).unwrap_or_else(|_| self.data_dir.clone());
        let candidate = std::fs::canonicalize(path).unwrap_or_else(|_| {
            // file có thể chưa tồn tại khi check — resolve lexical parent
            let abs = if path.is_absolute() {
                path.to_path_buf()
            } else {
                base.join(path)
            };
            abs
        });
        candidate.starts_with(&base)
    }
}

// ---------------------------------------------------------------------------
// Mini glob matcher — hỗ trợ đúng các pattern legacy dùng:
// `**/*.part`, `**/*`, `*/optimization-snapshots/opt-*.json`, `*/*`,
// `instances/*/game/saves`, `accounts`. Không cần glob crate: pattern chỉ gồm
// `**` (bất kỳ depth), `*` (trong 1 segment), ký tự thường.
// ---------------------------------------------------------------------------

fn glob_files(root: &Path, pattern: &str) -> Vec<PathBuf> {
    let parts: Vec<&str> = pattern.split('/').collect();
    let mut out = Vec::new();
    walk_match(root, &parts, &mut out);
    out
}

fn walk_match(dir: &Path, parts: &[&str], out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    // Dọn `.`/`./` đầu pattern (legacy dùng path string, không có trường hợp này,
    // nhưng giữ matcher đơn giản an toàn).
    let parts = if parts.first().is_some_and(|p| *p == ".") {
        &parts[1..]
    } else {
        parts
    };
    if parts.is_empty() {
        return;
    }
    let (first, rest) = (parts[0], &parts[1..]);
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if first == "**" {
            // `**` khớp 0+ segment: ăn 0 → khớp rest với CHÍNH entry này
            // (vd `**/*` phải ăn file ngay dưới root), ăn 1+ → đệ quy với ** giữ nguyên.
            if rest.is_empty() {
                if path.is_file() {
                    out.push(path.clone());
                }
            } else {
                match_here(&path, &name, rest, out);
            }
            if path.is_dir() {
                walk_match(&path, parts, out);
            }
        } else if seg_match(first, &name) {
            if rest.is_empty() {
                out.push(path);
            } else if path.is_dir() {
                walk_match(&path, rest, out);
            }
        }
    }
}

/// Khớp `parts` với CHÍNH path/name (không đọc dir) — nhánh "`**` ăn 0 segment".
fn match_here(path: &Path, name: &str, parts: &[&str], out: &mut Vec<PathBuf>) {
    let Some((first, rest)) = parts.split_first() else {
        out.push(path.to_path_buf());
        return;
    };
    if first == &"**" {
        // rest có thể bắt đầu bằng `**` (pattern tổng quát): ăn 0 → rest khớp
        // chính path; ăn 1+ → giữ `**` xuống con.
        match_here(path, name, rest, out);
        if path.is_dir() {
            walk_match(path, parts, out);
        }
        return;
    }
    if !seg_match(first, name) {
        return;
    }
    if rest.is_empty() {
        out.push(path.to_path_buf());
    } else if path.is_dir() {
        walk_match(path, rest, out);
    }
}

/// Khớp 1 segment: `*` wildcard, ký tự thường literal. `opt-*.json` cũng vậy.
fn seg_match(pattern: &str, name: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    match pattern.split_once('*') {
        None => pattern == name,
        Some((prefix, suffix)) => {
            name.len() >= prefix.len() + suffix.len()
                && name.starts_with(prefix)
                && name.ends_with(suffix)
        }
    }
}

/// Duyệt toàn bộ file dưới dir (cho protected dirs).
fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(walk_files(&path));
        } else {
            out.push(path);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("antares-cleaner-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn clock_at(epoch: f64) -> impl Fn() -> f64 {
        move || epoch
    }

    fn fixed_id() -> fn() -> String {
        || "abcdef012345".to_string()
    }

    fn write_file(path: &Path, size: usize, age_days_ago: f64, now: f64) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, vec![b'x'; size]).unwrap();
        let mtime = now - age_days_ago * 86_400.0;
        set_mtime(path, mtime);
    }

fn set_mtime(path: &Path, epoch: f64) {
    // `File::set_times` cross-platform (Windows qua SetFileTime) — trước đây
    // nhánh non-unix là no-op → file age=0 → scan test fail trên Windows CI.
    use std::time::{Duration, UNIX_EPOCH};
    let st = UNIX_EPOCH
        .checked_add(Duration::from_secs_f64(epoch.max(0.0)))
        .unwrap();
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .expect("open for mtime");
    file.set_times(std::fs::FileTimes::new().set_modified(st))
        .expect("set mtime");
}

    #[test]
    fn seg_match_patterns() {
        assert!(seg_match("*", "anything.log"));
        assert!(seg_match("opt-*.json", "opt-20260929.json"));
        assert!(seg_match("opt-*.json", "opt-.json"));
        assert!(!seg_match("opt-*.json", "opt.json"));
        assert!(!seg_match("opt-*.json", "xopt-1.json"));
        assert!(seg_match("manifest.json", "manifest.json"));
        assert!(!seg_match("manifest.json", "other.json"));
    }

    #[test]
    fn scan_counts_old_files_only() {
        let root = temp_root("scan");
        let now = 1_700_000_000.0;
        let cleaner = DiskCleaner::with_clock(&root, clock_at(now), fixed_id());

        // download_parts: 1 file quá 1 ngày + 1 file mới
        write_file(&root.join("cache/downloads/a.jar.part"), 100, 2.0, now);
        write_file(&root.join("cache/downloads/fresh.jar.part"), 10, 0.5, now);
        // old_logs: 2 file quá 7 ngày
        write_file(&root.join("logs/2026/old.log"), 200, 8.0, now);
        write_file(&root.join("logs/2026/new.log"), 5, 1.0, now);

        let report = cleaner.scan();
        let parts = report.safe.iter().find(|g| g.id == "download_parts").unwrap();
        assert_eq!(parts.count, 1);
        assert_eq!(parts.bytes, 100);
        let logs = report.safe.iter().find(|g| g.id == "old_logs").unwrap();
        assert_eq!(logs.count, 1);
        assert_eq!(logs.bytes, 200);
        // mọi group đều risk LOW + reversible (parity)
        assert!(report.safe.iter().all(|g| g.risk == "LOW" && g.reversible));
        // total = 100 + 200 (snapshots/trash rỗng)
        assert_eq!(report.total_cleanable_bytes, 300);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_skips_manifest_and_lists_protected() {
        let root = temp_root("protected");
        let now = 1_700_000_000.0;
        let cleaner = DiskCleaner::with_clock(&root, clock_at(now), fixed_id());
        write_file(&root.join("logs/manifest.json"), 500, 30.0, now);
        write_file(&root.join("logs/real.log"), 10, 30.0, now);
        // protected: accounts dir có file
        write_file(&root.join("accounts/tokens.json"), 64, 1.0, now);

        let report = cleaner.scan();
        let logs = report.safe.iter().find(|g| g.id == "old_logs").unwrap();
        assert_eq!(logs.count, 1, "manifest.json phải bị bỏ qua");
        assert_eq!(logs.bytes, 10);

        let accounts = report.protected.iter().find(|p| p.id == "accounts").unwrap();
        assert_eq!(accounts.bytes, 64);
        // protected không bao giờ có action — chỉ id/label/bytes
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn clean_moves_to_trash_and_undo_restores() {
        let root = temp_root("undo");
        let now = 1_700_000_000.0;
        let cleaner = DiskCleaner::with_clock(&root, clock_at(now), fixed_id());
        let file = root.join("logs/old.log");
        write_file(&file, 256, 10.0, now);

        let result = cleaner.clean(&[file.display().to_string()]).unwrap();
        assert_eq!(result.moved, 1);
        assert_eq!(result.bytes, 256);
        assert_eq!(result.clean_id, "abcdef012345");
        assert!(!file.exists(), "file phải đã move vào trash");

        let trash_dir = root.join("trash/abcdef012345");
        assert!(trash_dir.join("manifest.json").is_file());

        // undo → file trở về đúng chỗ, trash bị xoá
        let undo = cleaner.undo("abcdef012345").unwrap();
        assert_eq!(undo.restored, 1);
        assert!(file.is_file());
        assert_eq!(std::fs::read(&file).unwrap().len(), 256);
        assert!(!trash_dir.exists());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn clean_refuses_outside_data_dir() {
        let root = temp_root("escape");
        let cleaner = DiskCleaner::with_clock(&root, clock_at(0.0), fixed_id());
        let err = cleaner.clean(&["/etc/passwd".to_string()]).unwrap_err();
        assert_eq!(err.code(), "VALIDATION_FAILED");
        // path tồn tại nhưng ngoài data
        let outside = std::env::temp_dir().join("antares-outside.txt");
        std::fs::write(&outside, b"x").unwrap();
        let err = cleaner.clean(&[outside.display().to_string()]).unwrap_err();
        assert_eq!(err.code(), "VALIDATION_FAILED");
        let _ = std::fs::remove_file(&outside);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn undo_missing_manifest_is_file_not_found() {
        let root = temp_root("nomanifest");
        let cleaner = DiskCleaner::with_clock(&root, clock_at(0.0), fixed_id());
        let err = cleaner.undo("nonexistent").unwrap_err();
        assert_eq!(err.code(), "FILE_NOT_FOUND");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn empty_trash_counts_removed_dirs() {
        let root = temp_root("empty");
        let cleaner = DiskCleaner::with_clock(&root, clock_at(0.0), fixed_id());
        std::fs::create_dir_all(root.join("trash/aaa")).unwrap();
        std::fs::create_dir_all(root.join("trash/bbb")).unwrap();
        std::fs::create_dir_all(root.join("trash/ccc/inner")).unwrap();
        assert_eq!(cleaner.empty_trash(), 3);
        assert_eq!(cleaner.empty_trash(), 0, "trash rỗng → 0");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn glob_matches_legacy_patterns() {
        let root = temp_root("glob");
        write_file(&root.join("instances/i1/optimization-snapshots/opt-1.json"), 5, 0.0, 0.0);
        write_file(&root.join("instances/i2/optimization-snapshots/opt-2.json"), 6, 0.0, 0.0);
        write_file(&root.join("instances/i2/optimization-snapshots/other.json"), 7, 0.0, 0.0);

        let hits = glob_files(&root.join("instances"), "*/optimization-snapshots/opt-*.json");
        assert_eq!(hits.len(), 2);
        assert!(hits.iter().all(|p| p
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with("opt-"))));
        let _ = std::fs::remove_dir_all(&root);
    }

    // Helper dùng chung đã khai báo ở trên.
}
