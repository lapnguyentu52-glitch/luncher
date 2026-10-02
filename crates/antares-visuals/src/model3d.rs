//! Model3D backend — VoxelSpec validation — parity 1:1
//! `services/visuals/model3d.py` (master plan v3 mục 8, 15).
//!
//! Nguồn sự thật cho Totem 3D (Batch 4 UI) và item model pipeline (Batch 2 gọi
//! validate trước khi compile — mục 14: ERROR block build, WARNING cho qua).
//!
//! Geometry rules (mục 8.1):
//! - grid 16; tọa độ cho phép float (decision Batch 0 — model MC chuẩn nhận).
//! - Element bounds Minecraft: from/to trong [-16, 32] từng trục.
//! - from < to từng trục; không NaN/Infinity.
//! - Mỗi cube ID duy nhất; face key chỉ 6 mặt hợp lệ.
//! - UV: [x1, y1, x2, y2] với x1 < x2, y1 < y2 (đảo = ERROR fixable — mục 94);
//!   ngoài [0, 16] = WARNING (MC tự tile).
//!
//! Severity: INFO/WARNING/ERROR/FATAL (mục 15.1). Finding shape mục 15.2:
//! `{severity, code, path, message, fixable}`. Error codes thuộc taxonomy mục 33.

use serde_json::Value;

use crate::ModelFinding;

pub const GRID: i64 = 16;

/// Giới hạn element Minecraft (block coords): [-16, 32]
pub const COORD_MIN: f64 = -16.0;
pub const COORD_MAX: f64 = 32.0;

/// Policy số cube (mục 35): 32 khuyến nghị, 64 warning, 256 hard cap
pub const RECOMMENDED_CUBES: usize = 32;
pub const WARN_CUBES: usize = 64;
pub const MAX_CUBES: usize = 256;

const FACES: [&str; 6] = ["north", "south", "east", "west", "up", "down"];

fn finding(severity: &'static str, code: &str, path: &str, message: impl Into<String>, fixable: bool) -> ModelFinding {
    ModelFinding {
        severity,
        code: code.to_string(),
        path: path.to_string(),
        message: message.into(),
        fixable,
    }
}

/// Severity ERROR/FATAL → chặn build (mục 14).
pub fn has_errors(findings: &[ModelFinding]) -> bool {
    findings
        .iter()
        .any(|f| f.severity == "ERROR" || f.severity == "FATAL")
}

pub fn errors(findings: &[ModelFinding]) -> Vec<&ModelFinding> {
    findings
        .iter()
        .filter(|f| f.severity == "ERROR" || f.severity == "FATAL")
        .collect()
}

pub fn warnings(findings: &[ModelFinding]) -> Vec<&ModelFinding> {
    findings.iter().filter(|f| f.severity == "WARNING").collect()
}

fn is_finite_num(v: &Value) -> Option<f64> {
    // parity: Python NHẬN ĐÌNH reject bool ở đây (`isinstance(v, bool)` →
    // ERROR "không phải số hữu hạn") — Value::Bool không phải Number nên tự
    // rơi vào None → path ERROR. Đúng semantics model3d (khác hud/renderer
    // nơi float(True)=1.0 được chấp nhận).
    match v {
        Value::Number(n) => n.as_f64().filter(|f| f.is_finite()),
        _ => None,
    }
}

fn is_hex(value: &str) -> bool {
    crate::is_hex_color(value)
}

fn is_asset_path(value: &str) -> bool {
    let v = value.trim().replace('\\', "/");
    let Some(stripped) = v.strip_prefix("assets/") else {
        return false;
    };
    let Some(inner) = stripped.strip_suffix(".png") else {
        return false;
    };
    inner.contains('/')
}

/// VoxelSpec → list findings. [] = hợp lệ hoàn toàn. Parity `validate()`.
pub fn validate(spec: Option<&Value>) -> Vec<ModelFinding> {
    let mut out: Vec<ModelFinding> = Vec::new();
    let Some(spec) = spec.filter(|s| s.is_object()) else {
        out.push(finding(
            "FATAL",
            "MODEL_SPEC_INVALID",
            "model3d",
            "Model spec phải là object",
            false,
        ));
        return out;
    };

    match spec.get("grid") {
        None => {} // default GRID — parity: get("grid", GRID); chỉ reject khi có giá trị sai
        Some(v) => {
            // parity: isinstance(True, int) == True trong Python → bool cũng đi qua
            let ok = v
                .as_i64()
                .map(|g| g > 0)
                .or_else(|| v.as_bool())
                .unwrap_or(false);
            if !ok {
                out.push(finding(
                    "ERROR",
                    "MODEL_SPEC_INVALID",
                    "model3d.grid",
                    format!("grid phải là int dương, thấy {}", fmt_val(v)),
                    false,
                ));
            }
        }
    }

    let Some(cubes) = spec.get("cubes") else {
        out.push(finding(
            "FATAL",
            "MODEL_SPEC_INVALID",
            "model3d.cubes",
            "cubes phải là array",
            false,
        ));
        return out;
    };
    let Some(cubes) = cubes.as_array() else {
        out.push(finding(
            "FATAL",
            "MODEL_SPEC_INVALID",
            "model3d.cubes",
            "cubes phải là array",
            false,
        ));
        return out;
    };
    if cubes.is_empty() {
        out.push(finding(
            "ERROR",
            "MODEL_NO_CUBES",
            "model3d.cubes",
            "Model không có cube nào",
            true,
        ));
        return out;
    }

    if cubes.len() > MAX_CUBES {
        out.push(finding(
            "FATAL",
            "MODEL_CUBE_LIMIT",
            "model3d.cubes",
            format!("{} cubes > hard cap {MAX_CUBES}", cubes.len()),
            false,
        ));
    } else if cubes.len() > WARN_CUBES {
        out.push(finding(
            "WARNING",
            "MODEL_CUBE_LIMIT",
            "model3d.cubes",
            format!("{} cubes > {WARN_CUBES} — có thể chậm", cubes.len()),
            false,
        ));
    }

    let textures = spec.get("textures");
    if let Some(t) = textures {
        if !t.is_object() {
            out.push(finding(
                "ERROR",
                "MODEL_TEXTURE_INVALID",
                "model3d.textures",
                "textures phải là object",
                false,
            ));
        }
    }
    let empty = serde_json::Map::new();
    let textures_obj = textures.and_then(|t| t.as_object()).unwrap_or(&empty);

    let mut seen_ids: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for (i, cube) in cubes.iter().enumerate() {
        out.extend(validate_cube(i, cube, textures_obj, &mut seen_ids));
    }
    out
}

fn fmt_val(v: &Value) -> String {
    match v {
        Value::String(s) => format!("'{s}'"),
        other => other.to_string(),
    }
}

fn validate_cube(
    i: usize,
    cube: &Value,
    textures: &serde_json::Map<String, Value>,
    seen_ids: &mut std::collections::HashMap<String, usize>,
) -> Vec<ModelFinding> {
    let mut out: Vec<ModelFinding> = Vec::new();
    let path = format!("model3d.cubes[{i}]");
    let Some(cube) = cube.as_object() else {
        out.push(finding(
            "ERROR",
            "MODEL_SPEC_INVALID",
            &path,
            "cube phải là object",
            false,
        ));
        return out;
    };

    // ID — duplicate chỉ warning (normalize có thể rename)
    match cube.get("id").and_then(|v| v.as_str()) {
        Some(cid) if !cid.is_empty() => {
            if let Some(prev) = seen_ids.get(cid) {
                out.push(finding(
                    "WARNING",
                    "MODEL_ID_DUPLICATE",
                    &format!("{path}.id"),
                    format!("ID '{cid}' trùng cubes[{prev}]"),
                    false,
                ));
            } else {
                seen_ids.insert(cid.to_string(), i);
            }
        }
        _ => {
            out.push(finding(
                "ERROR",
                "MODEL_SPEC_INVALID",
                &format!("{path}.id"),
                "cube.id phải là string không rỗng",
                true,
            ));
        }
    }

    // from/to — số hợp lệ + bounds
    let mut coords: std::collections::HashMap<&str, [f64; 3]> = std::collections::HashMap::new();
    for key in ["from", "to"] {
        let raw = cube.get(key);
        let Some(vals) = raw.and_then(|v| v.as_array()) else {
            out.push(finding(
                "ERROR",
                "MODEL_COORD_INVALID",
                &format!("{path}.{key}"),
                format!("{key} phải là array 3 số"),
                true,
            ));
            coords.insert(key, [0.0; 3]);
            continue;
        };
        if vals.len() != 3 {
            out.push(finding(
                "ERROR",
                "MODEL_COORD_INVALID",
                &format!("{path}.{key}"),
                format!("{key} phải là array 3 số"),
                true,
            ));
            coords.insert(key, [0.0; 3]);
            continue;
        }
        let mut xyz = [0.0f64; 3];
        for (j, v) in vals.iter().enumerate() {
            match is_finite_num(v) {
                Some(fv) => {
                    if !(COORD_MIN..=COORD_MAX).contains(&fv) {
                        out.push(finding(
                            "ERROR",
                            "MODEL_COORD_INVALID",
                            &format!("{path}.{key}[{j}]"),
                            format!("{fv} ngoài giới hạn element [{COORD_MIN}, {COORD_MAX}]"),
                            false,
                        ));
                    }
                    xyz[j] = fv;
                }
                None => {
                    out.push(finding(
                        "ERROR",
                        "MODEL_COORD_INVALID",
                        &format!("{path}.{key}[{j}]"),
                        format!("giá trị không phải số hữu hạn: {}", fmt_val(v)),
                        true,
                    ));
                    xyz[j] = 0.0;
                }
            }
        }
        coords.insert(key, xyz);
    }

    // Zero/negative size
    let frm = coords["from"];
    let to = coords["to"];
    for (axis, a) in ["x", "y", "z"].iter().enumerate() {
        let size = to[axis] - frm[axis];
        if size <= 0.0 {
            let word = if size == 0.0 { "không dương" } else { "âm" };
            out.push(finding(
                "ERROR",
                "MODEL_CUBE_EMPTY",
                &path,
                format!("Cube có kích thước {word} trục {a}"),
                true,
            ));
        }
    }

    // Faces
    let faces = cube.get("faces");
    if let Some(f) = faces {
        if !f.is_object() {
            out.push(finding(
                "ERROR",
                "MODEL_FACE_INVALID",
                &format!("{path}.faces"),
                "faces phải là object",
                false,
            ));
        }
    }
    if let Some(fmap) = faces.and_then(|f| f.as_object()) {
        for (fname, fs) in fmap {
            if !FACES.contains(&fname.as_str()) {
                out.push(finding(
                    "ERROR",
                    "MODEL_FACE_INVALID",
                    &format!("{path}.faces.{fname}"),
                    format!("'{}' không phải mặt hợp lệ ({})", fname, FACES.join(", ")),
                    true,
                ));
                continue;
            }
            let fpath = format!("{path}.faces.{fname}");
            let Some(fs) = fs.as_object() else {
                if !fs.is_null() {
                    out.push(finding(
                        "ERROR",
                        "MODEL_FACE_INVALID",
                        &fpath,
                        "face spec phải là object",
                        false,
                    ));
                }
                continue;
            };
            out.extend(validate_face_material(&fpath, fs.get("texture"), textures));
            out.extend(validate_uv(&fpath, fs.get("uv")));
        }
    }
    out
}

/// Texture ref: hex trực tiếp | key trong spec.textures | asset path.
fn validate_face_material(
    fpath: &str,
    tex: Option<&Value>,
    textures: &serde_json::Map<String, Value>,
) -> Vec<ModelFinding> {
    let Some(tex) = tex else {
        return Vec::new(); // missing face -> default "base" ở compiler
    };
    let Some(s) = tex.as_str() else {
        return vec![finding(
            "ERROR",
            "MODEL_TEXTURE_MISSING",
            &format!("{fpath}.texture"),
            format!("texture ref không hợp lệ: {}", fmt_val(tex)),
            false,
        )];
    };
    let v = s.trim();
    if v.is_empty() {
        return vec![finding(
            "ERROR",
            "MODEL_TEXTURE_MISSING",
            &format!("{fpath}.texture"),
            format!("texture ref không hợp lệ: {}", fmt_val(tex)),
            false,
        )];
    }
    if is_hex(v) {
        return Vec::new();
    }
    if is_asset_path(v) {
        return Vec::new();
    }
    match textures.get(v) {
        Some(target) => {
            let ok = target
                .as_str()
                .map(|t| {
                    let t = t.trim();
                    is_hex(t) || is_asset_path(t)
                })
                .unwrap_or(false);
            if ok {
                Vec::new()
            } else {
                vec![finding(
                    "ERROR",
                    "MODEL_TEXTURE_INVALID",
                    &format!("model3d.textures.{v}"),
                    format!("texture '{v}' trỏ tới giá trị không hợp lệ: {}", fmt_val(target)),
                    false,
                )]
            }
        }
        None => vec![finding(
            "ERROR",
            "MODEL_TEXTURE_MISSING",
            &format!("{fpath}.texture"),
            format!("texture '{v}' không có trong spec.textures"),
            false,
        )],
    }
}

fn validate_uv(fpath: &str, uv: Option<&Value>) -> Vec<ModelFinding> {
    let Some(uv) = uv else {
        return Vec::new();
    };
    let Some(items) = uv.as_array() else {
        return vec![finding(
            "ERROR",
            "MODEL_UV_INVALID",
            &format!("{fpath}.uv"),
            "uv phải là array 4 số",
            true,
        )];
    };
    if items.len() != 4 || items.iter().any(|u| is_finite_num(u).is_none()) {
        return vec![finding(
            "ERROR",
            "MODEL_UV_INVALID",
            &format!("{fpath}.uv"),
            "uv phải là array 4 số",
            true,
        )];
    }
    let nums: Vec<f64> = items.iter().filter_map(is_finite_num).collect();
    let (x1, y1, x2, y2) = (nums[0], nums[1], nums[2], nums[3]);
    let mut out: Vec<ModelFinding> = Vec::new();
    if x2 <= x1 || y2 <= y1 {
        out.push(finding(
            "ERROR",
            "MODEL_UV_INVALID",
            &format!("{fpath}.uv"),
            format!("uv đảo chiều: [{x1}, {y1}, {x2}, {y2}] (cần x1<x2, y1<y2)"),
            true,
        ));
    }
    let g = GRID as f64;
    if !(0.0..=g).contains(&x1)
        || !(0.0..=g).contains(&x2)
        || !(0.0..=g).contains(&y1)
        || !(0.0..=g).contains(&y2)
    {
        out.push(finding(
            "WARNING",
            "MODEL_UV_OUT_OF_RANGE",
            &format!("{fpath}.uv"),
            format!("uv ngoài [0, {GRID}] — Minecraft sẽ tile texture"),
            false,
        ));
    }
    out
}

/// Clone + fix deterministic: rename duplicate ID, bỏ cube lỗi nghiêm trọng.
/// Chỉ dùng cho render/preview; build pipeline phải qua `validate()` trước.
/// Parity `normalize()`.
pub fn normalize(spec: Option<&Value>) -> Value {
    let cubes_val = spec.and_then(|s| s.get("cubes")).cloned().unwrap_or(Value::Null);
    let Some(cubes) = cubes_val.as_array() else {
        return serde_json::json!({"grid": GRID, "cubes": [], "textures": {}});
    };
    let Some(spec) = spec else {
        return serde_json::json!({"grid": GRID, "cubes": [], "textures": {}});
    };
    // parity `spec.get("grid", GRID) or GRID`: truthy giữ nguyên (kể cả -5,
    // bool), falsy (0/null/thiếu) → GRID
    let grid_val = spec.get("grid").cloned().unwrap_or(serde_json::json!(GRID));
    let grid = if crate::truthy(Some(&grid_val), false) {
        grid_val
    } else {
        serde_json::json!(GRID)
    };
    let mut out = serde_json::json!({
        "grid": grid,
        "textures": spec.get("textures").cloned().unwrap_or(serde_json::json!({})),
    });
    let mut used: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut clean: Vec<Value> = Vec::new();
    for cube in cubes {
        let Some(c) = cube.as_object() else {
            continue;
        };
        let mut c = c.clone();
        // parity `cid = c.get("id"); if not isinstance(cid, str) or not cid:` —
        // non-string (number) cũng rơi vào nhánh "cube"
        let base = c
            .get("id")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("cube")
            .to_string();
        let mut cid = base.clone();
        let mut n = 2;
        while used.contains(&cid) {
            cid = format!("{base}-{n}");
            n += 1;
        }
        used.insert(cid.clone());
        c.insert("id".into(), Value::String(cid));
        clean.push(Value::Object(c));
    }
    out["cubes"] = Value::Array(clean);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn good_cube(id: &str) -> Value {
        json!({
            "id": id,
            "from": [0.0, 0.0, 0.0],
            "to": [4.0, 4.0, 4.0],
            "faces": {"north": {"texture": "#ffffff"}},
        })
    }

    #[test]
    fn valid_spec_passes_clean() {
        let spec = json!({
            "grid": 16,
            "cubes": [good_cube("body"), good_cube("head")],
            "textures": {"base": "#e8b23a"},
        });
        let f = validate(Some(&spec));
        assert!(f.is_empty(), "clean spec, got {f:?}");
    }

    #[test]
    fn non_object_and_missing_cubes_are_fatal() {
        let f = validate(None);
        assert_eq!(f[0].severity, "FATAL");
        assert_eq!(f[0].code, "MODEL_SPEC_INVALID");

        let f = validate(Some(&json!({"grid": 16})));
        assert_eq!(f[0].severity, "FATAL");
        assert_eq!(f[0].code, "MODEL_SPEC_INVALID");

        let f = validate(Some(&json!({"cubes": []})));
        assert_eq!(f[0].code, "MODEL_NO_CUBES");
        assert!(f[0].fixable);
        assert!(has_errors(&f));

        // finding shape mục 15.2
        assert_eq!(f[0].path, "model3d.cubes");
    }

    #[test]
    fn grid_and_cube_count_policies() {
        let f = validate(Some(&json!({"grid": 0, "cubes": [good_cube("a")]})));
        assert!(f.iter().any(|x| x.code == "MODEL_SPEC_INVALID" && x.path == "model3d.grid"));

        // >64 cubes → WARNING
        let many: Vec<Value> = (0..70).map(|i| good_cube(&format!("c{i}"))).collect();
        let f = validate(Some(&json!({ "cubes": many })));
        assert!(f.iter().any(|x| x.severity == "WARNING" && x.code == "MODEL_CUBE_LIMIT"));
        assert!(!has_errors(&f), "70 cubes chỉ warning");

        // >256 → FATAL
        let many: Vec<Value> = (0..300).map(|i| good_cube(&format!("c{i}"))).collect();
        let f = validate(Some(&json!({ "cubes": many })));
        assert!(f.iter().any(|x| x.severity == "FATAL" && x.code == "MODEL_CUBE_LIMIT"));
    }

    #[test]
    fn cube_geometry_rules() {
        // id rỗng, coords sai, size 0, ngoài bounds, NaN-like (string)
        let spec = json!({
            "cubes": [
                {"id": "", "from": [0, 0, 0], "to": [1, 1, 1]},
                {"id": "b", "from": [0, 0], "to": [1, 1, 1]},
                {"id": "c", "from": [2, 2, 2], "to": [2, 3, 3]},
                {"id": "d", "from": [-20, 0, 0], "to": [1, 1, 1]},
                {"id": "e", "from": [0, "x", 0], "to": [1, 1, 1]},
            ]
        });
        let f = validate(Some(&spec));
        let codes: Vec<&str> = f.iter().map(|x| x.code.as_str()).collect();
        assert!(codes.contains(&"MODEL_SPEC_INVALID")); // id rỗng (fixable)
        assert!(codes.contains(&"MODEL_COORD_INVALID")); // from 2 số + ngoài bounds + string
        assert!(codes.contains(&"MODEL_CUBE_EMPTY")); // size 0
        assert!(has_errors(&f));
        // fixable flags
        assert!(f.iter().any(|x| x.code == "MODEL_CUBE_EMPTY" && x.fixable));
        assert!(f.iter().any(|x| x.message.contains("ngoài giới hạn") && !x.fixable));
    }

    #[test]
    fn face_and_texture_rules() {
        let spec = json!({
            "cubes": [
                {"id": "a", "from": [0, 0, 0], "to": [4, 4, 4],
                 "faces": {
                     "front": {"texture": "#ffffff"},
                     "up": {"texture": "base"},
                     "down": {"texture": "missing-key"},
                     "north": {"texture": 42},
                     "south": {"uv": [4, 4, 1, 1]},
                     "east": {"uv": [-2, 0, 20, 1]},
                 }},
            ],
            "textures": {"base": "#e8b23a"},
        });
        let f = validate(Some(&spec));
        let codes: Vec<&str> = f.iter().map(|x| x.code.as_str()).collect();
        assert!(codes.contains(&"MODEL_FACE_INVALID"), "front không hợp lệ");
        assert!(codes.contains(&"MODEL_TEXTURE_MISSING"), "missing-key");
        assert!(codes.contains(&"MODEL_TEXTURE_MISSING"), "texture number");
        assert!(codes.contains(&"MODEL_UV_INVALID"), "uv đảo");
        assert!(codes.contains(&"MODEL_UV_OUT_OF_RANGE"), "uv tile warning");
        // uv ngoài range là WARNING — không block
        assert!(f.iter().any(|x| x.code == "MODEL_UV_OUT_OF_RANGE" && x.severity == "WARNING"));
        // texture key hợp lệ → không lỗi
        let ok = json!({
            "cubes": [{"id": "a", "from": [0, 0, 0], "to": [1, 1, 1],
                       "faces": {"up": {"texture": "base"}}}],
            "textures": {"base": "#e8b23a"},
        });
        assert!(validate(Some(&ok)).is_empty());
        // asset path OK
        let ok2 = json!({
            "cubes": [{"id": "a", "from": [0, 0, 0], "to": [1, 1, 1],
                       "faces": {"up": {"texture": "assets/minecraft/textures/x.png"}}}]
        });
        assert!(validate(Some(&ok2)).is_empty());
        // textures trỏ tới giá trị sai
        let bad = json!({
            "cubes": [{"id": "a", "from": [0, 0, 0], "to": [1, 1, 1],
                       "faces": {"up": {"texture": "base"}}}],
            "textures": {"base": 123},
        });
        let f = validate(Some(&bad));
        assert!(f.iter().any(|x| x.code == "MODEL_TEXTURE_INVALID"));
    }

    #[test]
    fn duplicate_id_is_warning_only() {
        let spec = json!({
            "cubes": [good_cube("dup"), good_cube("dup")],
        });
        let f = validate(Some(&spec));
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].code, "MODEL_ID_DUPLICATE");
        assert_eq!(f[0].severity, "WARNING");
        assert!(f[0].message.contains("trùng cubes[0]"));
    }

    #[test]
    fn normalize_renames_and_drops() {
        let spec = json!({
            "grid": 16,
            "cubes": [
                {"id": "dup", "from": [0, 0, 0], "to": [1, 1, 1]},
                {"id": "dup", "from": [0, 0, 0], "to": [1, 1, 1]},
                "junk",
            ],
        });
        let n = normalize(Some(&spec));
        let cubes = n["cubes"].as_array().unwrap();
        assert_eq!(cubes.len(), 2, "cube không phải object bị bỏ");
        assert_eq!(cubes[0]["id"], "dup");
        assert_eq!(cubes[1]["id"], "dup-2");
        // id rỗng → "cube"
        let n = normalize(Some(&json!({"cubes": [{"id": "", "from": [0, 0, 0], "to": [1, 1, 1]}]})));
        assert_eq!(n["cubes"][0]["id"], "cube");
        // spec hỏng → skeleton rỗng
        let n = normalize(Some(&json!("junk")));
        assert_eq!(n["cubes"].as_array().unwrap().len(), 0);
        assert_eq!(n["grid"], 16);
    }

    #[test]
    fn errors_and_warnings_filters() {
        let spec = json!({
            "cubes": [
                {"id": "", "from": [0, 0, 0], "to": [1, 1, 1]},
                {"id": "dup", "from": [0, 0, 0], "to": [1, 1, 1]},
                {"id": "dup", "from": [0, 0, 0], "to": [1, 1, 1]},
            ]
        });
        let f = validate(Some(&spec));
        // 1 ERROR (id rỗng) + 1 WARNING (dup) — cube 3 không tạo thêm finding
        assert_eq!(errors(&f).len(), 1);
        assert_eq!(warnings(&f).len(), 1);
        assert!(has_errors(&f));
        // cube không phải object → ERROR "cube phải là object"
        let f = validate(Some(&json!({"cubes": ["junk"]})));
        assert_eq!(f[0].code, "MODEL_SPEC_INVALID");
        assert!(f[0].message.contains("cube phải là object"));
    }
}
