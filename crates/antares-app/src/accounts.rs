//! M4 Batch 06 — AccountService parity (`services/accounts/service.py` +
//! sidecar `accounts.list/select` handlers).
//!
//! - Mọi IO qua `StorageHandle` scoped Config (§98) — cùng file
//!   `config/settings.json` mà sidecar `ConfigManager` đọc/ghi (bootstrap
//!   truyền `ANTARES_DATA_DIR` = Rust storage root).
//! - `list` trả account dicts đã strip secret (`token`, key `_`-prefix như
//!   `_refresh_token` — mục 24: token KHÔNG bao giờ lên UI) — parity intent
//!   của `api/bridge::accounts_list` (`{k: v for k,v in a.items() if k != "token"}`).
//!   Sidecar `handle_accounts_list` lọc `[a for a in accounts if "token" not in a]`
//!   (ẩn SẠCH account có token → select được nhưng list không thấy); native
//!   strip key thay vì ẩn account để list/select/dashboard nhất quán.
//! - `select` ghi `selectedAccount` vào settings.json (RMW atomic — cùng
//!   pattern `InstanceStore::select`); không thấy id → `AUTH_FAILED`
//!   "account not found: {id}" (parity sidecar `handle_accounts_select`).

use antares_storage::{StorageError, StorageHandle};
use serde_json::Value;

use crate::error::{codes, AppError, AppResult};

/// Secret key không bao giờ serialize lên UI (mục 24): `token` + mọi key
/// internal đánh dấu `_` (vd `_refresh_token` của Microsoft flow).
fn is_secret_key(key: &str) -> bool {
    key == "token" || key.starts_with('_')
}

/// Store cho group accounts (list/select — parity sidecar `accounts.*`).
pub struct AccountStore {
    /// scope Config → `<root>/config` (settings.json)
    config: StorageHandle,
}

impl AccountStore {
    pub fn new(config: StorageHandle) -> Self {
        Self { config }
    }

    /// parity `AccountService.list()` — đọc `settings.json["accounts"]`,
    /// thiếu key → `[]`. JSON hỏng → warn + `[]` (parity ConfigManager trả
    /// defaults); lỗi IO thật (permission) → typed error §117.
    pub fn list(&self) -> AppResult<Vec<Value>> {
        let settings = self.read_settings()?;
        let Some(accounts) = settings.get("accounts").and_then(|a| a.as_array()) else {
            return Ok(Vec::new());
        };
        // Entry không phải object là dữ liệu hỏng — bỏ qua (sidecar vẫn trả
        // raw nhưng UI không dùng được; defensive thay vì panic).
        Ok(accounts
            .iter()
            .filter_map(|entry| {
                let object = entry.as_object()?;
                let mut cleaned = object.clone();
                cleaned.retain(|key, _| !is_secret_key(key));
                Some(Value::Object(cleaned))
            })
            .collect())
    }

    /// parity sidecar `accounts.select` → `AccountService.select`:
    /// id không có trong `accounts` → `AUTH_FAILED` "account not found: {id}";
    /// có → ghi `selectedAccount` (giữ key khác trong settings.json), trả id.
    /// Id kiểm trên list THẬT (gồm cả account có token — như `AccountService.get`).
    pub fn select(&self, id: &str) -> AppResult<String> {
        let mut settings = self.read_settings()?;
        let exists = settings
            .get("accounts")
            .and_then(|a| a.as_array())
            .is_some_and(|accounts| {
                accounts
                    .iter()
                    .any(|account| account.get("id").and_then(|v| v.as_str()) == Some(id))
            });
        if !exists {
            return Err(AppError::new(
                codes::AUTH_FAILED,
                format!("account not found: {id}"),
            ));
        }
        if !settings.is_object() {
            settings = serde_json::json!({});
        }
        settings["selectedAccount"] = serde_json::json!(id);
        self.config.write_json_atomic("settings.json", &settings)?;
        Ok(id.to_string())
    }

    /// Đọc `selectedAccount` id hiện tại (dashboard parity `config.get`).
    /// Không có → None; lỗi IO → None (không crash dashboard).
    pub fn selected_id(&self) -> Option<String> {
        self.read_settings()
            .ok()
            .and_then(|s| s.get("selectedAccount").and_then(|v| v.as_str()).map(|s| s.to_string()))
    }

    /// Đọc selected account object (stripped) cho dashboard.
    /// None nếu chưa có selectedAccount hoặc không tìm thấy trong list.
    pub fn selected_account(&self) -> Option<Value> {
        let selected_id = self.selected_id()?;
        self.list().ok()?.into_iter().find(|a| {
            a.get("id").and_then(|v| v.as_str()) == Some(&selected_id)
        })
    }

    /// B07b — selected account NGUYÊN vẹn (chưa strip secret) cho
    /// `play_launch`: legacy `_build_options` lấy `token`/`minecraftUuid`
    /// thẳng từ account. Giá trị chỉ nằm trong RAM — không serialize/log
    /// (§26); UI mọi nơi khác vẫn đi `selected_account` (đã strip).
    pub fn selected_account_for_launch(&self) -> Option<Value> {
        let selected_id = self.selected_id()?;
        let settings = self.read_settings().ok()?;
        settings
            .get("accounts")?
            .as_array()?
            .iter()
            .find(|a| a.get("id").and_then(|v| v.as_str()) == Some(&selected_id))
            .cloned()
    }

    /// Read settings.json — NotFound → `{}`, JSON hỏng → warn + `{}` (repair
    /// khi select ghi — parity ConfigManager), lỗi IO khác → typed error.
    fn read_settings(&self) -> AppResult<Value> {
        match self.config.read_json("settings.json") {
            Ok(value) => Ok(value),
            Err(StorageError::Io { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(serde_json::json!({}))
            }
            Err(StorageError::Json { path, .. }) => {
                log::warn!("settings.json unreadable ({path}) — reset defaults");
                Ok(serde_json::json!({}))
            }
            Err(err) => Err(err.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use antares_storage::{ScopedRoot, StorageService};

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!("antares-acc-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        root
    }

    fn store(root: &std::path::Path) -> AccountStore {
        AccountStore::new(StorageService::new(root).scoped(ScopedRoot::Config))
    }

    fn write_settings(root: &std::path::Path, value: &Value) {
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::write(
            root.join("config/settings.json"),
            serde_json::to_vec_pretty(value).unwrap(),
        )
        .unwrap();
    }

    #[test]
    fn list_strips_secret_keys_but_keeps_account() {
        let root = temp_root("list");
        let store = store(&root);
        write_settings(
            &root,
            &serde_json::json!({
                "schemaVersion": 3,
                "accounts": [
                    { "id": "ely-1", "type": "ely", "displayName": "Steve",
                      "minecraftUuid": "uuid-1", "token": "s3cret", "_refresh_token": "rt" },
                    { "id": "off-1", "type": "offline", "displayName": "Alex" }
                ]
            }),
        );

        let accounts = store.list().expect("list");
        assert_eq!(accounts.len(), 2, "account có token vẫn phải thấy (strip key, không ẩn)");
        assert_eq!(accounts[0]["id"], "ely-1");
        assert_eq!(accounts[0]["displayName"], "Steve");
        assert_eq!(accounts[0]["minecraftUuid"], "uuid-1");
        assert!(accounts[0].get("token").is_none(), "token không được lên UI (mục 24)");
        assert!(accounts[0].get("_refresh_token").is_none(), "secret _*-key không được lên UI");
        assert_eq!(accounts[1]["displayName"], "Alex");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn list_missing_or_corrupt_settings_is_empty() {
        // Chưa có settings.json → [] (parity config.get("accounts", [])).
        let root = temp_root("empty");
        let store = store(&root);
        assert!(store.list().expect("list").is_empty());

        // File thiếu key accounts → [].
        write_settings(&root, &serde_json::json!({ "theme": "dark" }));
        assert!(store.list().expect("list2").is_empty());

        // JSON hỏng → warn + [] (không panic).
        std::fs::write(root.join("config/settings.json"), b"broken{").unwrap();
        assert!(store.list().expect("list3").is_empty());

        // Entry không phải object → bỏ qua.
        write_settings(
            &root,
            &serde_json::json!({ "accounts": ["garbage", { "id": "ok", "displayName": "Ok" }] }),
        );
        let accounts = store.list().expect("list4");
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0]["id"], "ok");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn select_writes_selected_account_and_keeps_other_keys() {
        let root = temp_root("select");
        let store = store(&root);
        write_settings(
            &root,
            &serde_json::json!({
                "schemaVersion": 3,
                "theme": "dark",
                "accounts": [
                    { "id": "a1", "displayName": "Steve", "token": "s3cret" },
                    { "id": "a2", "displayName": "Alex" }
                ]
            }),
        );

        // Chọn được account CÓ token (parity AccountService.get dùng raw list).
        assert_eq!(store.select("a1").expect("select"), "a1");
        let raw = std::fs::read_to_string(root.join("config/settings.json")).expect("read");
        let data: Value = serde_json::from_str(&raw).expect("json");
        assert_eq!(data["selectedAccount"], "a1");
        assert_eq!(data["theme"], "dark", "key khác phải giữ nguyên");
        assert_eq!(data["schemaVersion"], 3);
        assert_eq!(data["accounts"][0]["token"], "s3cret", "select không đụng accounts");

        // Id không tồn tại → AUTH_FAILED (parity sidecar message).
        let err = store.select("ghost").expect_err("missing");
        assert_eq!(err.code, codes::AUTH_FAILED);
        assert_eq!(err.message, "account not found: ghost");
        assert!(!err.retryable, "auth fail không tự retry được");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn select_missing_account_is_auth_failed_without_write() {
        let root = temp_root("select-missing");
        let store = store(&root);
        // settings chưa có accounts → mọi id đều AUTH_FAILED, không tạo file.
        let err = store.select("a1").expect_err("no accounts");
        assert_eq!(err.code, codes::AUTH_FAILED);
        assert!(!root.join("config/settings.json").exists());

        // JSON hỏng → như defaults → AUTH_FAILED (parity ConfigManager reset).
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::write(root.join("config/settings.json"), b"broken{").unwrap();
        let err = store.select("").expect_err("empty id");
        assert_eq!(err.code, codes::AUTH_FAILED);
        assert_eq!(err.message, "account not found: ");

        let _ = std::fs::remove_dir_all(&root);
    }
}
