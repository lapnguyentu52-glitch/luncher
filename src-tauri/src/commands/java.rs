//! B15.2 — java/versions command surface (parity sidecar `java.*` + `versions.*`).
//!
//! - `java_list`: scan system JREs qua `antares_java::discovery` — payload key
//!   `javas` (contract sidecar `handle_java_list` = `{"javas": infos}` giữ nguyên,
//!   PARITY evidence #3: TS types không sửa).
//! - `versions_list`: vanilla → Mojang manifest qua `antares_net::manifest`;
//!   Batch 07c → fabric (stable MC) / forge (maven) qua
//!   `antares_app::install::list_loader_versions` (parity FabricProvider /
//!   ForgeProvider `list_versions` — trả `list[str]`); loader lạ →
//!   `VALIDATION_FAILED` (parity `get_loader`).
//!
//! Handler không làm business logic — chỉ map request → crate → response §88.2.

use antares_app::{codes, AppError};
use antares_java::JavaInfo;
use serde::Serialize;

use crate::protocol::response::AntaresResponse;
use crate::state::core_state::CoreState;

// ---------------------------------------------------------------------------
// java.list
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaListPayload {
    /// parity `java.list` — sidecar trả `{"javas": [...]}`.
    pub javas: Vec<JavaInfo>,
}

/// parity `java.list` — scan system Java (JAVA_HOME + PATH + known dirs),
/// detect_major qua `java -showversion`. Blocking I/O — chấp nhận (discovery
/// chạy nhanh ≤ vài giây; async Tokio dành khi thực sự cần per audit §12).
#[tauri::command]
pub fn java_list(_state: tauri::State<'_, CoreState>) -> AntaresResponse<JavaListPayload> {
    AntaresResponse::ok(JavaListPayload {
        javas: antares_java::scan_java_infos(),
    })
}

// ---------------------------------------------------------------------------
// versions.list
// ---------------------------------------------------------------------------

#[derive(Serialize)]
#[serde(untagged)]
pub enum VersionsVersionEntry {
    /// Vanilla — Mojang manifest entry (contract sidecar `mlu.get_available_versions`).
    Full {
        id: String,
        r#type: String,
        release_time: String,
        url: String,
    },
    /// Fabric/Forge — chuỗi version (parity `list_versions` trả `list[str]`):
    /// fabric = stable MC version, forge = `{mc}-{build}`.
    Id(String),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionsListPayload {
    pub versions: Vec<VersionsVersionEntry>,
}

/// parity `versions.list` — Mojang manifest với DiskCache TTL 30 phút +
/// stale fallback offline (antares-net phase 4). Cache path qua storage root.
/// Batch 07c — loader fabric/forge: metadata list (parity provider list_versions).
#[tauri::command]
pub fn versions_list(
    state: tauri::State<'_, CoreState>,
    loader: Option<String>,
) -> AntaresResponse<VersionsListPayload> {
    let services = state.services();
    let cache_dir = services.storage().root().join("cache");
    let timeout = std::time::Duration::from_secs(10);

    // parity `handle_versions_list` → get_loader(loader): loader không hỗ trợ
    // → VALIDATION_FAILED (§117 — AppError tự chốt retryable=false).
    if let Some(loader) = loader.as_deref().filter(|l| !l.is_empty()) {
        if loader != "vanilla" {
            if matches!(loader, "fabric" | "forge") {
                // parity FabricProvider/ForgeProvider.list_versions — network
                // thật, cache không cần (metadata list nhỏ).
                return match antares_app::install::list_loader_versions(
                    loader,
                    &antares_app::install::Endpoints::default(),
                ) {
                    Ok(versions) => AntaresResponse::ok(VersionsListPayload {
                        versions: versions
                            .into_iter()
                            .map(VersionsVersionEntry::Id)
                            .collect(),
                    }),
                    Err(err) => AntaresResponse::from_result(Err(err)),
                };
            }
            return AntaresResponse::from_result(Err(AppError::new(
                codes::VALIDATION_FAILED,
                format!(
                    "unsupported loader: {loader} (native versions.list: vanilla/fabric/forge)"
                ),
            )));
        }
    }

    let cache = antares_net::ManifestCache::new(&cache_dir);

    match antares_net::get_manifest(Some(&cache), timeout, true) {
        Ok(Some(entries)) => {
            let versions = entries
                .into_iter()
                .map(|v| VersionsVersionEntry::Full {
                    id: v.id,
                    r#type: v.r#type,
                    release_time: v.release_time,
                    url: v.url,
                })
                .collect();
            AntaresResponse::ok(VersionsListPayload { versions })
        }
        Ok(None) => {
            // Cache miss + network unavailable → trả danh sách rỗng (không crash UI)
            AntaresResponse::ok(VersionsListPayload { versions: Vec::new() })
        }
        // NETWORK_UNAVAILABLE — retryable=true theo catalog §117 (AppError tự chốt).
        Err(err) => AntaresResponse::from_result(Err(AppError::new(
            codes::NETWORK_UNAVAILABLE,
            err.to_string(),
        ))),
    }
}
