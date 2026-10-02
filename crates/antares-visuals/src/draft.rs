//! Totem 3D draft store — parity `totem_model_get` / `totem_model_save` trong
//! `services/visuals/service.py` (Batch 4, mục 62: draft store backend-backed).
//!
//! - Draft lưu `<data>/totem-3d/draft.json` — spec v3 mục 5.3.
//! - Save: validate chặn ERROR (mục 14), atomic write tmp+rename (mục 62/91).
//! - Get: file thiếu → `{spec: None}`; corrupt → `{spec: None, corrupt: true}`
//!   để UI recovery (không crash).

use std::path::PathBuf;

use serde_json::{json, Value};

use crate::{model3d, VisualError};

pub struct Totem3dDraft {
    dir: PathBuf,
}

impl Totem3dDraft {
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self { dir: data_dir.into() }
    }

    fn draft_path(&self) -> PathBuf {
        self.dir.join("totem-3d").join("draft.json")
    }

    /// Draft voxel model hiện tại hoặc None — parity `totem_model_get()`.
    pub fn get(&self) -> Value {
        let path = self.draft_path();
        if !path.is_file() {
            return json!({"spec": None::<Value>});
        }
        let text = match std::fs::read_to_string(&path) {
            Ok(t) => t,
            Err(_) => {
                return json!({"spec": None::<Value>, "corrupt": true});
            }
        };
        match serde_json::from_str::<Value>(&text) {
            Ok(spec) => json!({
                "spec": spec,
                "findings": model3d::validate(Some(&spec)),
            }),
            Err(_) => {
                // parity logger.warning("Corrupt totem-3d draft — trả None để UI recovery")
                json!({"spec": None::<Value>, "corrupt": true})
            }
        }
    }

    /// Lưu draft — validate chặn ERROR, atomic write — parity `totem_model_save()`.
    pub fn save(&self, spec: &Value) -> Result<Value, VisualError> {
        let findings = model3d::validate(Some(spec));
        if model3d::has_errors(&findings) {
            let first = model3d::errors(&findings)[0].code.clone();
            return Err(VisualError::SpecInvalid(format!("Invalid model: {first}")));
        }
        let path = self.draft_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| VisualError::Io(e.to_string()))?;
        }
        // parity write_json_atomic: indent 2, ensure_ascii=False, tmp ".json.tmp"
        let tmp = path.with_extension("json.tmp");
        let body = serde_json::to_string_pretty(spec).unwrap_or_else(|_| "null".into());
        std::fs::write(&tmp, body).map_err(|e| VisualError::Io(e.to_string()))?;
        std::fs::rename(&tmp, &path).map_err(|e| VisualError::Io(e.to_string()))?;
        Ok(json!({"saved": true, "findings": findings}))
    }
}

/// Validate Finding list từ draft cho caller muốn check riêng (parity shape).
pub fn findings_of(spec: &Value) -> Vec<crate::ModelFinding> {
    model3d::validate(Some(spec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_root(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("antares-draft-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn good_spec() -> Value {
        json!({
            "grid": 16,
            "cubes": [
                {"id": "body", "from": [4, 0, 4], "to": [12, 10, 12],
                 "faces": {"north": {"texture": "#e8b23a"}}},
            ],
        })
    }

    #[test]
    fn get_missing_returns_none() {
        let d = Totem3dDraft::new(temp_root("missing"));
        let out = d.get();
        assert_eq!(out["spec"], Value::Null);
        assert!(out.get("corrupt").is_none());
    }

    #[test]
    fn save_then_get_roundtrip_with_findings() {
        let root = temp_root("roundtrip");
        let d = Totem3dDraft::new(&root);
        let out = d.save(&good_spec()).unwrap();
        assert_eq!(out["saved"], true);
        assert_eq!(out["findings"].as_array().unwrap().len(), 0);

        // file đúng vị trí + tmp đã rename
        let path = root.join("totem-3d").join("draft.json");
        assert!(path.is_file());
        assert!(!root.join("totem-3d").join("draft.json.tmp").exists());
        // indent 2 parity
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("{\n  "));

        let out = d.get();
        assert_eq!(out["spec"]["grid"], 16);
        assert_eq!(out["findings"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn save_invalid_spec_blocked_and_file_untouched() {
        let root = temp_root("invalid");
        let d = Totem3dDraft::new(&root);
        // lần 1 lưu hợp lệ
        d.save(&good_spec()).unwrap();
        let before = std::fs::read_to_string(root.join("totem-3d").join("draft.json")).unwrap();

        // spec lỗi → VALIDATION_FAILED kèm code đầu tiên
        let bad = json!({"grid": 16, "cubes": []});
        let err = d.save(&bad).unwrap_err();
        assert_eq!(err.code(), "VALIDATION_FAILED");
        assert!(err.to_string().contains("Invalid model: MODEL_NO_CUBES"));
        // file cũ nguyên vẹn
        let after = std::fs::read_to_string(root.join("totem-3d").join("draft.json")).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn corrupt_draft_recovers_none() {
        let root = temp_root("corrupt");
        std::fs::create_dir_all(root.join("totem-3d")).unwrap();
        std::fs::write(root.join("totem-3d").join("draft.json"), "{broken").unwrap();
        let d = Totem3dDraft::new(&root);
        let out = d.get();
        assert_eq!(out["spec"], Value::Null);
        assert_eq!(out["corrupt"], true);
        // save tiếp vẫn hoạt động (ghi đè)
        d.save(&good_spec()).unwrap();
        assert_eq!(d.get()["spec"]["grid"], 16);
    }
}
