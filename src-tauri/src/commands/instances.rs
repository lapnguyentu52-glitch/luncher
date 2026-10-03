//! Batch 05 — group instances command surface (parity sidecar `instances.*`).
//!
//! Handler chỉ map request → service → response (§88.2): toàn bộ logic nằm
//! trong `antares_app::InstanceStore`, lỗi typed §117 → envelope §96.

use antares_app::Instance;
use serde::Serialize;

use crate::protocol::response::AntaresResponse;
use crate::state::core_state::CoreState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstancesListPayload {
    pub instances: Vec<Instance>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstancePayload {
    pub instance: Instance,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSelectedPayload {
    pub selected: String,
}

/// parity `instances.list` — scan `<root>/instances/*/instance.json`.
#[tauri::command]
pub fn instances_list(state: tauri::State<'_, CoreState>) -> AntaresResponse<InstancesListPayload> {
    AntaresResponse::from_result(
        state
            .services()
            .instances()
            .list()
            .map(|instances| InstancesListPayload { instances }),
    )
}

/// parity `instances.get` — không có → `MC_VERSION_UNKNOWN` (cùng code sidecar).
#[tauri::command]
pub fn instances_get(
    state: tauri::State<'_, CoreState>,
    instance_id: String,
) -> AntaresResponse<InstancePayload> {
    let result = state
        .services()
        .instances()
        .get(&instance_id)
        .map(|instance| InstancePayload { instance })
        .ok_or_else(|| {
            antares_app::AppError::new(
                antares_app::codes::MC_VERSION_UNKNOWN,
                format!("instance not found: {instance_id}"),
            )
        });
    AntaresResponse::from_result(result)
}

/// parity `instances.create` — name/minecraftVersion bắt buộc (CONFIG_INVALID),
/// tên không sạch → INSTANCE_NAME_INVALID; auto-select do handler sidecar làm
/// sau create (parity) — ghi `selectedInstance` cùng lúc.
#[tauri::command]
pub fn instances_create(
    state: tauri::State<'_, CoreState>,
    name: String,
    minecraft_version: String,
    loader: Option<String>,
    memory_max_mb: Option<u32>,
    memory_min_mb: Option<u32>,
) -> AntaresResponse<InstancePayload> {
    let services = state.services();
    let result = services
        .instances()
        .create(
            &name,
            &minecraft_version,
            loader.as_deref(),
            memory_max_mb,
            memory_min_mb,
        )
        .and_then(|instance| {
            // parity handle_instances_create: tạo mới → auto select.
            services.instances().select(&instance.id)?;
            Ok(InstancePayload { instance })
        });
    AntaresResponse::from_result(result)
}

/// parity `instances.select` — ghi `selectedInstance` vào settings.json
/// (cùng file sidecar ConfigManager đọc).
#[tauri::command]
pub fn instances_select(
    state: tauri::State<'_, CoreState>,
    instance_id: String,
) -> AntaresResponse<InstanceSelectedPayload> {
    AntaresResponse::from_result(
        state
            .services()
            .instances()
            .select(&instance_id)
            .map(|selected| InstanceSelectedPayload { selected }),
    )
}
