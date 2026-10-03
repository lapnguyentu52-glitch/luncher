use std::sync::Arc;

use antares_core::tasks::TaskPriority;
use antares_core::{AppState, CoreError};
use serde::Serialize;

use crate::protocol::response::AntaresResponse;
use crate::state::core_state::CoreState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CoreStatusPayload {
    pub storage_mode: crate::state::storage_mode::StorageMode,
    pub uptime_ms: u64,
    pub hub: antares_core::events::HubStats,
    pub active_tasks: Vec<antares_core::tasks::Task>,
    /// M5 — `ANTA_RUST_ONLY=1`: UI biết mà chuyển flow native thay vì legacy_call.
    pub rust_only: bool,
}

static STARTED: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

fn started() -> &'static std::time::Instant {
    STARTED.get_or_init(std::time::Instant::now)
}

#[tauri::command]
pub fn core_status(state: tauri::State<'_, CoreState>) -> AntaresResponse<CoreStatusPayload> {
    AntaresResponse::ok(CoreStatusPayload {
        storage_mode: crate::state::storage_mode::StorageMode::detect(),
        uptime_ms: started().elapsed().as_millis() as u64,
        hub: state.app.events.stats(),
        active_tasks: state.app.tasks.active(),
        rust_only: state.services().flags().rust_only,
    })
}

#[tauri::command]
pub fn core_spawn_task(
    state: tauri::State<'_, CoreState>,
    kind: String,
    priority: String,
    dedupe_key: Option<String>,
) -> AntaresResponse<antares_core::tasks::Task> {
    let priority = match priority.to_ascii_uppercase().as_str() {
        "P0" | "P0_CRITICAL" => TaskPriority::P0Critical,
        "P2" | "P2_BACKGROUND" => TaskPriority::P2Background,
        "P3" | "P3_PREFETCH" => TaskPriority::P3Prefetch,
        _ => TaskPriority::P1UserAction,
    };
    let storage = &state.app;
    let result: Result<antares_core::tasks::Task, CoreError> =
        storage.tasks.spawn(kind, priority, dedupe_key);
    match result {
        Ok(task) => AntaresResponse::ok(task),
        Err(err) => AntaresResponse::err(crate::protocol::error::AntaresError::new(
            err.code(),
            err.to_string(),
            false,
        )),
    }
}

#[tauri::command]
pub fn core_task_progress(
    state: tauri::State<'_, CoreState>,
    task_id: String,
    progress: f64,
    message: Option<String>,
) -> AntaresResponse<antares_core::tasks::Task> {
    run_core(&state.app, |app| app.tasks.set_progress(&task_id, progress, message))
}

#[tauri::command]
pub fn core_complete_task(
    state: tauri::State<'_, CoreState>,
    task_id: String,
) -> AntaresResponse<antares_core::tasks::Task> {
    run_core(&state.app, |app| app.tasks.complete(&task_id))
}

#[tauri::command]
pub fn core_cancel_task(
    state: tauri::State<'_, CoreState>,
    task_id: String,
) -> AntaresResponse<antares_core::tasks::Task> {
    run_core(&state.app, |app| app.tasks.request_cancel(&task_id))
}

fn run_core<T: Serialize>(
    app: &Arc<AppState>,
    op: impl FnOnce(&Arc<AppState>) -> Result<T, CoreError>,
) -> AntaresResponse<T> {
    match op(app) {
        Ok(value) => AntaresResponse::ok(value),
        Err(err) => AntaresResponse::err(crate::protocol::error::AntaresError::new(
            err.code(),
            err.to_string(),
            false,
        )),
    }
}
