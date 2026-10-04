//! Batch 07a — play command surface (parity sidecar `play.preflight` /
//! `play.launch`).

use antares_app::play::PreflightReport;
use serde::Serialize;

use crate::protocol::response::AntaresResponse;
use crate::state::core_state::CoreState;

/// parity `play.preflight` — 5 check (java/version/account/disk/mods),
/// instance thiếu → typed `INSTANCE_NOT_FOUND`. Handler chỉ map request →
/// service → response (§88.2); logic nằm trong `AppServices::play_preflight`.
#[tauri::command]
pub fn play_preflight(
    state: tauri::State<'_, CoreState>,
    instance_id: String,
) -> AntaresResponse<PreflightReport> {
    AntaresResponse::from_result(state.services().play_preflight(&instance_id))
}

/// parity `play.launch` — trả `{taskId}` (contract sidecar giữ nguyên);
/// orchestration đầy đủ trong `antares_app::launch` (lock → account → resolve
/// loader → **auto-install nếu chưa cài** (07c, parity orchestrator step 3) →
/// java → build lệnh MLL-parity → spawn → task complete). Lỗi validation trả
/// typed §117 NGAY (khác legacy fail âm thầm trong thread).
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchPayload {
    pub task_id: String,
}

#[tauri::command]
pub fn play_launch(
    state: tauri::State<'_, CoreState>,
    instance_id: String,
) -> AntaresResponse<LaunchPayload> {
    let services = state.services();
    AntaresResponse::from_result(
        antares_app::launch::launch(&state.app.tasks, &services, &instance_id)
            .map(|outcome| LaunchPayload {
                task_id: outcome.task_id,
            }),
    )
}

/// Batch 07c — `play.install` (native-only; legacy không có lệnh này, legacy
/// chỉ cài chung trong play.launch): cài/repair instance **không launch**.
/// Contract `{taskId}` như launch: validate sync (instance/lock → typed §117),
/// task `INSTALL` trả về NGAY, cài chạy nền (progress qua task — tải hàng trăm
/// MB không được block invoke). Lỗi trong thread → task failed.
#[tauri::command]
pub fn play_install(
    state: tauri::State<'_, CoreState>,
    instance_id: String,
) -> AntaresResponse<LaunchPayload> {
    AntaresResponse::from_result(
        antares_app::play_install(&state.app.tasks, &state.services(), &instance_id).map(
            |outcome| LaunchPayload {
                task_id: outcome.task_id,
            },
        ),
    )
}
