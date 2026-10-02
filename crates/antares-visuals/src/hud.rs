//! HUD Studio backend — parity 1:1 `services/visuals/hud.py` (mục 11).
//!
//! HUD runtime hiển thị là việc của companion mod (mục 12); launcher quản lý
//! **layout profile**: widget nào, toạ độ, scale, visible. Validate chặt (mục 76):
//! tọa độ trong 0..819, scale 0.5..4, widget id trong danh sách cho phép.
//!
//! Layout lưu trong visual project (mục 34: JSON, không binary).

use serde_json::{json, Value};

/// Widgets hỗ trợ (mục 11).
pub const WIDGETS: [&str; 14] = [
    "fps",
    "cps",
    "coordinates",
    "armor",
    "potions",
    "clock",
    "ping",
    "server",
    "biome",
    "facing",
    "memory",
    "keystrokes",
    "target_info",
    "session_time",
];

/// Widget mặc định bật (gọn — mục 25.4: không crowded).
pub fn default_layout() -> Value {
    json!([
        {"id": "fps", "x": 4, "y": 4, "scale": 1.0, "visible": true},
        {"id": "coordinates", "x": 4, "y": 20, "scale": 1.0, "visible": true},
        {"id": "clock", "x": 4, "y": 36, "scale": 1.0, "visible": false},
    ])
}

/// (lo, hi) — x 0..819, y 0..459, scale 0.5..4.0.
pub const LIMITS_X: (f64, f64) = (0.0, 819.0);
pub const LIMITS_Y: (f64, f64) = (0.0, 459.0);
pub const LIMITS_SCALE: (f64, f64) = (0.5, 4.0);

fn limits(key: &str) -> Option<(f64, f64)> {
    match key {
        "x" => Some(LIMITS_X),
        "y" => Some(LIMITS_Y),
        "scale" => Some(LIMITS_SCALE),
        _ => None,
    }
}

/// parity Python `float(v)`: nhận Number + numeric string + bool
/// (`float(True) == 1.0`); lỗi None/null, object, string phi số.
fn py_float(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

/// Trả danh sách lỗi rỗng nếu hợp lệ — parity `validate()`.
/// Error shape: `{"widget": ..., "error": ...}`.
pub fn validate(layout: Option<&Value>) -> Vec<Value> {
    let mut errors: Vec<Value> = Vec::new();
    let Some(Value::Array(items)) = layout else {
        return vec![json!({"widget": "?", "error": "layout must be a list"})];
    };
    let mut seen: Vec<&str> = Vec::new();
    for (i, w) in items.iter().enumerate() {
        let Some(id) = w.get("id").and_then(|v| v.as_str()) else {
            errors.push(json!({"widget": format!("[{i}]"), "error": "unknown widget id"}));
            continue;
        };
        if !WIDGETS.contains(&id) {
            errors.push(json!({"widget": format!("[{i}]"), "error": "unknown widget id"}));
            continue;
        }
        if seen.contains(&id) {
            errors.push(json!({"widget": id, "error": "duplicate widget"}));
        }
        seen.push(id);
        for key in ["x", "y", "scale"] {
            let (lo, hi) = limits(key).unwrap();
            // parity w.get(key, default): key THIẾU → default → không lỗi; chỉ
            // key CÓ mà float() fail (null, object, string phi số) mới lỗi
            let Some(v) = w.get(key) else { continue };
            let Some(fv) = py_float(v) else {
                errors.push(json!({"widget": id, "error": format!("{key} not a number")}));
                continue;
            };
            if !(lo..=hi).contains(&fv) {
                errors.push(json!({
                    "widget": id,
                    "error": format!("{key}={} out of {}..{}", fmt_num(fv), fmt_num(lo), fmt_num(hi)),
                }));
            }
        }
    }
    errors
}

/// Format số khớp Python str(float): 4.0 → "4.0", 0.5 → "0.5".
fn fmt_num(v: f64) -> String {
    if v == v.trunc() && v.abs() < 1e15 {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

/// Clamp về khoảng hợp lệ — dùng khi persist. Parity `sanitize()`.
pub fn sanitize(layout: Option<&Value>) -> Vec<Value> {
    let Some(Value::Array(items)) = layout else {
        return default_layout().as_array().cloned().unwrap_or_default();
    };
    let mut out: Vec<Value> = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for w in items {
        let Some(id) = w.get("id").and_then(|v| v.as_str()) else {
            continue;
        };
        if !WIDGETS.contains(&id) || seen.contains(&id) {
            continue;
        }
        let mut clean = serde_json::Map::new();
        clean.insert("id".into(), json!(id));
        clean.insert("visible".into(), json!(crate::truthy(w.get("visible"), true)));
        for key in ["x", "y", "scale"] {
            let (lo, hi) = limits(key).unwrap();
            let default = if key == "scale" { 1.0 } else { 0.0 };
            // parity float(w.get(key, default)): key thiếu → default; giá trị
            // bool/numeric-string cũng được float() hoá như Python
            let mut v = w
                .get(key)
                .map(py_float)
                .unwrap_or_else(|| Some(default))
                .unwrap_or(default);
            v = v.clamp(lo, hi);
            clean.insert(
                key.into(),
                if key == "scale" {
                    json!((v * 100.0).round() / 100.0)
                } else {
                    json!(v as i64)
                },
            );
        }
        seen.push(id);
        out.push(Value::Object(clean));
    }
    out
}

/// Sanitize + đảm bảo đủ widget mặc định (widget chưa cấu hình → default).
/// Parity `merge_default()`.
pub fn merge_default(layout: Option<&Value>) -> Vec<Value> {
    let mut clean = sanitize(layout);
    let have: Vec<String> = clean
        .iter()
        .filter_map(|w| w.get("id").and_then(|v| v.as_str()).map(String::from))
        .collect();
    for w in default_layout().as_array().cloned().unwrap_or_default() {
        let id = w.get("id").and_then(|v| v.as_str()).unwrap_or("");
        if !have.iter().any(|h| h == id) {
            clean.push(w);
        }
    }
    clean
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widgets_catalogue_parity() {
        assert_eq!(WIDGETS.len(), 14);
        assert_eq!(WIDGETS[0], "fps");
        assert_eq!(WIDGETS[13], "session_time");
        let dl = default_layout();
        assert_eq!(dl.as_array().unwrap().len(), 3);
        assert_eq!(dl[0]["id"], "fps");
        assert_eq!(dl[2]["visible"], false);
    }

    #[test]
    fn validate_unknown_duplicate_and_bounds() {
        // không phải list
        let errs = validate(Some(&json!({"id": "fps"})));
        assert_eq!(errs[0]["error"], "layout must be a list");

        // unknown + duplicate + out-of-bounds + non-number
        let layout = json!([
            {"id": "fps", "x": 4, "y": 4, "scale": 1.0},
            {"id": "nope", "x": 0, "y": 0, "scale": 1.0},
            {"id": "fps", "x": 0, "y": 0, "scale": 1.0},
            {"id": "clock", "x": 900, "y": 4, "scale": 1.0},
            {"id": "clock", "x": 4, "y": 4, "scale": 9.0},
            {"id": "ping", "x": "four", "y": 4, "scale": 1.0},
        ]);
        let errs = validate(Some(&layout));
        let texts: Vec<String> = errs
            .iter()
            .map(|e| e["error"].as_str().unwrap_or("").to_string())
            .collect();
        assert!(texts.iter().any(|t| t == "unknown widget id"));
        assert!(texts.iter().any(|t| t == "duplicate widget"));
        assert!(texts.iter().any(|t| t.starts_with("x=900.0 out of 0.0..819.0")));
        assert!(texts.iter().any(|t| t.starts_with("scale=9.0 out of 0.5..4.0")));
        assert!(texts.iter().any(|t| t == "x not a number"));
        // key thiếu → default → không lỗi (parity w.get(key, default))
        let errs = validate(Some(&json!([{"id": "ping"}])),);
        assert!(errs.is_empty(), "missing keys dùng default, got {errs:?}");
        // layout hợp lệ → rỗng
        assert!(validate(Some(&default_layout())).is_empty());
    }

    #[test]
    fn sanitize_clamps_and_dedupes() {
        // không phải list → default layout
        let out = sanitize(Some(&json!(42)));
        assert_eq!(out.len(), 3);
        assert_eq!(out[0]["id"], "fps");

        let layout = json!([
            {"id": "fps", "x": -10, "y": 5000, "scale": 99.0, "visible": false},
            {"id": "nope", "x": 1, "y": 1, "scale": 1.0},
            {"id": "fps", "x": 7, "y": 7, "scale": 2.0},
            {"id": "clock", "x": 100, "y": 100},
        ]);
        let out = sanitize(Some(&layout));
        assert_eq!(out.len(), 2, "unknown + duplicate bị bỏ");
        assert_eq!(out[0]["x"], 0);
        assert_eq!(out[0]["y"], 459);
        assert_eq!(out[0]["scale"], 4.0);
        assert_eq!(out[0]["visible"], false);
        // fps thứ hai không được append (đã seen)
        assert_eq!(out[1]["id"], "clock");
        // missing key → default
        assert_eq!(out[1]["scale"], 1.0);
        assert_eq!(out[1]["visible"], true);
    }

    #[test]
    fn merge_default_appends_missing() {
        let layout = json!([{"id": "ping", "x": 10, "y": 10, "scale": 1.0}]);
        let out = merge_default(Some(&layout));
        let ids: Vec<&str> = out
            .iter()
            .filter_map(|w| w.get("id").and_then(|v| v.as_str()))
            .collect();
        assert_eq!(ids, vec!["ping", "fps", "coordinates", "clock"]);
        // None → đủ 3 default
        assert_eq!(merge_default(None).len(), 3);
    }
}
