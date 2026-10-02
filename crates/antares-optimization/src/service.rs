//! GameOptimizationService — parity 1:1 `services/optimization/service.py`
//! (mục 3, 70). Flow mục 3.1: scan → preview diff → snapshot → apply → rollback.
//!
//! - `plan()` không ghi gì; `apply()` luôn snapshot `opt-<ts>.json` TRƯỚC khi ghi
//!   (mục 70 rollback 1 click); `rollback()` restore instance.json fields + raw
//!   options.txt nguyên vẹn
//! - Instance-local §74: chỉ đụng instance.json + game/options.txt
//! - `instance.json` đọc/ghi qua callback inject (crate không biết instance store)

use std::path::PathBuf;

use serde::Serialize;

use crate::profiles::{get_profile, memory_for_profile, profile_catalog, McValue};
use crate::{recommend, OptimizationError, Recommendation};

/// Key Minecraft hiển thị diff theo thứ tự cố định (preview ổn định — mục 10.5).
pub const OPTIONS_KEYS: &[&str] = &[
    "renderDistance",
    "simulationDistance",
    "particles",
    "clouds",
    "entityShadows",
    "mipmapLevels",
    "vsync",
    "maxFps",
    "biomeBlendRadius",
    "entityDistanceScaling",
    "guiScale",
    "fullscreen",
];

/// State file của optimization theo instance — parity `optimization.json`.
pub const SERVICE_STATE_FILE: &str = "optimization.json";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JvmChange {
    pub field: String,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionChange {
    pub field: String,
    pub before: Option<String>,
    pub after: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanOutput {
    pub instance_id: String,
    pub profile_id: String,
    pub jvm: Vec<JvmChange>,
    pub minecraft: Vec<OptionChange>,
    pub has_changes: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanOutput {
    pub hardware: crate::Hardware,
    pub memory: Option<crate::MemoryRecommendation>,
    pub profile: &'static str,
    pub warnings: Vec<&'static str>,
    pub bottlenecks: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_profile: Option<String>,
    pub profiles: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    pub file: String,
    pub applied_at: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyOutput {
    pub snapshot: Snapshot,
    pub plan: PlanOutput,
    pub profile: String,
}

/// Instance fields dùng cho snapshot + diff (parity 3 key).
#[derive(Debug, Clone, Default)]
pub struct InstanceSnapshot {
    pub memory: Option<serde_json::Value>,
    pub jvm_preset: Option<serde_json::Value>,
    pub jvm_args: Option<serde_json::Value>,
}

/// Callbacks vào runtime còn lại (instances store) — crate thuần không giữ store.
pub trait InstanceStore {
    /// `instances.get(id)` — None khi không tồn tại.
    fn get(&self, instance_id: &str) -> Option<serde_json::Value>;
    /// `instances.update(id, patch)`.
    fn update(&self, instance_id: &str, patch: &serde_json::Value);
}

/// GameOptimizationService — paths + instance store + clock inject.
pub struct Service<S: InstanceStore> {
    pub(crate) instances_root: PathBuf,
    pub(crate) store: S,
    pub(crate) now: fn() -> f64,
    /// Hardware đo sẵn (parity advisor.recommend dùng psutil) — inject.
    pub(crate) hardware: crate::Hardware,
}

impl<S: InstanceStore> Service<S> {
    pub fn new(instances_root: impl Into<PathBuf>, store: S, hardware: crate::Hardware) -> Self {
        Self {
            instances_root: instances_root.into(),
            store,
            now: || {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0)
            },
            hardware,
        }
    }

    pub fn with_clock(mut self, now: fn() -> f64) -> Self {
        self.now = now;
        self
    }

    // ------------------------------------------------------------------
    // Scan + recommendation (mục 3.1)
    // ------------------------------------------------------------------

    pub fn scan(
        &self,
        instance_id: Option<&str>,
    ) -> Result<ScanOutput, OptimizationError> {
        let rec: Recommendation = recommend(self.hardware.clone());
        let mut instance = None;
        let mut current_profile = None;
        if let Some(iid) = instance_id {
            let inst = self.require(iid)?;
            current_profile = self.load_state(iid).get("profile").and_then(|v| v.as_str()).map(str::to_string);
            instance = Some(serde_json::json!({
                "id": inst.get("id").cloned().unwrap_or(serde_json::json!(iid)),
                "name": inst.get("name").cloned().unwrap_or(serde_json::Value::Null),
                "memory": inst.get("memory").cloned().unwrap_or(serde_json::Value::Null),
                "jvmPreset": inst.get("jvmPreset").cloned().unwrap_or(serde_json::Value::Null),
            }));
        }
        Ok(ScanOutput {
            hardware: rec.hardware,
            memory: rec.memory,
            profile: rec.profile,
            warnings: rec.warnings,
            bottlenecks: rec.bottlenecks,
            instance,
            current_profile,
            profiles: profile_catalog(),
        })
    }

    // ------------------------------------------------------------------
    // Plan (preview diff — không ghi gì, mục 3.1)
    // ------------------------------------------------------------------

    pub fn plan(&self, instance_id: &str, profile_id: &str) -> Result<PlanOutput, OptimizationError> {
        let inst = self.require(instance_id)?;
        let prof = get_profile(profile_id)
            .ok_or_else(|| OptimizationError::UnknownProfile(profile_id.to_string()))?;

        let recommended = recommend_memory_of(&self.hardware);
        let memory = memory_for_profile(profile_id, recommended.as_ref());
        let jvm_changes = diff_jvm(&inst, memory.as_ref(), &prof.jvm);
        let current = self.read_options(instance_id);
        let mc_changes = diff_options(&current, &prof.minecraft);

        let has_changes = !jvm_changes.is_empty() || !mc_changes.is_empty();
        Ok(PlanOutput {
            instance_id: instance_id.to_string(),
            profile_id: profile_id.to_string(),
            jvm: jvm_changes,
            minecraft: mc_changes,
            has_changes,
        })
    }

    // ------------------------------------------------------------------
    // Apply (snapshot → ghi) + rollback (mục 70)
    // ------------------------------------------------------------------

    pub fn apply(&self, instance_id: &str, profile_id: &str) -> Result<ApplyOutput, OptimizationError> {
        let plan = self.plan(instance_id, profile_id)?; // validate trước, không ghi
        let snapshot = self.snapshot(instance_id)?;

        // JVM patch từ plan
        let mut patch = serde_json::Map::new();
        for ch in &plan.jvm {
            patch.insert(ch.field.clone(), ch.after.clone());
        }
        if !patch.is_empty() {
            self.store
                .update(instance_id, &serde_json::Value::Object(patch));
        }

        // options.txt: merge các change vào raw hiện tại rồi ghi merged.
        // after đã là chuỗi render (to_option_str) — bọc McValue::Owned để ghi nguyên.
        let mut merged: std::collections::BTreeMap<String, McValue> = self
            .read_options_map(instance_id)
            .into_iter()
            .map(|(k, v)| (k, McValue::Owned(v)))
            .collect();
        for ch in &plan.minecraft {
            merged.insert(ch.field.clone(), McValue::Owned(ch.after.clone()));
        }
        self.write_options(instance_id, &merged)?;

        self.save_state(
            instance_id,
            &serde_json::json!({
                "profile": profile_id,
                "appliedAt": (self.now)(),
                "snapshotFile": snapshot.file,
            }),
        )?;
        Ok(ApplyOutput {
            snapshot,
            plan,
            profile: profile_id.to_string(),
        })
    }

    pub fn rollback(&self, instance_id: &str) -> Result<serde_json::Value, OptimizationError> {
        let state = self.load_state(instance_id);
        let Some(snap_file) = state.get("snapshotFile").and_then(|v| v.as_str()) else {
            return Err(OptimizationError::NoSnapshot);
        };
        let path = self.snapshots_dir(instance_id).join(snap_file);
        if !path.is_file() {
            return Err(OptimizationError::SnapshotMissing(path.display().to_string()));
        }
        let snap: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).map_err(|err| OptimizationError::Io(err.to_string()))?)
                .map_err(|err| OptimizationError::Io(err.to_string()))?;

        // Restore instance.json fields đã backup
        if let Some(instance_patch) = snap.get("instance") {
            if instance_patch.is_object() {
                self.store.update(instance_id, instance_patch);
            }
        }
        // Restore options.txt nguyên vẹn
        if let Some(raw) = snap.get("optionsRaw").and_then(|v| v.as_str()) {
            let options_path = self.options_path(instance_id);
            if let Some(parent) = options_path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|err| OptimizationError::Io(err.to_string()))?;
            }
            std::fs::write(&options_path, raw).map_err(|err| OptimizationError::Io(err.to_string()))?;
        }

        self.save_state(
            instance_id,
            &serde_json::json!({"profile": serde_json::Value::Null, "rolledBackAt": (self.now)()}),
        )?;
        Ok(serde_json::json!({"restored": true}))
    }

    pub fn snapshot_info(&self, instance_id: &str) -> Result<Option<serde_json::Value>, OptimizationError> {
        let state = self.load_state(instance_id);
        let Some(snap_file) = state.get("snapshotFile").and_then(|v| v.as_str()) else {
            return Ok(None);
        };
        let path = self.snapshots_dir(instance_id).join(snap_file);
        if !path.is_file() {
            return Ok(None);
        }
        let snap: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).map_err(|err| OptimizationError::Io(err.to_string()))?)
                .map_err(|err| OptimizationError::Io(err.to_string()))?;
        let instance_fields: Vec<String> = snap
            .get("instance")
            .and_then(|v| v.as_object())
            .map(|o| {
                let mut keys: Vec<String> = o.keys().cloned().collect();
                keys.sort();
                keys
            })
            .unwrap_or_default();
        Ok(Some(serde_json::json!({
            "file": snap_file,
            "appliedAt": snap.get("appliedAt").cloned().unwrap_or(serde_json::Value::Null),
            "profile": state.get("profile").cloned().unwrap_or(serde_json::Value::Null),
            "instanceFields": instance_fields,
        })))
    }

    // ------------------------------------------------------------------
    // Helpers — instance / options.txt / snapshot / state
    // ------------------------------------------------------------------

    fn require(&self, instance_id: &str) -> Result<serde_json::Value, OptimizationError> {
        self.store
            .get(instance_id)
            .ok_or_else(|| OptimizationError::InstanceNotFound(instance_id.to_string()))
    }

    pub(crate) fn options_path(&self, instance_id: &str) -> PathBuf {
        self.instances_root
            .join(instance_id)
            .join("game")
            .join("options.txt")
    }

    pub(crate) fn snapshots_dir(&self, instance_id: &str) -> PathBuf {
        let d = self
            .instances_root
            .join(instance_id)
            .join("optimization-snapshots");
        let _ = std::fs::create_dir_all(&d);
        d
    }

    /// Parse options.txt key:value (giữ value raw string — chưa set key = absent).
    pub(crate) fn read_options(&self, instance_id: &str) -> std::collections::BTreeMap<String, String> {
        self.read_options_map(instance_id)
    }

    pub(crate) fn read_options_map(
        &self,
        instance_id: &str,
    ) -> std::collections::BTreeMap<String, String> {
        let mut out = std::collections::BTreeMap::new();
        let path = self.options_path(instance_id);
        if path.is_file() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                for line in text.lines() {
                    if let Some((k, v)) = line.split_once(':') {
                        if !line.starts_with('#') {
                            out.insert(k.trim().to_string(), v.trim().to_string());
                        }
                    }
                }
            }
        }
        out
    }

    /// Ghi merged options: giữ nguyên dòng cũ, cập nhật/append key patch, dedupe
    /// (dòng đầu thắng — mục 10.5). Parity `_write_options` — chỉ key trong
    /// OPTIONS_KEYS được ghi.
    pub(crate) fn write_options(
        &self,
        instance_id: &str,
        values: &std::collections::BTreeMap<String, McValue>,
    ) -> Result<(), OptimizationError> {
        let path = self.options_path(instance_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| OptimizationError::Io(err.to_string()))?;
        }
        let patch: std::collections::BTreeMap<&str, String> = values
            .iter()
            .filter(|(k, _)| OPTIONS_KEYS.contains(&k.as_str()))
            .map(|(k, v)| (k.as_str(), v.to_option_str()))
            .collect();

        let lines: Vec<String> = if path.is_file() {
            std::fs::read_to_string(&path)
                .map(|t| t.lines().map(str::to_string).collect())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let mut seen = std::collections::BTreeSet::new();
        let mut out: Vec<String> = Vec::new();
        for line in &lines {
            let key = line.split(':').next().unwrap_or("").trim().to_string();
            if let Some(val) = patch.get(key.as_str()) {
                if seen.contains(&key) {
                    continue; // duplicate — chỉ giữ 1 (mục 10.5)
                }
                out.push(format!("{key}:{val}"));
                seen.insert(key);
            } else {
                out.push(line.clone());
            }
        }
        for (key, val) in &patch {
            if !seen.contains(*key) {
                out.push(format!("{key}:{val}"));
            }
        }
        let mut text = out.join("\n");
        text.push('\n');
        std::fs::write(&path, text).map_err(|err| OptimizationError::Io(err.to_string()))?;
        Ok(())
    }

    /// Snapshot `opt-<epoch>.json` — instance fields + optionsRaw (mục 70).
    pub(crate) fn snapshot(&self, instance_id: &str) -> Result<Snapshot, OptimizationError> {
        let inst = self.require(instance_id)?;
        let raw = if self.options_path(instance_id).is_file() {
            std::fs::read_to_string(self.options_path(instance_id))
                .unwrap_or_default()
        } else {
            String::new()
        };
        let now = (self.now)();
        // Parity service.py:276 — `opt-<ts>.json` (kèm đuôi .json).
        let fname = format!("opt-{}.json", now as i64);
        let payload = serde_json::json!({
            "appliedAt": now,
            "instance": {
                "memory": inst.get("memory").cloned().unwrap_or(serde_json::Value::Null),
                "jvmPreset": inst.get("jvmPreset").cloned().unwrap_or(serde_json::Value::Null),
                "jvmArgs": inst.get("jvmArgs").cloned().unwrap_or(serde_json::Value::Null),
            },
            "optionsRaw": raw,
        });
        let path = self.snapshots_dir(instance_id).join(&fname);
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, payload.to_string())
            .map_err(|err| OptimizationError::Io(err.to_string()))?;
        std::fs::rename(&tmp, &path).map_err(|err| OptimizationError::Io(err.to_string()))?;
        Ok(Snapshot { file: fname, applied_at: now })
    }

    pub(crate) fn state_path(&self, instance_id: &str) -> PathBuf {
        self.instances_root.join(instance_id).join(SERVICE_STATE_FILE)
    }

    pub(crate) fn load_state(&self, instance_id: &str) -> serde_json::Value {
        let path = self.state_path(instance_id);
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or(serde_json::json!({}))
    }

    pub(crate) fn save_state(
        &self,
        instance_id: &str,
        patch: &serde_json::Value,
    ) -> Result<(), OptimizationError> {
        let mut state = self
            .load_state(instance_id)
            .as_object()
            .cloned()
            .unwrap_or_default();
        if let Some(patch_obj) = patch.as_object() {
            for (k, v) in patch_obj {
                state.insert(k.clone(), v.clone());
            }
        }
        let path = self.state_path(instance_id);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|err| OptimizationError::Io(err.to_string()))?;
        }
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, serde_json::Value::Object(state).to_string())
            .map_err(|err| OptimizationError::Io(err.to_string()))?;
        std::fs::rename(&tmp, &path).map_err(|err| OptimizationError::Io(err.to_string()))?;
        Ok(())
    }
}



/// Diff JVM — parity `_diff_jvm`: memory trước, rồi jvmPreset/jvmArgs.
pub fn diff_jvm(
    instance: &serde_json::Value,
    memory: Option<&crate::MemoryRecommendation>,
    jvm_patch: &crate::ProfileJvm,
) -> Vec<JvmChange> {
    let mut changes = Vec::new();
    let current_mem = instance.get("memory").cloned().unwrap_or(serde_json::Value::Null);
    let target_mem: serde_json::Value = match memory {
        Some(m) => serde_json::json!({"minMb": m.min_mb, "maxMb": m.max_mb}),
        None => current_mem.clone(),
    };
    if target_mem != serde_json::Value::Null && current_mem != target_mem {
        changes.push(JvmChange {
            field: "memory".into(),
            before: current_mem,
            after: target_mem,
        });
    }
    // jvmPreset
    let preset = jvm_patch.jvm_preset;
    let current_preset = instance.get("jvmPreset").cloned().unwrap_or(serde_json::Value::Null);
    if current_preset != serde_json::json!(preset) {
        changes.push(JvmChange {
            field: "jvmPreset".into(),
            before: current_preset,
            after: serde_json::json!(preset),
        });
    }
    // jvmArgs — parity: chỉ patch khi khác (list so bằng JSON)
    let current_args = instance.get("jvmArgs").cloned().unwrap_or(serde_json::Value::Null);
    let new_args = serde_json::Value::Array(
        jvm_patch
            .jvm_args
            .iter()
            .map(|a| serde_json::json!(a))
            .collect(),
    );
    if current_args != new_args {
        changes.push(JvmChange {
            field: "jvmArgs".into(),
            before: current_args,
            after: new_args,
        });
    }
    changes
}

/// Diff options — parity `_diff_options`: duyệt OPTIONS_KEYS theo thứ tự, so với
/// value raw parse từ file.
pub fn diff_options(
    current: &std::collections::BTreeMap<String, String>,
    mc_patch: &[(&'static str, McValue)],
) -> Vec<OptionChange> {
    let mut changes = Vec::new();
    for key in OPTIONS_KEYS {
        let Some((_, desired)) = mc_patch.iter().find(|(k, _)| k == key) else {
            continue;
        };
        let before = current.get(*key).cloned();
        let same = before
            .as_ref()
            .map(|raw| desired.equals_str(raw))
            .unwrap_or(false);
        if !same {
            changes.push(OptionChange {
                field: (*key).to_string(),
                before,
                after: desired.to_option_str(),
            });
        }
    }
    changes
}

fn recommend_memory_of(hardware: &crate::Hardware) -> Option<crate::MemoryRecommendation> {
    crate::recommend_memory(hardware.ram_total_mb)
}

/// HARDWARE_DEFAULT — fallback khi caller không đo được (threads=2 như legacy).
pub const HARDWARE_DEFAULT: crate::Hardware = crate::Hardware {
    ram_total_mb: 0,
    cpu_threads: 2,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Hardware;
    use std::cell::RefCell;

    fn temp_root(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("antares-opt-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// In-memory instance store — get/update như instances service thật.
    struct MemStore {
        instances: RefCell<std::collections::BTreeMap<String, serde_json::Value>>,
    }

    impl MemStore {
        fn new(ids: &[&str]) -> Self {
            let map = ids
                .iter()
                .map(|id| {
                    (
                        id.to_string(),
                        serde_json::json!({
                            "id": id,
                            "name": id,
                            "memory": {"minMb": 512, "maxMb": 2048},
                            "jvmPreset": "auto",
                            "jvmArgs": [],
                        }),
                    )
                })
                .collect();
            Self {
                instances: RefCell::new(map),
            }
        }
    }

    impl InstanceStore for MemStore {
        fn get(&self, instance_id: &str) -> Option<serde_json::Value> {
            self.instances.borrow().get(instance_id).cloned()
        }
        fn update(&self, instance_id: &str, patch: &serde_json::Value) {
            let mut map = self.instances.borrow_mut();
            if let Some(inst) = map.get_mut(instance_id) {
                if let (Some(obj), Some(patch_obj)) = (inst.as_object_mut(), patch.as_object()) {
                    for (k, v) in patch_obj {
                        obj.insert(k.clone(), v.clone());
                    }
                }
            }
        }
    }

    const HW: Hardware = Hardware {
        ram_total_mb: 16384,
        cpu_threads: 8,
    };

    fn fixed_clock() -> fn() -> f64 {
        || 1_700_000_000.0
    }

    #[test]
    fn scan_reports_hardware_and_catalog() {
        let root = temp_root("scan");
        let store = MemStore::new(&["i1"]);
        let svc = Service::new(&root, store, HW).with_clock(fixed_clock());
        let out = svc.scan(Some("i1")).unwrap();
        assert_eq!(out.hardware, HW);
        assert_eq!(out.profile, "balanced");
        assert_eq!(out.memory, Some(crate::MemoryRecommendation { min_mb: 2867, max_mb: 5734 }));
        assert_eq!(out.profiles.len(), 6);
        assert_eq!(out.current_profile, None);
        assert_eq!(out.instance.as_ref().unwrap()["id"], "i1");
        // instance không tồn tại → INSTANCE_NOT_FOUND
        assert_eq!(
            svc.scan(Some("nope")).unwrap_err().code(),
            "INSTANCE_NOT_FOUND"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn plan_previews_diff_without_writing() {
        let root = temp_root("plan");
        let store = MemStore::new(&["i1"]);
        let svc = Service::new(&root, store, HW).with_clock(fixed_clock());
        let plan = svc.plan("i1", "low_end").unwrap();
        assert!(plan.has_changes);
        // memory: advisor 2867/5734 → profile low_end ghi đè 512/1536
        let mem = plan.jvm.iter().find(|c| c.field == "memory").unwrap();
        assert_eq!(mem.before, serde_json::json!({"minMb": 512, "maxMb": 2048}));
        assert_eq!(mem.after, serde_json::json!({"minMb": 512, "maxMb": 1536}));
        // minecraft: mọi key của low_end là change mới (file chưa tồn tại → before None)
        assert_eq!(plan.minecraft.len(), 10);
        assert!(plan.minecraft.iter().all(|c| c.before.is_none()));
        // file KHÔNG được ghi khi plan
        assert!(!root.join("i1/game/options.txt").exists());
        // profile lạ → VALIDATION_FAILED
        assert_eq!(
            svc.plan("i1", "nope").unwrap_err().code(),
            "VALIDATION_FAILED"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_snapshots_then_writes_and_rollback_restores() {
        let root = temp_root("apply");
        let store = MemStore::new(&["i1"]);
        let svc = Service::new(&root, store, HW).with_clock(fixed_clock());

        // options.txt có sẵn — phải được backup vào snapshot
        let opt = root.join("i1/game/options.txt");
        std::fs::create_dir_all(opt.parent().unwrap()).unwrap();
        std::fs::write(&opt, "renderDistance:12\ncustomKey:keep\n").unwrap();

        let out = svc.apply("i1", "low_end").unwrap();
        assert_eq!(out.snapshot.file, "opt-1700000000.json");
        assert!(root.join("i1/optimization-snapshots/opt-1700000000.json").is_file());
        // state ghi profile + snapshotFile
        let state = svc.load_state("i1");
        assert_eq!(state["profile"], "low_end");

        // options.txt: giữ dòng lạ, patch key trong whitelist, append key mới
        let text = std::fs::read_to_string(&opt).unwrap();
        assert!(text.contains("customKey:keep"), "dòng lạ giữ nguyên");
        assert!(text.contains("renderDistance:6"));
        assert!(text.ends_with("\n"));

        // instance.json fields đã patch qua store
        let inst = svc.require("i1").unwrap();
        assert_eq!(inst["memory"]["maxMb"], 1536);
        assert_eq!(inst["jvmPreset"], "g1");

        // snapshot_info đọc được
        let info = svc.snapshot_info("i1").unwrap().unwrap();
        assert_eq!(info["file"], "opt-1700000000.json");
        assert_eq!(info["profile"], "low_end");

        // rollback → restore memory + jvmPreset + options raw nguyên vẹn
        let rolled = svc.rollback("i1").unwrap();
        assert_eq!(rolled["restored"], true);
        let text_after = std::fs::read_to_string(&opt).unwrap();
        assert_eq!(text_after, "renderDistance:12\ncustomKey:keep\n", "options restore nguyên vẹn");
        let inst = svc.require("i1").unwrap();
        assert_eq!(inst["memory"]["maxMb"], 2048, "memory restore từ snapshot");
        // state sau rollback: profile null
        let state = svc.load_state("i1");
        assert!(state["profile"].is_null());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn rollback_without_snapshot_is_error() {
        let root = temp_root("noroll");
        let store = MemStore::new(&["i1"]);
        let svc = Service::new(&root, store, HW).with_clock(fixed_clock());
        assert_eq!(svc.rollback("i1").unwrap_err().code(), "VALIDATION_FAILED");
        // snapshot_info không có snapshot → None
        assert!(svc.snapshot_info("i1").unwrap().is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn diff_jvm_memory_none_keeps_current() {
        // memory recommendation None → target = current → không change memory
        let inst = serde_json::json!({"memory": {"minMb": 512, "maxMb": 2048}, "jvmPreset": "auto", "jvmArgs": []});
        let prof_jvm = crate::ProfileJvm {
            memory: None,
            jvm_preset: "auto",
            jvm_args: &[],
        };
        let changes = diff_jvm(&inst, None, &prof_jvm);
        // jvmPreset/jvmArgs khớp → chỉ memory có thể change, nhưng target=current → 0 changes
        assert!(changes.is_empty());
    }
}
