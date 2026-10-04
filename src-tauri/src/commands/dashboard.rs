//! B15.2 — dashboard command surface (parity sidecar `dashboard.summary`).
//!
//! Aggregate: đọc instances + selected instance + selected account rồi gộp
//! theo ĐÚNG contract `handle_dashboard_summary` (PARITY evidence #3 — TS
//! `DashboardSummary` giữ nguyên, không sửa test mirror):
//!
//! ```json
//! { "appVersion", "instanceCount", "selectedInstanceId",
//!   "recentInstanceId", "recentInstanceName", "account", "legacyAvailable" }
//! ```
//!
//! Không có service layer riêng — handler tổng hợp trực tiếp từ InstanceStore +
//! AccountStore (đều đã có trong AppServices).

use serde::Serialize;
use serde_json::Value;

use crate::protocol::response::AntaresResponse;
use crate::state::core_state::CoreState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSummary {
    /// parity `SERVICE_VERSION` — sidecar = version package (4.0.0).
    pub app_version: String,
    /// Tổng số instance đang có.
    pub instance_count: usize,
    /// Instance được chọn hiện tại (nếu có).
    pub selected_instance_id: Option<String>,
    /// Instance gần nhất có `lastPlayedAt` (parity `max(played, key=...)`).
    pub recent_instance_id: Option<String>,
    pub recent_instance_name: Option<String>,
    /// Selected account đã strip secret — projection `{id, displayName}`.
    pub account: Option<AccountBrief>,
    /// Sidecar luôn `true` (nó chính là legacy). Native: reflect đúng nghĩa —
    /// false khi `ANTA_RUST_ONLY=1` (legacy đã bị cắt) để UI không chờ bridge.
    pub legacy_available: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountBrief {
    pub id: String,
    pub display_name: String,
}

/// parity `handle_dashboard_summary` — aggregate không có service layer riêng.
#[tauri::command]
pub fn dashboard_summary(state: tauri::State<'_, CoreState>) -> AntaresResponse<DashboardSummary> {
    let services = state.services();

    // instances — lỗi IO thật → typed error §117 (envelope §96).
    let instances = match services.instances().list() {
        Ok(list) => list,
        Err(err) => return AntaresResponse::from_result(Err(err)),
    };
    let instance_count = instances.len();

    // selected instance — đọc từ settings.json["selectedInstance"]
    let selected_instance_id = services.instances().selected_instance();

    // recent = instance có lastPlayedAt lớn nhất (legacy: max(played, ...);
    // không ai chơi → cả 2 field None). Sort ổn định bằng total_cmp (f64).
    let recent = instances
        .iter()
        .filter_map(|inst| inst.last_played_at.map(|ts| (ts, inst)))
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, inst)| (inst.id.clone(), inst.name.clone()));
    let recent_instance_id = recent.as_ref().map(|(id, _)| id.clone());
    let recent_instance_name = recent.map(|(_, name)| name);

    // account — legacy get_current() rồi chỉ giữ {id, displayName}
    // (token đã strip ở AccountStore::list — secret không bao giờ lên UI).
    let account = services
        .accounts()
        .selected_account()
        .and_then(|value| project_account(&value));

    AntaresResponse::ok(DashboardSummary {
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        instance_count,
        selected_instance_id,
        recent_instance_id,
        recent_instance_name,
        account,
        legacy_available: !services.flags().rust_only,
    })
}

/// Projection parity `{"id": account["id"], "displayName": account["displayName"]}`.
/// Thiếu `id` → None (account hỏng không render trên UI); thiếu displayName →
/// fallback id (offline account luôn có 2 field, đây chỉ là defensive).
fn project_account(value: &Value) -> Option<AccountBrief> {
    let id = value.get("id")?.as_str()?.to_string();
    let display_name = value
        .get("displayName")
        .and_then(|v| v.as_str())
        .unwrap_or(&id)
        .to_string();
    Some(AccountBrief {
        id,
        display_name,
    })
}
