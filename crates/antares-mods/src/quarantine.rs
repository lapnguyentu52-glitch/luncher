//! Quarantine — parity `services/mods/scanner/quarantine.py` (mục 368
//! safe-disable pattern). Jar nguy hiểm được move vào vault kèm manifest ghi lý
//! do. Không delete — user restore được nếu false positive.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum QuarantineError {
    #[error("file not found: {0}")]
    FileNotFound(String),
    #[error("quarantine entry not found: {0}")]
    EntryNotFound(String),
    #[error("unsafe file name: {0}")]
    UnsafeName(String),
    #[error("io: {0}")]
    Io(String),
}

impl QuarantineError {
    /// Code taxonomy §117 — parity `codes.INSTANCE_NOT_FOUND` (legacy dùng code này
    /// cho file/entry missing) + VALIDATION_FAILED cho tên nguy hiểm.
    pub fn code(&self) -> &'static str {
        match self {
            QuarantineError::FileNotFound(_) | QuarantineError::EntryNotFound(_) => {
                "INSTANCE_NOT_FOUND"
            }
            QuarantineError::UnsafeName(_) => "VALIDATION_FAILED",
            QuarantineError::Io(_) => "APP_INTERNAL",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineEntry {
    pub original_name: String,
    pub original_path: String,
    pub instance_id: String,
    pub verdict: String,
    pub score: i64,
    pub findings: serde_json::Value,
    pub quarantined_at: f64,
    pub quarantine_file: String,
}

/// Parity `is_safe_name` (core/utils/paths.py) — tên jar vào vault phải an toàn.
pub fn is_safe_name(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." || name.ends_with('.') {
        return false;
    }
    !name
        .chars()
        .any(|c| matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
}

/// Quarantine vault — `<cache>/quarantine/` + manifest.json `{items: {}}`.
pub struct Quarantine {
    dir: PathBuf,
    now: fn() -> f64,
}

impl Quarantine {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self {
            dir: dir.into(),
            now: || {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0)
            },
        }
    }

    pub fn with_clock(dir: impl Into<PathBuf>, now: fn() -> f64) -> Self {
        Self { dir: dir.into(), now }
    }

    fn manifest_path(&self) -> PathBuf {
        self.dir.join("manifest.json")
    }

    fn load_manifest(&self) -> serde_json::Value {
        std::fs::read_to_string(self.manifest_path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(serde_json::json!({"items": {}}))
    }

    fn save_manifest(&self, data: &serde_json::Value) -> Result<(), QuarantineError> {
        std::fs::create_dir_all(&self.dir).map_err(|err| QuarantineError::Io(err.to_string()))?;
        let tmp = self.dir.join("manifest.json.tmp");
        std::fs::write(&tmp, data.to_string())
            .map_err(|err| QuarantineError::Io(err.to_string()))?;
        std::fs::rename(&tmp, self.manifest_path())
            .map_err(|err| QuarantineError::Io(err.to_string()))?;
        Ok(())
    }

    /// Move jar vào vault + ghi manifest. Trả entry (parity `quarantine`).
    pub fn quarantine(
        &self,
        jar_path: &Path,
        instance_id: &str,
        verdict: &str,
        score: i64,
        findings: serde_json::Value,
    ) -> Result<QuarantineEntry, QuarantineError> {
        if !jar_path.exists() {
            return Err(QuarantineError::FileNotFound(
                jar_path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            ));
        }
        let original_name = jar_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        // Tên file unique: `<epoch>_<name>` — tên nguy hiểm → "unnamed.jar" (parity).
        let safe_name = if is_safe_name(&original_name) {
            original_name.clone()
        } else if original_name.is_empty() {
            return Err(QuarantineError::UnsafeName(original_name));
        } else {
            // Tên chứa / \ : ... — parity legacy chỉ dùng is_safe_name cho tên file;
            // tên jar trong mods dir luôn an toàn — trường hợp này chỉ từ path lạ.
            "unnamed.jar".to_string()
        };
        let dest_name = format!("{}_{safe_name}", (self.now)() as i64);
        let dest = self.dir.join(&dest_name);

        std::fs::create_dir_all(&self.dir).map_err(|err| QuarantineError::Io(err.to_string()))?;
        std::fs::rename(jar_path, &dest).map_err(|err| QuarantineError::Io(err.to_string()))?;

        let entry = QuarantineEntry {
            original_name,
            original_path: jar_path.display().to_string(),
            instance_id: instance_id.to_string(),
            verdict: verdict.to_string(),
            score,
            findings,
            quarantined_at: (self.now)(),
            quarantine_file: dest_name,
        };
        let mut manifest = self.load_manifest();
        if let Some(items) = manifest.get_mut("items").and_then(|v| v.as_object_mut()) {
            items.insert(entry.quarantine_file.clone(), serde_json::to_value(&entry).unwrap());
        }
        self.save_manifest(&manifest)?;
        Ok(entry)
    }

    /// Danh sách entry trong vault (parity `list` — values của manifest.items).
    pub fn list(&self) -> Vec<QuarantineEntry> {
        self.load_manifest()["items"]
            .as_object()
            .map(|items| {
                items
                    .values()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Restore mod về chỗ cũ (false positive) — parity `restore`.
    pub fn restore(&self, quarantine_file: &str) -> Result<QuarantineEntry, QuarantineError> {
        let manifest = self.load_manifest();
        let entry_value = manifest["items"]
            .get(quarantine_file)
            .cloned()
            .ok_or_else(|| QuarantineError::EntryNotFound(quarantine_file.to_string()))?;
        let entry: QuarantineEntry = serde_json::from_value(entry_value)
            .map_err(|err| QuarantineError::Io(err.to_string()))?;
        let src = self.dir.join(quarantine_file);
        if !src.exists() {
            return Err(QuarantineError::FileNotFound(
                src.display().to_string(),
            ));
        }
        let original = PathBuf::from(&entry.original_path);
        if let Some(parent) = original.parent() {
            std::fs::create_dir_all(parent).map_err(|err| QuarantineError::Io(err.to_string()))?;
        }
        std::fs::rename(&src, &original).map_err(|err| QuarantineError::Io(err.to_string()))?;
        let mut manifest = manifest;
        if let Some(items) = manifest.get_mut("items").and_then(|v| v.as_object_mut()) {
            items.remove(quarantine_file);
        }
        self.save_manifest(&manifest)?;
        Ok(entry)
    }

    /// Xoá hẳn entry + file (UI confirm) — parity `delete` trả bool.
    pub fn delete(&self, quarantine_file: &str) -> Result<bool, QuarantineError> {
        let mut manifest = self.load_manifest();
        let Some(items) = manifest.get_mut("items").and_then(|v| v.as_object_mut()) else {
            return Ok(false);
        };
        if items.remove(quarantine_file).is_none() {
            return Ok(false);
        }
        let src = self.dir.join(quarantine_file);
        if src.exists() {
            std::fs::remove_file(&src).map_err(|err| QuarantineError::Io(err.to_string()))?;
        }
        self.save_manifest(&manifest)?;
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("antares-quarantine-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn fixed_clock() -> fn() -> f64 {
        || 1_700_000_000.0
    }

    #[test]
    fn is_safe_name_parity() {
        assert!(is_safe_name("jei-1.21.jar"));
        assert!(!is_safe_name(""));
        assert!(!is_safe_name(".."));
        assert!(!is_safe_name("evil.jar."));
        assert!(!is_safe_name("a/b.jar"));
        assert!(!is_safe_name("a\\b.jar"));
        assert!(!is_safe_name("a:b"));
    }

    #[test]
    fn quarantine_move_and_list() {
        let root = temp_root("move");
        let vault = root.join("vault");
        let jar = root.join("mods/evil.jar");
        std::fs::create_dir_all(jar.parent().unwrap()).unwrap();
        std::fs::write(&jar, b"dangerous jar").unwrap();

        let q = Quarantine::with_clock(&vault, fixed_clock());
        let entry = q
            .quarantine(
                &jar,
                "i1",
                "DANGEROUS",
                75,
                serde_json::json!([{"ruleId": "exec_net", "title": "download+exec", "severity": "critical"}]),
            )
            .unwrap();
        assert_eq!(entry.quarantine_file, "1700000000_evil.jar");
        assert!(!jar.exists(), "jar phải được move vào vault");
        assert!(vault.join("1700000000_evil.jar").is_file());

        let list = q.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].original_name, "evil.jar");
        assert_eq!(list[0].verdict, "DANGEROUS");
        assert_eq!(list[0].instance_id, "i1");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn quarantine_missing_file_errors() {
        let root = temp_root("missing");
        let q = Quarantine::with_clock(root.join("vault"), fixed_clock());
        let err = q.quarantine(&root.join("nope.jar"), "i1", "SAFE", 0, serde_json::json!([])).unwrap_err();
        assert_eq!(err.code(), "INSTANCE_NOT_FOUND");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn restore_moves_back_and_removes_entry() {
        let root = temp_root("restore");
        let vault = root.join("vault");
        let jar = root.join("mods/evil.jar");
        std::fs::create_dir_all(jar.parent().unwrap()).unwrap();
        std::fs::write(&jar, b"jar").unwrap();
        let q = Quarantine::with_clock(&vault, fixed_clock());
        let entry = q.quarantine(&jar, "i1", "DANGEROUS", 80, serde_json::json!([])).unwrap();

        // entry lạ → lỗi
        assert_eq!(q.restore("nope").unwrap_err().code(), "INSTANCE_NOT_FOUND");

        // restore → jar trở về chỗ cũ, entry xoá
        let restored = q.restore(&entry.quarantine_file).unwrap();
        assert_eq!(restored.original_path, jar.display().to_string());
        assert!(jar.is_file());
        assert!(!vault.join(&entry.quarantine_file).exists());
        assert!(q.list().is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn delete_removes_file_and_entry() {
        let root = temp_root("delete");
        let vault = root.join("vault");
        let jar = root.join("mods/evil.jar");
        std::fs::create_dir_all(jar.parent().unwrap()).unwrap();
        std::fs::write(&jar, b"jar").unwrap();
        let q = Quarantine::with_clock(&vault, fixed_clock());
        let entry = q.quarantine(&jar, "i1", "DANGEROUS", 90, serde_json::json!([])).unwrap();

        // entry lạ → false
        assert!(!q.delete("nope").unwrap());
        assert!(q.delete(&entry.quarantine_file).unwrap());
        assert!(!vault.join(&entry.quarantine_file).exists(), "file xoá hẳn");
        assert!(q.list().is_empty());
        // delete lần 2 → false
        assert!(!q.delete(&entry.quarantine_file).unwrap());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn manifest_corrupt_is_tolerated() {
        let root = temp_root("corrupt");
        let vault = root.join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        std::fs::write(vault.join("manifest.json"), "{broken").unwrap();
        let q = Quarantine::with_clock(&vault, fixed_clock());
        assert!(q.list().is_empty(), "manifest corrupt → items rỗng (parity)");
        let _ = std::fs::remove_dir_all(&root);
    }
}
