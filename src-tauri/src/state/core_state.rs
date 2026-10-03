use std::path::PathBuf;
use std::sync::Arc;

use antares_bridge::LegacyBridge;
use antares_core::AppState;

/// Managed Tauri state — bọc typed AppState của core crate + legacy bridge.
/// `LegacyBridge::new()` trả `Arc<Self>` với interior mutability (Mutex/RwLock
/// trong crate), nên chỉ cần giữ Arc — không wrap thêm Mutex.
pub struct CoreState {
    pub app: Arc<AppState>,
    /// F-14 Batch 04 — composition root: command đi qua AppServices, không
    /// tự new service trong handler.
    services: Arc<antares_app::AppServices>,
    legacy_bridge: Arc<LegacyBridge>,
}

impl CoreState {
    /// §218 — storage root: portable mode ghi cạnh exe, installed ghi theo OS data dir.
    pub fn resolve(storage_mode: crate::state::storage_mode::StorageMode) -> Self {
        let root = match storage_mode {
            crate::state::storage_mode::StorageMode::Portable => {
                // §218 — data cạnh exe; ANTARES_PORTABLE_DIR override cho dev/test.
                std::env::var("ANTARES_PORTABLE_DIR").map(PathBuf::from).unwrap_or_else(|_| {
                    std::env::current_exe()
                        .ok()
                        .and_then(|exe| exe.parent().map(|dir| dir.join("data")))
                        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default().join("data"))
                })
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

        // F-14/M5 — composition root tạo trước: runtime flags (ANTA_RUST_ONLY)
        // quyết định có cấu hình sidecar hay không.
        let services = Arc::new(antares_app::AppServices::new(root.clone()));

        // §234 — legacy sidecar program: env override cho dev (vd:
        // "python3 legacy/python/sidecar.py"), sau bundle là externalBin.
        let bridge = LegacyBridge::new();
        if services.flags().rust_only {
            // M5 — rust-only: không set program/env cho sidecar → không bao giờ
            // auto-start Python; lệnh legacy_* mutate trả LEGACY_DISABLED ở
            // command layer (double guard).
            log::info!("rust-only mode (ANTA_RUST_ONLY=1): sidecar auto-start disabled");
        } else {
            let program = std::env::var("ANTARES_LEGACY_SIDECAR").ok().or_else(|| {
                // externalBin: cạnh exe — NSIS strip hậu tố triple khi cài
                // (antares-legacy.exe), portable giữ triple (antares-legacy-<triple>.exe).
                let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
                sidecar_candidates().iter().find_map(|name| {
                    let p = exe_dir.join(name);
                    p.exists().then(|| p.to_string_lossy().into_owned())
                })
            });
            match program {
                Some(program) => {
                    log::info!("legacy sidecar configured: {program}");
                    bridge.set_program(program);
                }
                None => {
                    log::info!("legacy sidecar not configured (ANTARES_LEGACY_SIDECAR unset, none next to exe)");
                }
            }
            // Mục 5.1 — truyền data dir cho sidecar để Rust/Python thấy cùng dữ liệu.
            // ANTARES_ROOT = cạnh exe để portable.flag khớp §218.
            bridge.set_env("ANTARES_DATA_DIR", root.to_string_lossy().into_owned());
            if let Some(exe_dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(PathBuf::from)) {
                bridge.set_env("ANTARES_ROOT", exe_dir.to_string_lossy().into_owned());
            }
        }

        Self {
            app: Arc::new(AppState::new(root)),
            services,
            legacy_bridge: bridge,
        }
    }

    /// Accessor legacy bridge — trả Arc để clone vào thread/blocking task.
    pub fn legacy(&self) -> Arc<LegacyBridge> {
        Arc::clone(&self.legacy_bridge)
    }

    /// Composition root (F-14) — trả Arc để command clone vào thread/blocking.
    pub fn services(&self) -> Arc<antares_app::AppServices> {
        Arc::clone(&self.services)
    }
}

/// Tên sidecar cạnh exe: tên trần (NSIS đã strip triple) trước, rồi các triple phổ biến
/// (bản portable giữ hậu tố triple từ externalBin).
fn sidecar_candidates() -> Vec<String> {
    let mut names = vec!["antares-legacy.exe".to_string(), "antares-legacy".to_string()];
    #[cfg(windows)]
    names.extend(
        ["x86_64", "i686", "aarch64"]
            .iter()
            .map(|arch| format!("antares-legacy-{arch}-pc-windows-msvc.exe")),
    );
    #[cfg(all(unix, not(target_os = "macos")))]
    names.extend(
        ["x86_64", "aarch64"]
            .iter()
            .map(|arch| format!("antares-legacy-{arch}-unknown-linux-gnu")),
    );
    #[cfg(target_os = "macos")]
    names.extend(
        ["x86_64", "aarch64"]
            .iter()
            .map(|arch| format!("antares-legacy-{arch}-apple-darwin")),
    );
    names
}

/// Data dir theo platform mà không thêm dependency: %APPDATA% (win), XDG_DATA_HOME (linux),
/// ~/Library/Application Support (mac).
fn dirs_data_root() -> Option<PathBuf> {
    #[allow(unused_variables)]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_candidates_prefers_plain_name_then_triple() {
        let names = sidecar_candidates();
        assert_eq!(names[0], "antares-legacy.exe");
        assert_eq!(names[1], "antares-legacy");
        assert!(
            names.iter().any(|n| n.starts_with("antares-legacy-")),
            "cần biến thể triple cho portable bundle: {names:?}"
        );
    }
}
