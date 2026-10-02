use std::path::PathBuf;
use std::sync::Arc;

use antares_bridge::LegacyBridge;
use antares_core::AppState;

/// Managed Tauri state — bọc typed AppState của core crate + legacy bridge.
/// `LegacyBridge::new()` trả `Arc<Self>` với interior mutability (Mutex/RwLock
/// trong crate), nên chỉ cần giữ Arc — không wrap thêm Mutex.
pub struct CoreState {
    pub app: Arc<AppState>,
    legacy_bridge: Arc<LegacyBridge>,
}

impl CoreState {
    /// §218 — storage root: portable mode ghi cạnh exe, installed ghi theo OS data dir.
    pub fn resolve(storage_mode: crate::state::storage_mode::StorageMode) -> Self {
        let root = match storage_mode {
            crate::state::storage_mode::StorageMode::Portable => {
                // ANTARES_PORTABLE_DIR nếu set, fallback cwd/data
                std::env::var("ANTARES_PORTABLE_DIR")
                    .map(PathBuf::from)
                    .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default().join("data"))
            }
            crate::state::storage_mode::StorageMode::Installed => {
                if let Some(dir) = dirs_data_root() {
                    dir.join("Antares")
                } else {
                    std::env::temp_dir().join("antares-fallback-data")
                }
            }
        };
        if let Err(err) = std::fs::create_dir_all(&root) {
            log::warn!("cannot create storage root {:?}: {err}", root);
        }
        log::info!("storage root: {:?}", root);

        // §234 — legacy sidecar program: env override cho dev, sau bundle là externalBin.
        let bridge = LegacyBridge::new();
        if let Ok(program) = std::env::var("ANTARES_LEGACY_SIDECAR") {
            log::info!("legacy sidecar configured: {program}");
            bridge.set_program(program);
        } else {
            log::info!("legacy sidecar not configured (ANTARES_LEGACY_SIDECAR unset)");
        }

        Self {
            app: Arc::new(AppState::new(root)),
            legacy_bridge: bridge,
        }
    }

    /// Accessor legacy bridge — mọi method đều `&self` (interior mutability).
    pub fn legacy(&self) -> &LegacyBridge {
        &self.legacy_bridge
    }
}

/// Data dir theo platform mà không thêm dependency: %APPDATA% (win), XDG_DATA_HOME (linux),
/// ~/Library/Application Support (mac).
fn dirs_data_root() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA").ok().map(PathBuf::from)
    }
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME")
            .ok()
            .map(|h| PathBuf::from(h).join("Library/Application Support"))
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        std::env::var("XDG_DATA_HOME")
            .ok()
            .map(PathBuf::from)
            .or_else(|| std::env::var("HOME").ok().map(|h| PathBuf::from(h).join(".local/share")))
    }
}
