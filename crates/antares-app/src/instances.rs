//! M4 Batch 05 — InstanceService parity (`services/instances/service.py`).
//!
//! Instance = runtime environment: metadata (`instance.json`) + game dir.
//!
//! - Mọi IO qua `StorageHandle` scoped Instances (§98) — id/name validate
//!   không thoát root (§50): đọc `../` bị sanitize chặn, id select phải
//!   sạch ký tự (hardening — Python sidecar bắt chước được nhưng không làm).
//! - `select` ghi `selectedInstance` vào `config/settings.json` — CÙNG file
//!   `ConfigManager` của sidecar đọc (bootstrap truyền `ANTARES_DATA_DIR`
//!   = data root → Python `data/config/settings.json` = scope Config).
//! - Corrupt `instance.json` bị bỏ qua khi list, trả None khi get (parity
//!   `except Exception: pass`).

use std::sync::atomic::{AtomicU32, Ordering};

use antares_storage::{StorageError, StorageHandle};
use serde::{Deserialize, Serialize};

use crate::error::{codes, AppError, AppResult};

/// Subdir tạo trong `<instances>/<id>/game/` (parity InstanceService.create).
const GAME_SUBDIRS: &[&str] = &[
    "mods",
    "config",
    "resourcepacks",
    "shaderpacks",
    "screenshots",
    "saves",
    "logs",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceMemory {
    pub min_mb: u32,
    pub max_mb: u32,
}

/// Mirror `instance.json` (TS `AntaresInstance` trong types/instances.ts).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instance {
    pub id: String,
    pub name: String,
    pub minecraft_version: String,
    pub loader: String,
    pub directory: String,
    pub memory: InstanceMemory,
    #[serde(default)]
    pub jvm_args: Vec<String>,
    #[serde(default = "default_jvm_preset")]
    pub jvm_preset: String,
    /// Parity: sidecar đặt `"createdAt": instance_id` (string id, không timestamp).
    #[serde(default)]
    pub created_at: Option<String>,
    #[serde(default)]
    pub last_played_at: Option<f64>,
    #[serde(default)]
    pub launch_count: u32,
}

fn default_jvm_preset() -> String {
    "auto".into()
}

/// parity `is_safe_name` (core/utils/paths.py): không traversal, không ký tự cấm.
pub fn is_safe_name(name: &str) -> bool {
    if name.is_empty() || matches!(name, "." | "..") {
        return false;
    }
    // Windows cấm tên kết thúc bằng '.'; tránh ambiguity với traversal.
    if name.ends_with('.') {
        return false;
    }
    !name
        .chars()
        .any(|c| matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
}

/// Id sạch cho path join (hardening §50 — id do mình sinh hoặc từ instance.json cũ).
fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// parity `uuid4().hex[:12]` —12 hex chars. Nguồn: subsec-nanos (8) + pid^seq
/// (4) — không cần crate uuid; 2 create cùng nanos vẫn khác seq.
fn new_instance_id(seq: u32) -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let salt = (u64::from(std::process::id()) ^ u64::from(seq)) & 0xFFFF;
    format!("{nanos:08x}{salt:04x}")
}

/// Store cho group instances (list/get/create/select — parity sidecar handlers).
pub struct InstanceStore {
    /// scope Instances → `<root>/instances`
    instances: StorageHandle,
    /// scope Config → `<root>/config` (settings.json)
    config: StorageHandle,
    seq: AtomicU32,
}

impl InstanceStore {
    pub fn new(instances: StorageHandle, config: StorageHandle) -> Self {
        Self {
            instances,
            config,
            seq: AtomicU32::new(0),
        }
    }

    /// parity `InstanceService.list()` — scan `<scope>/*/instance.json`;
    /// dir không có metadata hoặc json hỏng → bỏ qua (không panic).
    pub fn list(&self) -> AppResult<Vec<Instance>> {
        let mut out = Vec::new();
        for dir in self.instances.list_dirs()? {
            let relative = format!("{dir}/instance.json");
            if let Ok(instance) = self.instances.read_json::<Instance>(&relative) {
                out.push(instance);
            }
        }
        // Thứ tự fs không ổn định — sort theo id cho deterministic (superset parity).
        out.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(out)
    }

    /// parity `InstanceService.get()` — không có / json hỏng / id không an toàn
    /// → None (không leak path ra ngoài).
    pub fn get(&self, id: &str) -> Option<Instance> {
        if !is_safe_id(id) {
            return None;
        }
        self.instances
            .read_json(&format!("{id}/instance.json"))
            .ok()
    }

    /// parity `InstanceService.create()` — validate (presence → safe name),
    /// mkdir game + subdirs, ghi `instance.json` atomic. Defaults áp tại đây
    /// (parity sidecar handler: loader vanilla, 2048/512 MB).
    pub fn create(
        &self,
        name: &str,
        minecraft_version: &str,
        loader: Option<&str>,
        memory_max_mb: Option<u32>,
        memory_min_mb: Option<u32>,
    ) -> AppResult<Instance> {
        let name = name.trim();
        let minecraft_version = minecraft_version.trim();
        if name.is_empty() || minecraft_version.is_empty() {
            return Err(AppError::new(
                codes::CONFIG_INVALID,
                "name and minecraftVersion are required",
            ));
        }
        if !is_safe_name(name) {
            return Err(AppError::new(
                codes::INSTANCE_NAME_INVALID,
                format!("Invalid instance name: {name}"),
            ));
        }

        let seq = self.seq.fetch_add(1, Ordering::Relaxed);
        let id = new_instance_id(seq);
        // parity: mkdir parents + từng subdir game (bỏ lỗi đã tồn — mkdir family).
        self.instances.ensure_dir(&format!("{id}/game"))?;
        for sub in GAME_SUBDIRS {
            self.instances.ensure_dir(&format!("{id}/game/{sub}"))?;
        }

        let directory = self.instances.root().join(&id).display().to_string();
        let instance = Instance {
            id: id.clone(),
            name: name.to_string(),
            minecraft_version: minecraft_version.to_string(),
            loader: loader.unwrap_or("vanilla").to_string(),
            directory,
            memory: InstanceMemory {
                min_mb: memory_min_mb.unwrap_or(512),
                max_mb: memory_max_mb.unwrap_or(2048),
            },
            jvm_args: Vec::new(),
            jvm_preset: default_jvm_preset(),
            created_at: Some(id.clone()),
            last_played_at: None,
            launch_count: 0,
        };
        self.instances
            .write_json_atomic(&format!("{id}/instance.json"), &instance)?;
        Ok(instance)
    }

    /// parity sidecar `instances.select` → `ctx.config.set("selectedInstance",
    /// id, flush_now=True)`: read-modify-write `settings.json` (giữ key khác),
    /// atomic. Id kiểm format sạch (§50) — không kiểm tồn tại (parity).
    pub fn select(&self, id: &str) -> AppResult<String> {
        if !is_safe_id(id) {
            return Err(AppError::new(
                codes::CONFIG_INVALID,
                format!("invalid instanceId: {id:?}"),
            ));
        }
        let mut data: serde_json::Value = match self.config.read_json("settings.json") {
            Ok(value) => value,
            Err(StorageError::Io { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                serde_json::json!({})
            }
            Err(StorageError::Json { path, .. }) => {
                // Parity ConfigManager: file hỏng → reset defaults + set (repair cũng vậy).
                log::warn!("settings.json unreadable ({path}) — reset trước khi set selectedInstance");
                serde_json::json!({})
            }
            Err(err) => return Err(err.into()),
        };
        if !data.is_object() {
            data = serde_json::json!({});
        }
        data["selectedInstance"] = serde_json::json!(id);
        self.config.write_json_atomic("settings.json", &data)?;
        Ok(id.to_string())
    }

    /// Đọc `selectedInstance` hiện tại (dashboard/test parity `config.get`).
    pub fn selected_instance(&self) -> Option<String> {
        self.config
            .read_json::<serde_json::Value>("settings.json")
            .ok()?
            .get("selectedInstance")?
            .as_str()
            .map(String::from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use antares_storage::{ScopedRoot, StorageService};

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("antares-inst-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn store(root: &std::path::Path) -> InstanceStore {
        let service = StorageService::new(root);
        InstanceStore::new(
            service.scoped(ScopedRoot::Instances),
            service.scoped(ScopedRoot::Config),
        )
    }

    #[test]
    fn safe_name_rules_match_python() {
        // parity core/utils/paths.py is_safe_name
        assert!(is_safe_name("My Vanilla"));
        assert!(is_safe_name("thế giới ❤"));
        assert!(!is_safe_name(""));
        assert!(!is_safe_name("."));
        assert!(!is_safe_name(".."));
        assert!(!is_safe_name("trailing."));
        assert!(!is_safe_name("a/b"));
        assert!(!is_safe_name("a\\b"));
        assert!(!is_safe_name("a:b*c?d\"e<f>g|h"));
    }

    #[test]
    fn create_builds_dirs_and_metadata_parity() {
        let root = temp_root("create");
        let store = store(&root);

        let instance = store
            .create("My Vanilla", "1.21.11", None, None, None)
            .expect("create");
        assert_eq!(instance.name, "My Vanilla");
        assert_eq!(instance.minecraft_version, "1.21.11");
        assert_eq!(instance.loader, "vanilla");
        assert_eq!(instance.memory.min_mb, 512);
        assert_eq!(instance.memory.max_mb, 2048);
        assert_eq!(instance.jvm_args, Vec::<String>::new());
        assert_eq!(instance.jvm_preset, "auto");
        assert_eq!(instance.last_played_at, None);
        assert_eq!(instance.launch_count, 0);
        // createdAt = id (parity sidecar/service).
        assert_eq!(instance.created_at.as_deref(), Some(instance.id.as_str()));
        assert_eq!(instance.id.len(), 12, "uuid4 hex[:12] parity: {}", instance.id);
        assert!(instance.id.chars().all(|c| c.is_ascii_hexdigit()));
        assert!(instance.directory.ends_with(&instance.id));

        // game dir + đủ 7 subdir (parity GAME_SUBDIRS).
        let game = root.join("instances").join(&instance.id).join("game");
        for sub in ["mods", "config", "resourcepacks", "shaderpacks", "screenshots", "saves", "logs"] {
            assert!(game.join(sub).is_dir(), "thiếu subdir {sub}");
        }
        assert!(root.join("instances").join(&instance.id).join("instance.json").is_file());

        // list/get roundtrip.
        let listed = store.list().expect("list");
        assert_eq!(listed, vec![instance.clone()]);
        assert_eq!(store.get(&instance.id), Some(instance));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn create_validates_presence_then_name_parity() {
        let root = temp_root("validate");
        let store = store(&root);

        // Sidecar thứ tự: presence (CONFIG_INVALID) trước safe-name.
        let err = store.create("", "1.21", None, None, None).expect_err("empty name");
        assert_eq!(err.code, codes::CONFIG_INVALID);
        let err = store.create("name", "  ", None, None, None).expect_err("empty version");
        assert_eq!(err.code, codes::CONFIG_INVALID);
        let err = store.create("bad/name", "1.21", None, None, None).expect_err("unsafe name");
        assert_eq!(err.code, codes::INSTANCE_NAME_INVALID);

        // Defaults khi truyền options (parity sidecar params).
        let custom = store
            .create("Custom", "1.20.1", Some("fabric"), Some(4096), Some(1024))
            .expect("create");
        assert_eq!(custom.loader, "fabric");
        assert_eq!(custom.memory.min_mb, 1024);
        assert_eq!(custom.memory.max_mb, 4096);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn list_skips_corrupt_and_dirs_without_meta_get_none() {
        let root = temp_root("corrupt");
        let store = store(&root);
        let good = store.create("Good", "1.21", None, None, None).expect("create");

        // Dir không có instance.json + file json hỏng → bỏ qua (parity).
        std::fs::create_dir_all(root.join("instances/stray")).unwrap();
        std::fs::write(root.join("instances/broken/instance.json"), b"{not json").ok();
        std::fs::create_dir_all(root.join("instances/broken")).unwrap();

        let listed = store.list().expect("list");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, good.id);

        assert!(store.get("stray").is_none());
        assert!(store.get("broken").is_none());
        // Id không an toàn → None, không đọc ra ngoài root (§50).
        assert!(store.get("../escape").is_none());
        assert!(store.get("").is_none());
        assert!(store.get("missing").is_none());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn select_writes_settings_and_keeps_other_keys() {
        let root = temp_root("select");
        let store = store(&root);

        // File chưa có → tạo mới.
        assert_eq!(store.select("abc123").expect("select"), "abc123");
        assert_eq!(store.selected_instance().as_deref(), Some("abc123"));

        // Giữ key khác khi merge (parity ConfigManager set).
        std::fs::write(
            root.join("config/settings.json"),
            br#"{"schemaVersion":3,"theme":"dark"}"#,
        )
        .unwrap();
        store.select("def456").expect("select2");
        let raw = std::fs::read_to_string(root.join("config/settings.json")).expect("read");
        let data: serde_json::Value = serde_json::from_str(&raw).expect("json");
        assert_eq!(data["selectedInstance"], "def456");
        assert_eq!(data["theme"], "dark");
        assert_eq!(data["schemaVersion"], 3);

        // Id không sạch → typed error, không ghi gì.
        let err = store.select("../evil").expect_err("traversal");
        assert_eq!(err.code, codes::CONFIG_INVALID);
        assert_eq!(store.selected_instance().as_deref(), Some("def456"));

        // settings.json hỏng → reset + set (parity ConfigManager đọc lại defaults).
        std::fs::write(root.join("config/settings.json"), b"broken{").unwrap();
        store.select("abc123").expect("select after corrupt");
        assert_eq!(store.selected_instance().as_deref(), Some("abc123"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
