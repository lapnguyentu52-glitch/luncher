
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartInfo {
    pub part_path: PathBuf,
    pub meta_path: PathBuf,
    pub url: String,
    pub etag: Option<String>,
    pub size: u64,
}

impl PartInfo {
    pub fn for_target(target: &Path) -> Self {
        let file_name = target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let part = target.with_file_name(format!("{file_name}.antares-part"));
        let meta = target.with_file_name(format!("{file_name}.antares-part.meta"));
        Self {
            part_path: part,
            meta_path: meta,
            url: String::new(),
            etag: None,
            size: 0,
        }
    }

    pub fn save_meta(&self) -> std::io::Result<()> {
        let json = serde_json::json!({
            "url": self.url,
            "etag": self.etag,
            "size": self.size,
        });
        std::fs::write(&self.meta_path, json.to_string())
    }

    pub fn load_meta(&mut self) -> Result<bool, ResumeError> {
        let bytes = std::fs::read(&self.meta_path)
            .map_err(|_| ResumeError::MetaUnreadable(self.meta_path.display().to_string()))?;
        let Ok(data) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return Ok(false);
        };
        let Some(saved_url) = data.get("url").and_then(serde_json::Value::as_str) else {
            return Ok(false);
        };
        if saved_url.is_empty() || (!self.url.is_empty() && self.url != saved_url) {
            return Ok(false);
        }
        self.etag = data
            .get("etag")
            .and_then(serde_json::Value::as_str)
            .map(str::to_string);
        self.size = data
            .get("size")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        self.url = saved_url.to_string();
        Ok(true)
    }

    pub fn bytes_have(&self) -> u64 {
        std::fs::metadata(&self.part_path)
            .map(|meta| meta.len())
            .unwrap_or(0)
    }

    pub fn cleanup(&self) {
        let _ = std::fs::remove_file(&self.part_path);
        let _ = std::fs::remove_file(&self.meta_path);
    }

    pub fn finalize(&self, target: &Path) -> std::io::Result<()> {
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::rename(&self.part_path, target)?;
        let _ = std::fs::remove_file(&self.meta_path);
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResumeError {
    #[error("part meta unreadable: {0}")]
    MetaUnreadable(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("antares-resume-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    #[test]
    fn part_paths_follow_legacy_naming() {
        let target = Path::new("/game/libraries/a.jar");
        let part = PartInfo::for_target(target);
        assert_eq!(part.part_path, PathBuf::from("/game/libraries/a.jar.antares-part"));
        assert_eq!(part.meta_path, PathBuf::from("/game/libraries/a.jar.antares-part.meta"));
    }

    #[test]
    fn save_load_meta_roundtrip() {
        let root = temp_root("meta");
        let mut part = PartInfo::for_target(&root.join("file.jar"));
        part.url = "https://example.com/file.jar".into();
        part.etag = Some("\"abc123\"".into());
        part.size = 4096;
        part.save_meta().unwrap();

        let mut loaded = PartInfo::for_target(&root.join("file.jar"));
        assert!(loaded.load_meta().unwrap());
        assert_eq!(loaded.url, part.url);
        assert_eq!(loaded.etag.as_deref(), Some("\"abc123\""));
        assert_eq!(loaded.size, 4096);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn load_meta_rejects_different_url() {
        let root = temp_root("urlmatch");
        let mut saved = PartInfo::for_target(&root.join("file.jar"));
        saved.url = "https://mirror-a.com/file.jar".into();
        saved.size = 100;
        saved.save_meta().unwrap();

        let mut resumed = PartInfo::for_target(&root.join("file.jar"));
        resumed.url = "https://mirror-b.com/file.jar".into();
        assert!(!resumed.load_meta().unwrap(), "resume khác nguồn phải bị chặn");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn load_meta_corrupt_json_is_false() {
        let root = temp_root("corrupt");
        let part = PartInfo::for_target(&root.join("file.jar"));
        std::fs::write(&part.meta_path, b"{not json").unwrap();
        let mut loaded = PartInfo::for_target(&root.join("file.jar"));
        assert!(!loaded.load_meta().unwrap());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn bytes_have_cleanup_and_finalize() {
        let root = temp_root("finalize");
        let target = root.join("out/file.jar");
        let mut part = PartInfo::for_target(&target);
        part.url = "https://example.com/f.jar".into();
        std::fs::create_dir_all(&root.join("out")).unwrap();
        std::fs::write(&part.part_path, b"part-data").unwrap();
        part.save_meta().unwrap();
        assert_eq!(part.bytes_have(), 9);

        part.finalize(&target).unwrap();
        assert!(target.is_file());
        assert_eq!(std::fs::read(&target).unwrap(), b"part-data");
        assert!(!part.part_path.exists());
        assert!(!part.meta_path.exists());
        assert_eq!(part.bytes_have(), 0);

        let _ = std::fs::remove_dir_all(&root);
    }
}
