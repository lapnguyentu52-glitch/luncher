use std::sync::Arc;

use antares_storage::{ScopedRoot, StorageHandle, StorageService};

use crate::error::CoreResult;
use crate::events::EventHub;
use crate::tasks::TaskRegistry;

/// §88.1 — Typed AppState. Thay pattern `ctx.set("runtime", ...)` string-key
/// bằng struct có type rõ ràng. Mỗi service là một field.
pub struct AppState {
    pub events: Arc<EventHub>,
    pub tasks: Arc<TaskRegistry>,
    storage: StorageService,
}

pub struct AppStateHandles {
    pub app_data: StorageHandle,
    pub cache: StorageHandle,
    pub logs: StorageHandle,
    pub profiles: StorageHandle,
}

impl AppState {
    pub fn new(storage_root: impl Into<std::path::PathBuf>) -> Self {
        Self {
            events: Arc::new(EventHub::new()),
            tasks: Arc::new(TaskRegistry::new()),
            storage: StorageService::new(storage_root),
        }
    }

    /// Cấp storage handles scoped — mỗi nhóm service chỉ thấy root cần thiết (§98.2).
    pub fn handles(&self) -> AppStateHandles {
        AppStateHandles {
            app_data: self.storage.scoped(ScopedRoot::AppData),
            cache: self.storage.scoped(ScopedRoot::Cache),
            logs: self.storage.scoped(ScopedRoot::Logs),
            profiles: self.storage.scoped(ScopedRoot::Profiles),
        }
    }

    /// Read một JSON config qua scoped AppData root.
    pub fn read_config<T: serde::de::DeserializeOwned>(&self, relative: &str) -> CoreResult<Option<T>> {
        let handle = self.storage.scoped(ScopedRoot::AppData);
        match handle.read_json::<T>(relative) {
            Ok(value) => Ok(Some(value)),
            Err(antares_storage::StorageError::Io { .. }) => Ok(None), // chưa tồn tại
            Err(err) => Err(err.into()),
        }
    }

    /// Write một JSON config atomically qua scoped AppData root.
    pub fn write_config<T: serde::Serialize>(&self, relative: &str, value: &T) -> CoreResult<()> {
        let handle = self.storage.scoped(ScopedRoot::AppData);
        handle.write_json_atomic(relative, value)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "antares-core-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("mkdir");
        dir
    }

    #[test]
    fn config_roundtrip_and_missing_is_none() {
        let root = temp_root();
        let state = AppState::new(root.clone());

        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct Theme {
            accent: String,
        }

        let missing: Option<Theme> = state.read_config("ui/theme.json").expect("read missing");
        assert!(missing.is_none());

        state
            .write_config("ui/theme.json", &Theme { accent: "red".into() })
            .expect("write");
        let loaded: Option<Theme> = state.read_config("ui/theme.json").expect("read");
        assert_eq!(loaded, Some(Theme { accent: "red".into() }));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn events_and_tasks_are_shared_arcs() {
        let root = temp_root();
        let state = AppState::new(root.clone());
        state.events.publish(crate::events::EventEnvelope::new(
            crate::events::EventTopic::App,
            crate::events::EventQos::Batched,
            "app.boot",
            serde_json::json!({}),
        ));
        assert_eq!(state.events.stats().published, 1);
        let task = state
            .tasks
            .spawn("boot", crate::tasks::TaskPriority::P0Critical, None)
            .expect("spawn");
        assert_eq!(task.state, crate::tasks::TaskState::Queued);
        let _ = std::fs::remove_dir_all(&root);
    }
}
