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

/// Gọi method bất kỳ trên sidecar — params/data đều là JSON value.
/// `params` optional: §43 contract cho phép bỏ key (method không cần params),
/// thiếu → chuẩn hóa thành `{}` đúng format request sidecar.
#[tauri::command]
pub fn legacy_call(
    state: tauri::State<'_, CoreState>,
    method: String,
    params: Option<serde_json::Value>,
    timeout_ms: Option<u64>,
) -> AntaresResponse<serde_json::Value> {
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
