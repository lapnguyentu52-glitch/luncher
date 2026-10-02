//! antares-storage — §98 Storage Layer 2.0
//!
//! Không để service tự mở file lung tung. Tất cả qua `StorageService` với:
//! - Scoped roots (AppDataRoot, CacheRoot, LogsRoot, ...) — service chỉ thấy root của mình
//! - Path sandbox: normalize + reject traversal/absolute (§50)
//! - Atomic writes: tmp file + rename

mod error;
mod paths;
mod service;

pub use error::StorageError;
pub use paths::ScopedRoot;
pub use service::{StorageHandle, StorageService};

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;

    fn temp_root(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("antares-storage-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("create temp root");
        dir
    }

    #[test]
    fn write_json_atomic_roundtrip() {
        let root = temp_root("atomic");
        let service = StorageService::new(root.clone());
        let handle = service.scoped(ScopedRoot::AppData);

        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct Cfg {
            name: String,
            count: u32,
        }

        let cfg = Cfg {
            name: "antares".into(),
            count: 42,
        };
        handle.write_json_atomic("config/app.json", &cfg).expect("write");

        let loaded: Cfg = handle.read_json("config/app.json").expect("read");
        assert_eq!(loaded, cfg);

        // Không sótmp file
        let leftovers = fs::read_dir(root.join("app-data").join("config"))
            .expect("dir")
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "tmp"))
            .count();
        assert_eq!(leftovers, 0);

        let _ = fs::remove_dir_all(&root);
    }

    #[test]
    fn sandbox_rejects_traversal() {
        let root = temp_root("sandbox");
        let service = StorageService::new(root);
        let handle = service.scoped(ScopedRoot::Cache);

        assert!(handle.read_json::<serde_json::Value>("../escape.json").is_err());
        assert!(handle.read_json::<serde_json::Value>("/abs/path.json").is_err());
        assert!(handle.read_json::<serde_json::Value>("a/../../b.json").is_err());
    }

    #[test]
    fn remove_safe_and_list_scoped() {
        let root = temp_root("list");
        let service = StorageService::new(root);
        let handle = service.scoped(ScopedRoot::Logs);

        handle
            .write_json_atomic("2026/09/log.json", &serde_json::json!({"lines": 3}))
            .expect("write");
        let entries = handle.list_scoped("2026/09").expect("list");
        assert_eq!(entries, vec!["log.json"]);

        handle.remove_safe("2026/09/log.json").expect("remove");
        assert!(handle.list_scoped("2026/09").expect("list").is_empty());
    }
}
