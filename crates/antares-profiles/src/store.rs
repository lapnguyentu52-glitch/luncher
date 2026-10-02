//! Phase 4 — ProfileStore CRUD qua antares-storage (§98 + parity `ProfileService`).
//!
//! Parity services/profiles/service.py (CRUD + sanitize; plan/apply/revert cần
//! instances/accounts runtime — nối ở tầng bridge):
//! - state `profiles-state.json` — version 1, atomic write (storage write_json_atomic)
//! - create: id `prof-YYYYmmdd-HHMMSS-<6 hex>`, name validate (safe name, ≤64)
//! - duplicate: "<name> copy", "<name> copy 2"... (tránh trùng)
//! - update: chỉ name/spec, sanitize trước khi ghi
//! - delete: bắt buộc confirm=true (nhất quán InstanceService)
//! - sanitize_spec: chỉ 5 section, game key whitelist + coerce, launch key whitelist
//!   (giá trị falsy bị bỏ), jvm key whitelist (jvmArgs list[str])
//! - validate_spec: trả issues thay vì raise (parity validate_spec)

use antares_storage::StorageHandle;
use serde::{Deserialize, Serialize};

use crate::{coerce, is_game_key};

const STATE_FILE: &str = "profiles-state.json";
const STATE_VERSION: i64 = 1;
const MAX_NAME_LEN: usize = 64;
const LAUNCH_KEYS: &[&str] = &[
    "server",
    "port",
    "quickPlaySingleplayer",
    "quickPlayMultiplayer",
    "quickPlayRealms",
    "customResolution",
    "resolutionWidth",
    "resolutionHeight",
];

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileStoreError {
    #[error("invalid profile name: {0}")]
    InvalidName(String),
    #[error("profile not found: {0}")]
    NotFound(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("delete requires explicit confirmation")]
    DeleteUnconfirmed,
    #[error("storage: {0}")]
    Storage(String),
}

impl ProfileStoreError {
    /// Code taxonomy §117 — parity codes.PROFILE_NOT_FOUND / VALIDATION_FAILED.
    pub fn code(&self) -> &'static str {
        match self {
            ProfileStoreError::NotFound(_) => "PROFILE_NOT_FOUND",
            ProfileStoreError::InvalidName(_)
            | ProfileStoreError::Validation(_)
            | ProfileStoreError::DeleteUnconfirmed => "VALIDATION_FAILED",
            ProfileStoreError::Storage(_) => "APP_INTERNAL",
        }
    }
}

/// Parity `is_safe_name` (core/utils/paths.py): không rỗng, không ./.., không kết
/// thúc bằng '.', không chứa \ / : * ? " < > |
pub fn is_safe_name(name: &str) -> bool {
    if name.is_empty() || name == "." || name == ".." || name.ends_with('.') {
        return false;
    }
    !name
        .chars()
        .any(|c| matches!(c, '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
}

fn validate_profile_name(name: &str) -> Result<(), ProfileStoreError> {
    // Parity Python len() — đếm ký tự, không byte (tên có dấu vẫn ≤64 ký tự).
    if !is_safe_name(name) || name.chars().count() > MAX_NAME_LEN {
        return Err(ProfileStoreError::InvalidName(name.to_string()));
    }
    Ok(())
}

/// Spec một profile — 5 section §40.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub account: Option<SectionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<SectionId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jvm: Option<JvmSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionId {
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JvmSpec {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jvm_preset: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub jvm_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileRecord {
    pub id: String,
    pub name: String,
    pub created_at: f64,
    pub updated_at: f64,
    pub spec: ProfileSpec,
}

/// State file JSON: {version, profiles, applied, lastRaw, launch} — parity sidecar
/// (tên field JSON khớp legacy để bridge đọc được state ghi từ 2 phía).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProfileState {
    pub version: i64,
    #[serde(default)]
    pub profiles: Vec<ProfileRecord>,
    #[serde(default)]
    pub applied: std::collections::BTreeMap<String, f64>,
    #[serde(default, rename = "lastRaw")]
    pub last_raw: Option<String>,
    #[serde(default)]
    pub launch: Option<serde_json::Value>,
}

/// CRUD store — 1 file JSON qua StorageHandle (atomic write mỗi lần ghi).
pub struct ProfileStore {
    handle: StorageHandle,
    now: fn() -> f64,
    id_suffix: fn() -> String,
}

impl ProfileStore {
    pub fn new(handle: StorageHandle) -> Self {
        Self {
            handle,
            now: || {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0)
            },
            id_suffix: || {
                // 6 hex chars — đủ phân biệt 2 profile cùng mili-giây (parity uuid4[:6]).
                let nanos = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.subsec_nanos())
                    .unwrap_or(0);
                format!("{nanos:06x}")
            },
        }
    }

    /// Inject clock/id cho test deterministic.
    pub fn with_clock(
        handle: StorageHandle,
        now: fn() -> f64,
        id_suffix: fn() -> String,
    ) -> Self {
        Self {
            handle,
            now,
            id_suffix,
        }
    }

    fn load_state(&self) -> Result<ProfileState, ProfileStoreError> {
        match self.handle.read_json::<ProfileState>(STATE_FILE) {
            Ok(state) if state.version == STATE_VERSION => Ok(state),
            Ok(_) => Ok(Self::empty_state()), // version lệch → reset (parity)
            Err(_) => Ok(Self::empty_state()), // chưa có file → state trống
        }
    }

    fn empty_state() -> ProfileState {
        ProfileState {
            version: STATE_VERSION,
            ..Default::default()
        }
    }

    fn save_state(&self, mut state: ProfileState) -> Result<(), ProfileStoreError> {
        state.version = STATE_VERSION;
        self.handle
            .write_json_atomic(STATE_FILE, &state)
            .map_err(|err| ProfileStoreError::Storage(err.to_string()))
    }

    /// Danh sách profile (không kèm raw — state không chứa raw, chỉ spec).
    pub fn list(&self) -> Result<Vec<ProfileRecord>, ProfileStoreError> {
        Ok(self.load_state()?.profiles)
    }

    pub fn get(&self, profile_id: &str) -> Result<Option<ProfileRecord>, ProfileStoreError> {
        Ok(self
            .load_state()?
            .profiles
            .into_iter()
            .find(|p| p.id == profile_id))
    }

    fn require(&self, profile_id: &str) -> Result<ProfileRecord, ProfileStoreError> {
        self.get(profile_id)?
            .ok_or_else(|| ProfileStoreError::NotFound(profile_id.to_string()))
    }

    pub fn create(&self, name: &str, spec: ProfileSpec) -> Result<ProfileRecord, ProfileStoreError> {
        let name = name.trim();
        validate_profile_name(name)?;
        let spec = sanitize_spec(&spec)?;
        let now = (self.now)();
        let profile = ProfileRecord {
            id: format!("prof-{}-{}", timestamp_id(now), (self.id_suffix)()),
            name: name.to_string(),
            created_at: now,
            updated_at: now,
            spec,
        };
        let mut state = self.load_state()?;
        state.profiles.push(profile.clone());
        self.save_state(state)?;
        Ok(profile)
    }

    pub fn duplicate(&self, profile_id: &str) -> Result<ProfileRecord, ProfileStoreError> {
        let src = self.require(profile_id)?;
        let existing: std::collections::HashSet<String> =
            self.list()?.into_iter().map(|p| p.name).collect();
        let mut name = format!("{} copy", src.name);
        let mut n = 2;
        while existing.contains(&name) {
            name = format!("{} copy {n}", src.name);
            n += 1;
        }
        self.create(&name, src.spec)
    }

    /// Update chỉ name/spec (parity update) — không đụng runtime state.
    pub fn update(
        &self,
        profile_id: &str,
        patch: ProfilePatch,
    ) -> Result<ProfileRecord, ProfileStoreError> {
        let mut state = self.load_state()?;
        let target = state
            .profiles
            .iter_mut()
            .find(|p| p.id == profile_id)
            .ok_or_else(|| ProfileStoreError::NotFound(profile_id.to_string()))?;
        // Parity legacy: patch rỗng (không name/spec) → chỉ bump updatedAt.
        if let Some(name) = patch.name {
            let name = name.trim().to_string();
            validate_profile_name(&name)?;
            target.name = name;
        }
        if let Some(spec) = patch.spec {
            target.spec = sanitize_spec(&spec)?;
        }
        target.updated_at = (self.now)();
        let updated = target.clone();
        self.save_state(state)?;
        Ok(updated)
    }

    /// Delete — bắt buộc confirm=true; trả false nếu id không tồn tại (parity).
    pub fn delete(&self, profile_id: &str, confirm: bool) -> Result<bool, ProfileStoreError> {
        if !confirm {
            return Err(ProfileStoreError::DeleteUnconfirmed);
        }
        let mut state = self.load_state()?;
        let before = state.profiles.len();
        state.profiles.retain(|p| p.id != profile_id);
        if state.profiles.len() == before {
            return Ok(false);
        }
        state.applied.remove(profile_id);
        self.save_state(state)?;
        Ok(true)
    }

    /// Ghi nhận lần apply gần nhất (dùng khi tầng bridge nối apply/revert).
    pub fn mark_applied(
        &self,
        profile_id: &str,
        last_raw: Option<String>,
    ) -> Result<f64, ProfileStoreError> {
        self.require(profile_id)?;
        let mut state = self.load_state()?;
        let now = (self.now)();
        state.applied.clear(); // parity: applied = {profile_id: time} (chỉ 1 entry)
        state.applied.insert(profile_id.to_string(), now);
        if last_raw.is_some() {
            state.last_raw = last_raw;
        }
        self.save_state(state)?;
        Ok(now)
    }

    /// Revert: xoá lastRaw/applied/launch (caller ghi file options trước).
    pub fn clear_revert(&self) -> Result<(), ProfileStoreError> {
        let mut state = self.load_state()?;
        state.last_raw = None;
        state.applied.clear();
        state.launch = None;
        self.save_state(state)
    }

    pub fn launch_hint(&self) -> Result<Option<serde_json::Value>, ProfileStoreError> {
        Ok(self.load_state()?.launch)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ProfilePatch {
    pub name: Option<String>,
    pub spec: Option<ProfileSpec>,
}

/// `prof-YYYYmmdd-HHMMSS` (giờ địa phương không quan trọng — dùng UTC cho deterministic).
fn timestamp_id(epoch_secs: f64) -> String {
    let secs = epoch_secs.floor() as i64;
    let days = secs.div_euclid(86_400);
    let tod = secs.rem_euclid(86_400);
    let (h, m, s) = (tod / 3600, (tod % 3600) / 60, tod % 60);
    // civil_from_days (Howard Hinnant algorithm) — UTC date từ days since epoch.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mth = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mth <= 2 { y + 1 } else { y };
    format!("{y:04}{mth:02}{d:02}-{h:02}{m:02}{s:02}")
}

/// Parity `_sanitize_spec`: chỉ 5 section; account/instance phải {id: str};
/// jvm whitelist + jvmArgs list[str]; game whitelist + coerce; launch whitelist,
/// giá trị falsy bị bỏ. Section lạ → lỗi (parity raise).
pub fn sanitize_spec(spec: &ProfileSpec) -> Result<ProfileSpec, ProfileStoreError> {
    let mut out = ProfileSpec::default();
    // Parity legacy: account/instance chỉ yêu cầu {id: str} (struct đã ép kiểu —
    // chuỗi rỗng vẫn hợp lệ như isinstance(str) của Python).
    if let Some(account) = &spec.account {
        out.account = Some(SectionId {
            id: account.id.clone(),
        });
    }
    if let Some(instance) = &spec.instance {
        out.instance = Some(SectionId {
            id: instance.id.clone(),
        });
    }
    if let Some(jvm) = &spec.jvm {
        out.jvm = Some(JvmSpec {
            memory: jvm.memory,
            jvm_preset: jvm.jvm_preset.clone(),
            jvm_args: jvm.jvm_args.clone(),
        });
    }
    if let Some(game) = &spec.game {
        let mut sanitized = serde_json::Map::new();
        for (key, value) in game {
            if !is_game_key(key) {
                return Err(ProfileStoreError::Validation(format!(
                    "game: unknown key {key}"
                )));
            }
            sanitized.insert(key.clone(), serde_json::Value::String(coerce(key, value)));
        }
        out.game = Some(sanitized);
    }
    if let Some(launch) = &spec.launch {
        let mut sanitized = serde_json::Map::new();
        for (key, value) in launch {
            if !LAUNCH_KEYS.contains(&key.as_str()) {
                return Err(ProfileStoreError::Validation(format!(
                    "launch: unknown key {key}"
                )));
            }
            if is_truthy(value) {
                sanitized.insert(key.clone(), value.clone());
            }
        }
        out.launch = Some(sanitized);
    }
    Ok(out)
}

pub fn is_truthy(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Null => false,
        serde_json::Value::Bool(b) => *b,
        serde_json::Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        serde_json::Value::String(s) => !s.is_empty(),
        serde_json::Value::Array(a) => !a.is_empty(),
        serde_json::Value::Object(o) => !o.is_empty(),
    }
}

/// Parity `validate_spec` — trả danh sách issues thay vì raise (cho UI preview).
pub fn validate_spec(spec: &ProfileSpec) -> Vec<String> {
    let mut issues = Vec::new();
    if let Some(launch) = &spec.launch {
        for key in launch.keys() {
            if !LAUNCH_KEYS.contains(&key.as_str()) {
                issues.push(format!("launch: unknown key {key}"));
            }
        }
    }
    if let Some(game) = &spec.game {
        for key in game.keys() {
            if !is_game_key(key) {
                issues.push(format!("game: unknown key {key}"));
            }
        }
    }
    if let Some(jvm) = &spec.jvm {
        // struct JvmSpec tự giới hạn key — giữ check shape cho jvmArgs
        // (serde deserialize đã ép list[str]).
        let _ = jvm;
    }
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use antares_storage::{ScopedRoot, StorageService};

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "antares-profile-store-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn handle(tag: &str) -> StorageHandle {
        let root = temp_root(tag);
        StorageService::new(root).scoped(ScopedRoot::AppData)
    }

    fn store(tag: &str) -> ProfileStore {
        ProfileStore::new(handle(tag))
    }

    fn fixed_clock() -> (fn() -> f64, fn() -> String) {
        (|| 1_700_000_000.0, || "abc123".to_string())
    }

    #[test]
    fn is_safe_name_parity() {
        assert!(is_safe_name("My Profile"));
        assert!(!is_safe_name(""));
        assert!(!is_safe_name("."));
        assert!(!is_safe_name(".."));
        assert!(!is_safe_name("trailing."));
        assert!(!is_safe_name("a/b"));
        assert!(!is_safe_name("a\\b"));
        assert!(!is_safe_name("a:b"));
        assert!(!is_safe_name("bad*name"));
    }

    #[test]
    fn create_get_list_roundtrip() {
        let store = store("crud");
        let spec = ProfileSpec {
            game: Some(
                [("gamma".to_string(), serde_json::json!(0.5))]
                    .into_iter()
                    .collect(),
            ),
            ..Default::default()
        };
        let created = store.create("PvP Setup", spec.clone()).unwrap();
        assert!(created.id.starts_with("prof-"));
        assert_eq!(created.name, "PvP Setup");
        // game coerce khi lưu
        assert_eq!(
            created.spec.game.as_ref().unwrap()["gamma"],
            serde_json::json!("0.5")
        );

        let fetched = store.get(&created.id).unwrap().unwrap();
        assert_eq!(fetched, created);
        assert_eq!(store.list().unwrap().len(), 1);

        // name sai → VALIDATION_FAILED
        assert_eq!(store.create("", spec.clone()).unwrap_err().code(), "VALIDATION_FAILED");
        assert_eq!(store.create("bad/name", spec).unwrap_err().code(), "VALIDATION_FAILED");
    }

    #[test]
    fn duplicate_names_uniquified() {
        let store = store("dup");
        let a = store.create("Base", ProfileSpec::default()).unwrap();
        let b = store.duplicate(&a.id).unwrap();
        assert_eq!(b.name, "Base copy");
        let c = store.duplicate(&a.id).unwrap();
        assert_eq!(c.name, "Base copy 2");
        // không tồn tại
        assert_eq!(
            store.duplicate("prof-none").unwrap_err().code(),
            "PROFILE_NOT_FOUND"
        );
    }

    #[test]
    fn update_patch_name_and_spec() {
        let store = store("update");
        let created = store
            .create(
                "Old",
                ProfileSpec {
                    jvm: Some(JvmSpec {
                        memory: Some(4096),
                        jvm_preset: None,
                        jvm_args: vec!["-XX:+UseG1GC".into()],
                    }),
                    ..Default::default()
                },
            )
            .unwrap();

        let updated = store
            .update(
                &created.id,
                ProfilePatch {
                    name: Some("New".into()),
                    spec: None,
                },
            )
            .unwrap();
        assert_eq!(updated.name, "New");
        assert_eq!(updated.spec.jvm.as_ref().unwrap().memory, Some(4096));

        // spec patch — game key lạ bị chặn
        let bad = store
            .update(
                &created.id,
                ProfilePatch {
                    name: None,
                    spec: Some(ProfileSpec {
                        game: Some(
                            [("hacky".to_string(), serde_json::json!(1))]
                                .into_iter()
                                .collect(),
                        ),
                        ..Default::default()
                    }),
                },
            )
            .unwrap_err();
        assert_eq!(bad.code(), "VALIDATION_FAILED");
    }

    #[test]
    fn delete_requires_confirm() {
        let store = store("delete");
        let created = store.create("Temp", ProfileSpec::default()).unwrap();
        assert_eq!(
            store.delete(&created.id, false).unwrap_err().code(),
            "VALIDATION_FAILED"
        );
        assert!(store.delete(&created.id, true).unwrap());
        assert!(store.get(&created.id).unwrap().is_none());
        // delete lần 2 → false (parity)
        assert!(!store.delete(&created.id, true).unwrap());
    }

    #[test]
    fn mark_applied_and_clear_revert() {
        let (now, suffix) = fixed_clock();
        let root = temp_root("applied");
        let service = StorageService::new(root);
        let store = ProfileStore::with_clock(service.scoped(ScopedRoot::AppData), now, suffix);
        let created = store.create("P", ProfileSpec::default()).unwrap();

        let applied_at = store.mark_applied(&created.id, Some("gamma:1.0\n".into())).unwrap();
        assert_eq!(applied_at, 1_700_000_000.0);
        // launch_hint chưa set → None
        assert!(store.launch_hint().unwrap().is_none());

        store.clear_revert().unwrap();
        let state = store.list().unwrap();
        assert_eq!(state.len(), 1); // profile vẫn còn, chỉ applied clear

        // id format deterministic với clock inject
        assert_eq!(created.id, "prof-20231114-221320-abc123");
    }

    #[test]
    fn sanitize_spec_launch_drops_falsy_and_rejects_unknown() {
        // launch giá trị falsy bị bỏ (parity {k: v for ... if v})
        let spec = ProfileSpec {
            launch: Some(
                [
                    ("server".to_string(), serde_json::json!("mc.example.com")),
                    ("port".to_string(), serde_json::json!(0)),
                ]
                .into_iter()
                .collect(),
            ),
            ..Default::default()
        };
        let sanitized = sanitize_spec(&spec).unwrap();
        let launch = sanitized.launch.unwrap();
        assert!(launch.contains_key("server"));
        assert!(!launch.contains_key("port"));

        // section lạ — struct không có field lạ (serde deny UnknownFields? không) —
        // nhưng game/launch key lạ phải bị chặn.
        let bad = ProfileSpec {
            launch: Some([("weirdKey".to_string(), serde_json::json!(1))].into_iter().collect()),
            ..Default::default()
        };
        assert_eq!(sanitize_spec(&bad).unwrap_err().code(), "VALIDATION_FAILED");
    }

    #[test]
    fn validate_spec_reports_issues_without_raising() {
        let spec = ProfileSpec {
            game: Some([("nope".to_string(), serde_json::json!(1))].into_iter().collect()),
            launch: Some([("bad".to_string(), serde_json::json!(1))].into_iter().collect()),
            ..Default::default()
        };
        let issues = validate_spec(&spec);
        assert_eq!(issues, vec!["launch: unknown key bad", "game: unknown key nope"]);
    }

    #[test]
    fn timestamp_id_format() {
        // 2023-11-14 22:13:20 UTC → prof-20231114-221320
        assert_eq!(timestamp_id(1_700_000_000.0), "20231114-221320");
        assert_eq!(timestamp_id(0.0), "19700101-000000");
    }
}
