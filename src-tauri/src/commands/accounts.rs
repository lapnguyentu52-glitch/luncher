//! B15.2 — accounts command surface (parity sidecar `accounts.*`).
//!
//! Handler chỉ map request → service → response (§88.2): toàn bộ logic nằm
//! trong `antares_app::AccountStore`, lỗi typed §117 → envelope §96.

use serde::Serialize;
use serde_json::Value;

use crate::protocol::response::AntaresResponse;
use crate::state::core_state::CoreState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountsListPayload {
    pub accounts: Vec<Value>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSelectedPayload {
    pub selected: String,
}

/// parity `accounts.list` — đọc settings.json["accounts"], strip secret keys
/// (token/_-prefix). Parity: strip key thay vì ẩn account.
#[tauri::command]
pub fn accounts_list(
    state: tauri::State<'_, CoreState>,
) -> AntaresResponse<AccountsListPayload> {
    AntaresResponse::from_result(
        state
            .services()
            .accounts()
            .list()
            .map(|accounts| AccountsListPayload { accounts }),
    )
}

/// parity `accounts.select` — ghi `selectedAccount` vào settings.json;
/// id không tồn tại → AUTH_FAILED "account not found: {id}".
#[tauri::command]
pub fn accounts_select(
    state: tauri::State<'_, CoreState>,
    account_id: String,
) -> AntaresResponse<AccountSelectedPayload> {
    AntaresResponse::from_result(
        state
            .services()
            .accounts()
            .select(&account_id)
            .map(|selected| AccountSelectedPayload { selected }),
    )
}
