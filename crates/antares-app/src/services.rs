//! F-14 / M3 — composition root: mọi service native của app gặp nhau tại đây.

use std::path::PathBuf;
use std::sync::Mutex;

use antares_process::ProcessRegistry;
use antares_storage::{ScopedRoot, StorageService};

use crate::accounts::AccountStore;
use crate::error::AppResult;
use crate::instances::{Instance, InstanceStore};
use crate::play::PreflightReport;
use crate::runtime::RuntimeFlags;

/// Managed service container (Batch 04). Command nhận `Arc<AppServices>` từ
/// managed state — không tự new service, không đụng filesystem trực tiếp
/// (§98 mọi IO qua `StorageService`). Mutex chỉ nằm trong đây, không rò ra ngoài.
pub struct AppServices {
    storage: StorageService,
    processes: Mutex<ProcessRegistry>,
    instances: InstanceStore,
    accounts: AccountStore,
    flags: RuntimeFlags,
}

impl AppServices {
    /// Đọc runtime flags từ env thật (`ANTA_RUST_ONLY`) — test dùng `with_flags`
    /// để không phụ thuộc global state.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::with_flags(root, RuntimeFlags::from_env())
    }

    pub fn with_flags(root: impl Into<PathBuf>, flags: RuntimeFlags) -> Self {
        let storage = StorageService::new(root);
        let instances = InstanceStore::new(
            storage.scoped(ScopedRoot::Instances),
            storage.scoped(ScopedRoot::Config),
        );
        let accounts = AccountStore::new(storage.scoped(ScopedRoot::Config));
        Self {
            storage,
            processes: Mutex::new(ProcessRegistry::default()),
            instances,
            accounts,
            flags,
        }
    }

    /// Runtime flags (M5) — command/legacy guard đọc từ đây.
    pub fn flags(&self) -> RuntimeFlags {
        self.flags
    }

    /// Group instances (Batch 05) — parity sidecar `instances.*` handlers.
    pub fn instances(&self) -> &InstanceStore {
        &self.instances
    }

    /// Group accounts (Batch B15.2) — parity sidecar `accounts.*` handlers.
    pub fn accounts(&self) -> &AccountStore {
        &self.accounts
    }

    /// Group play (Batch 07a) — parity sidecar `play.preflight`:
    /// scan JRE thật rồi chạy 5 check thuần (java/version/account/disk/mods).
    pub fn play_preflight(&self, instance_id: &str) -> AppResult<PreflightReport> {
        self.play_preflight_with_javas(instance_id, &antares_java::scan_java_infos())
    }

    /// Biến thể inject javas — test deterministic không phụ thuộc JRE trên
    /// máy chạy test (parity sidecar test mock `scan_system_java`).
    pub(crate) fn play_preflight_with_javas(
        &self,
        instance_id: &str,
        javas: &[antares_java::JavaInfo],
    ) -> AppResult<PreflightReport> {
        let instance: Instance = self.instances.get(instance_id).ok_or_else(|| {
            crate::error::AppError::new(
                crate::error::codes::INSTANCE_NOT_FOUND,
                format!("instance not found: {instance_id}"),
            )
        })?;
        let account = self.accounts.selected_account();
        let instances_dir = self.storage.root().join(ScopedRoot::Instances.dir_name());
        Ok(crate::play::preflight(
            &instance,
            account.as_ref(),
            javas,
            &instances_dir,
        ))
    }

    pub fn storage(&self) -> &StorageService {
        &self.storage
    }

    /// Logs root cho LogMux scoped (F-12): `<root>/logs/<scope>.log`.
    pub fn logs_root(&self) -> PathBuf {
        self.storage.root().join(ScopedRoot::Logs.dir_name())
    }

    /// §113 — registry expose qua closure trên `&mut` (không cho mượn Mutex).
    pub fn with_processes<F, R>(&self, f: F) -> R
    where
        F: FnOnce(&mut ProcessRegistry) -> R,
    {
        let mut guard = self.processes.lock().unwrap_or_else(|err| err.into_inner());
        f(&mut guard)
    }

    /// Tạo scoped root còn thiếu (idempotent) rồi trả báo cáo trạng thái —
    /// command `app_storage_info` trả thẳng kết quả này. Error path thật:
    /// root không ghi được → `StorageError::Io` → `STORAGE_WRITE_FAILED` (§117).
    pub fn ensure_storage(&self) -> AppResult<StorageReport> {
        let mut scopes = Vec::with_capacity(ScopedRoot::ALL.len());
        for scoped in ScopedRoot::ALL {
            let path = self.storage.root().join(scoped.dir_name());
            std::fs::create_dir_all(&path).map_err(|source| {
                antares_storage::StorageError::Io {
                    path: path.display().to_string(),
                    source,
                }
            })?;
            scopes.push(ScopeInfo {
                dir: scoped.dir_name().to_string(),
                exists: path.is_dir(),
            });
        }
        Ok(StorageReport {
            root: self.storage.root().display().to_string(),
            scopes,
        })
    }
}

/// Snapshot storage cho UI/diagnostics (mirror TS `StorageInfoPayload`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageReport {
    pub root: String,
    pub scopes: Vec<ScopeInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScopeInfo {
    pub dir: String,
    pub exists: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::codes;

    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("antares-app-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    #[test]
    fn ensure_storage_creates_all_scopes_and_reports() {
        let root = temp_root("storage");
        let services = AppServices::new(&root);

        let report = services.ensure_storage().expect("ensure");
        assert_eq!(report.root, root.display().to_string());
        assert_eq!(report.scopes.len(), ScopedRoot::ALL.len());
        assert!(report.scopes.iter().all(|s| s.exists), "vừa tạo xong phải tồn tại");
        let dirs: Vec<&str> = report.scopes.iter().map(|s| s.dir.as_str()).collect();
        assert_eq!(
            dirs,
            vec!["app-data", "config", "cache", "logs", "profiles", "instances", "backups"]
        );
        assert!(root.join("logs").is_dir());

        // Idempotent — gọi lại không lỗi, vẫn report đủ.
        let again = services.ensure_storage().expect("re-ensure");
        assert_eq!(again, report);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn ensure_storage_unwritable_root_maps_storage_write_failed() {
        // Root là FILE → create_dir_all mọi scope đều fail → typed error §117.
        let root = temp_root("not-a-dir");
        std::fs::write(&root, b"i am a file").unwrap();
        let services = AppServices::new(&root);

        let err = services.ensure_storage().expect_err("phải fail");
        assert_eq!(err.code, codes::STORAGE_WRITE_FAILED);
        assert!(err.retryable);
        assert!(err.message.contains("io error"), "{}", err.message);

        let _ = std::fs::remove_file(&root);
    }

    #[test]
    fn logs_root_is_scoped_under_root() {
        let root = temp_root("logs-root");
        let services = AppServices::new(&root);
        assert_eq!(services.logs_root(), root.join("logs"));
        assert_eq!(services.storage().root(), root.as_path());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn with_processes_runs_closure_on_registry() {
        let services = AppServices::new(temp_root("proc"));
        let id = services.with_processes(|registry| {
            registry.register("minecraft", Some("inst-1".into()), "java -jar g.jar", Default::default(), 1_000)
        });
        let pid_set = services.with_processes(|registry| registry.set_pid(&id, 4242).is_ok());
        assert!(pid_set);
        let owner = services.with_processes(|registry| {
            registry.get(&id).map(|record| record.owner.clone())
        });
        assert_eq!(owner.as_deref(), Some("minecraft"));
    }

    #[test]
    fn flags_carried_through_services() {
        let root = temp_root("flags");
        let rust_only = AppServices::with_flags(&root, RuntimeFlags { rust_only: true });
        assert!(rust_only.flags().rust_only);

        let normal = AppServices::with_flags(&root, RuntimeFlags::default());
        assert!(!normal.flags().rust_only);
        let _ = std::fs::remove_dir_all(&root);
    }

    // Batch 07a — play_preflight qua composition root.

    #[test]
    fn play_preflight_missing_instance_is_instance_not_found() {
        let services = AppServices::new(temp_root("preflight-404"));
        let err = services
            .play_preflight_with_javas("ghost", &[])
            .expect_err("instance không tồn tại → typed error");
        assert_eq!(err.code, codes::INSTANCE_NOT_FOUND);
        assert!(!err.retryable);
        assert_eq!(err.message, "instance not found: ghost"); // parity sidecar
    }

    #[test]
    fn play_preflight_full_flow_with_injected_javas() {
        let root = temp_root("preflight-flow");
        let services = AppServices::new(&root);
        let instance = services
            .instances()
            .create("Demo", "1.21.11", Some("fabric"), None, None)
            .expect("create");

        // account = settings.json (cùng file sidecar ConfigManager)
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::write(
            root.join("config/settings.json"),
            r#"{"accounts":[{"id":"a1","displayName":"Steve","token":"secret"}],"selectedAccount":"a1"}"#,
        )
        .unwrap();

        let javas = [antares_java::JavaInfo {
            path: "/j".into(),
            exe: "/j/bin/java".into(),
            javaw: None,
            major: 21,
            name: "jdk21".into(),
        }];
        let report = services
            .play_preflight_with_javas(&instance.id, &javas)
            .expect("preflight");

        assert_eq!(report.instance_id, instance.id);
        assert!(report.can_play);
        assert_eq!(report.blockers, 0);
        let account = report.checks.iter().find(|c| c.id == "account").unwrap();
        assert_eq!(account.detail, "Steve"); // secret strip: token không lộ
        assert_eq!(report.checks[0].label, "Java ≥ 21");

        let _ = std::fs::remove_dir_all(&root);
    }
}
