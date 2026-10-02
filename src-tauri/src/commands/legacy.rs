use antares_bridge::protocol::VersionInfo;
use antares_bridge::BridgeError;
use serde::Serialize;

use crate::protocol::error::AntaresError;
use crate::protocol::response::AntaresResponse;
use crate::state::core_state::CoreState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LegacyStatusPayload {
    pub running: bool,
    pub program: Option<String>,
    pub version: Option<VersionInfo>,
    pub crash_count: u64,
}

#[tauri::command]
pub fn legacy_status(state: tauri::State<'_, CoreState>) -> AntaresResponse<LegacyStatusPayload> {
    let legacy = state.legacy();
    AntaresResponse::ok(LegacyStatusPayload {
        running: legacy.status() == antares_bridge::bridge::BridgeStatus::Ready,
        program: legacy.program(),
        version: legacy.version(),
        crash_count: legacy.crash_count(),
    })
}

#[tauri::command]
pub fn legacy_start(state: tauri::State<'_, CoreState>) -> AntaresResponse<VersionInfo> {
    let legacy = state.legacy();
    result_to_response(legacy.start())
}

/// §4.2 — allowlist group method hợp lệ của sidecar (contract §43/§234,
/// khớp registry HANDLERS trong legacy/python/sidecar.py — 23 nhóm).
/// Defense-in-depth cùng CSP: webview không được truyền method lạ;
/// sidecar vẫn là tầng reject cuối (METHOD_NOT_FOUND).
const LEGACY_METHOD_GROUPS: &[&str] = &[
    "health.",
    "app.",
    "instances.",
    "accounts.",
    "java.",
    "versions.",
    "play.",
    "dashboard.",
    "profiles.",
    "mod.",
    "mods.",
    "modpack.",
    "asset.",
    "resource.",
    "visual.",
    "optimization.",
    "system.",
    "net.",
    "runtime.",
    "packet.",
    "console.",
    "repair.",
    "diagnostic.",
];

/// Method hợp lệ: ít nhất `group.name`, mọi segment chỉ [a-z0-9_], không rỗng,
/// và group nằm trong allowlist.
pub fn is_allowed_legacy_method(method: &str) -> bool {
    let parts: Vec<&str> = method.split('.').collect();
    if parts.len() < 2 {
        return false;
    }
    let segments_ok = parts.iter().all(|part| {
        !part.is_empty()
            && part
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
    });
    segments_ok && LEGACY_METHOD_GROUPS.iter().any(|g| method.starts_with(g))
}

/// Gọi method bất kỳ trên sidecar — params/data đều là JSON value.
/// `params` optional: §43 contract cho phép bỏ key (method không cần params),
/// thiếu → chuẩn hóa thành `{}` đúng format request sidecar.
/// Method phải qua allowlist group (§4.2) trước khi chạm sidecar.
#[tauri::command]
pub fn legacy_call(
    state: tauri::State<'_, CoreState>,
    method: String,
    params: Option<serde_json::Value>,
    timeout_ms: Option<u64>,
) -> AntaresResponse<serde_json::Value> {
    if !is_allowed_legacy_method(&method) {
        return AntaresResponse::err(AntaresError::new(
            "METHOD_DENIED",
            format!("legacy method not in allowlist: {method}"),
            false,
        ));
    }
    let legacy = state.legacy();
    let params = params.unwrap_or_else(|| serde_json::json!({}));
    let result = match timeout_ms {
        Some(ms) => legacy.call_with_timeout(&method, params, ms),
        None => legacy.call(&method, params),
    };
    result_to_response(result)
}

#[tauri::command]
pub fn legacy_restart(state: tauri::State<'_, CoreState>) -> AntaresResponse<VersionInfo> {
    let legacy = state.legacy();
    result_to_response(legacy.restart())
}

#[tauri::command]
pub fn legacy_shutdown(state: tauri::State<'_, CoreState>) -> AntaresResponse<serde_json::Value> {
    let legacy = state.legacy();
    result_to_response(legacy.shutdown(std::time::Duration::from_secs(3)).map(|()| serde_json::json!({ "stopped": true })))
}

fn result_to_response<T: Serialize>(result: antares_bridge::BridgeResult<T>) -> AntaresResponse<T> {
    match result {
        Ok(value) => AntaresResponse::ok(value),
        Err(err) => AntaresResponse::err(to_antares(&err)),
    }
}

fn to_antares(err: &BridgeError) -> AntaresError {
    let mut antares = AntaresError::new(err.code(), err.to_string(), err.retryable());
    if matches!(err, BridgeError::Timeout { .. }) {
        antares = antares.with_action("RETRY");
    }
    antares
}

#[cfg(test)]
mod tests {
    use super::is_allowed_legacy_method;

    #[test]
    fn allows_registered_sidecar_groups() {
        assert!(is_allowed_legacy_method("instances.list"));
        assert!(is_allowed_legacy_method("health.version"));
        assert!(is_allowed_legacy_method("mods.quarantine.list"));
        assert!(is_allowed_legacy_method("diagnostic.export"));
        assert!(is_allowed_legacy_method("packet.ingest_2"));
    }

    #[test]
    fn denies_unknown_or_malformed_methods() {
        assert!(!is_allowed_legacy_method("evil.method"));
        assert!(!is_allowed_legacy_method("instances"));
        assert!(!is_allowed_legacy_method(""));
        assert!(!is_allowed_legacy_method(".."));
        assert!(!is_allowed_legacy_method("a..b"));
        assert!(!is_allowed_legacy_method("Instances.List"));
        assert!(!is_allowed_legacy_method("instances.list;rm -rf"));
        assert!(!is_allowed_legacy_method("../etc/passwd.read"));
        assert!(!is_allowed_legacy_method("legacy.restart"));
    }
}
