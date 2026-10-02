use antares_core::events::{EventEnvelope, EventQos, EventTopic, CURRENT_SCHEMA};
use serde::Serialize;

use crate::protocol::response::AntaresResponse;
use crate::state::core_state::CoreState;
use crate::state::storage_mode::StorageMode;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PingPayload {
    pub pong: bool,
    pub ts_ms: u64,
}

/// Health check cơ bản cho typed API layer.
#[tauri::command]
pub fn app_ping() -> AntaresResponse<PingPayload> {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    AntaresResponse::ok(PingPayload { pong: true, ts_ms: ts })
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionPayload {
    pub app: String,
    pub schema: u32,
}

#[tauri::command]
pub fn app_version() -> AntaresResponse<VersionPayload> {
    AntaresResponse::ok(VersionPayload {
        app: env!("CARGO_PKG_VERSION").to_string(),
        schema: CURRENT_SCHEMA,
    })
}

#[tauri::command]
pub fn app_storage_mode() -> AntaresResponse<StorageMode> {
    AntaresResponse::ok(StorageMode::detect())
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TestEventPayload {
    pub delivered: bool,
}

/// Phát một event mẫu qua EventHub → bridge → webview. Test event pipeline end-to-end.
#[tauri::command]
pub fn app_test_event(state: tauri::State<'_, CoreState>) -> AntaresResponse<TestEventPayload> {
    let envelope = EventEnvelope::new(
        EventTopic::App,
        EventQos::Lossless,
        "app.test_event",
        serde_json::json!({ "message": "typed event pipeline works" }),
    );
    state.app.events.publish(envelope);
    AntaresResponse::ok(TestEventPayload { delivered: true })
}
