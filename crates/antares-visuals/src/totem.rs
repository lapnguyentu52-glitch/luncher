//! Totem Studio backend — parity 1:1 `services/visuals/totem.py` (mục 9).
//!
//! MVP hướng resource pack: totem of undying hiển thị khi chủ nhân chết —
//! thay texture 32×32 `items/totem_of_undying.png` theo preset màu + pattern.
//! Model 3D thật (mục 9.2 orbit/zoom) cần renderer web — giai đoạn sau;
//! khuôn mẫu export giống crosshair (project RS + build + install).

use serde_json::{json, Value};

use crate::{hex_rgb, is_hex_color, truthy};
use antares_resources::encode_png;

pub fn default_spec() -> Value {
    json!({
        "base": "#e8b23a",      // vàng totem
        "accent": "#8c5a2b",    // nâu gỗ
        "eye": "#3ad0e8",       // màu mắt
        "glow": false,          // viền sáng quanh hình
        "wing": true,           // vẽ cánh
    })
}

pub fn presets() -> Value {
    let order = [
        ("classic", json!({"base": "#e8b23a", "accent": "#8c5a2b", "eye": "#3ad0e8", "glow": false, "wing": true})),
        ("crystal", json!({"base": "#7ef0ff", "accent": "#2b6f8c", "eye": "#ffffff", "glow": true, "wing": true})),
        ("minimal", json!({"base": "#e8b23a", "accent": "#8c5a2b", "eye": "#e8b23a", "glow": false, "wing": false})),
        ("dark",    json!({"base": "#3a3f4a", "accent": "#1c1f26", "eye": "#ff4655", "glow": true, "wing": true})),
        ("red",     json!({"base": "#ff4655", "accent": "#7a1420", "eye": "#ffd75b", "glow": true, "wing": true})),
        ("glass",   json!({"base": "#cfe8ff", "accent": "#7fa8c8", "eye": "#5b9dff", "glow": false, "wing": true})),
        ("lowpoly", json!({"base": "#f0c95a", "accent": "#a06a2c", "eye": "#2c2c2c", "glow": false, "wing": false})),
        ("competitive", json!({"base": "#ffd75b", "accent": "#8c5a2b", "eye": "#ff4655", "glow": false, "wing": true})),
    ];
    let def = default_spec();
    let list: Vec<Value> = order
        .iter()
        .map(|(id, over)| {
            let mut spec = def.as_object().cloned().unwrap_or_default();
            if let Some(o) = over.as_object() {
                for (k, v) in o {
                    spec.insert(k.clone(), v.clone());
                }
            }
            json!({"id": id, "spec": Value::Object(spec)})
        })
        .collect();
    json!({"presets": list, "default": def})
}

const SIZE: usize = 32;

/// Hình totem: thân + đầu + mắt (grid 32×32, đối xứng trục giữa).
const BODY_COLS: std::ops::Range<usize> = 12..20; // thân 8 cột giữa
const HEAD_ROWS: std::ops::Range<usize> = 4..12;
const TORSO_ROWS: std::ops::Range<usize> = 12..26;
const EYES: [(usize, usize); 2] = [(13, 7), (18, 7)]; // (x, y) mắt trái/phải

fn clamp_color(spec: &Value, key: &str, default: &str) -> String {
    match spec.get(key).and_then(|v| v.as_str()) {
        // parity: _clamp store nguyên văn (is_hex strip để check thôi)
        Some(v) if is_hex_color(v) => v.to_string(),
        _ => default.to_string(),
    }
}

/// Clone + clamp spec — parity `normalize()`.
pub fn normalize(spec: Option<&Value>) -> Value {
    let mut out = default_spec();
    let Some(spec) = spec.filter(|s| s.is_object()) else {
        return out;
    };
    out["base"] = json!(clamp_color(spec, "base", out["base"].as_str().unwrap()));
    out["accent"] = json!(clamp_color(spec, "accent", out["accent"].as_str().unwrap()));
    out["eye"] = json!(clamp_color(spec, "eye", out["eye"].as_str().unwrap()));
    out["glow"] = json!(truthy(spec.get("glow"), false));
    out["wing"] = json!(truthy(spec.get("wing"), true));
    out
}

/// Spec → PNG 32×32 texture totem (đối xứng, đủ nhận diện preset).
/// Parity `render_png()`.
pub fn render_png(spec: &Value) -> Vec<u8> {
    let s = normalize(Some(spec));
    let base = hex_rgb(s["base"].as_str().unwrap());
    let accent = hex_rgb(s["accent"].as_str().unwrap());
    let eye = hex_rgb(s["eye"].as_str().unwrap());
    let glow_a: i32 = 90;

    let mut px = vec![0u8; SIZE * SIZE * 4];
    let put = |px: &mut [u8], x: i64, y: i64, c: (u8, u8, u8), a: i32| {
        if x >= 0 && (x as usize) < SIZE && y >= 0 && (y as usize) < SIZE {
            let i = (y as usize * SIZE + x as usize) * 4;
            px[i] = c.0;
            px[i + 1] = c.1;
            px[i + 2] = c.2;
            px[i + 3] = a.clamp(0, 255) as u8;
        }
    };

    if s["glow"].as_bool().unwrap_or(false) {
        // quầng: viền ngoài thân/đầu — rect (12,4,8,22) và (8,10,16,12)
        for (x0, y0, w0, h0) in [(12i64, 4, 8, 22), (8, 10, 16, 12)] {
            for i in 0..w0 {
                put(&mut px, x0 + i, y0 - 1, base, glow_a);
                put(&mut px, x0 + i, y0 + h0, base, glow_a);
            }
            for j in 0..h0 {
                put(&mut px, x0 - 1, y0 + j, base, glow_a);
                put(&mut px, x0 + w0, y0 + j, base, glow_a);
            }
        }
    }

    // Đầu
    for y in HEAD_ROWS {
        for x in BODY_COLS.clone() {
            put(&mut px, x as i64, y as i64, base, 255);
        }
    }
    // Mắt — mỗi mắt 2 pixel ngang
    for (ex, ey) in EYES {
        put(&mut px, ex as i64, ey as i64, eye, 255);
        put(&mut px, ex as i64 + 1, ey as i64, eye, 255);
    }

    // Thân + hoạ tiết accent (zigzag)
    for y in TORSO_ROWS {
        for x in BODY_COLS.clone() {
            let accent_row = (y / 2) % 2 == 0;
            let accent_col = (x - 12) % 3 != 0;
            let c = if accent_row && accent_col { accent } else { base };
            put(&mut px, x as i64, y as i64, c, 255);
        }
    }
    // Chân
    for y in 26..30 {
        for x in [13usize, 14, 17, 18] {
            put(&mut px, x as i64, y as i64, accent, 255);
        }
    }

    // Cánh đối xứng
    if s["wing"].as_bool().unwrap_or(true) {
        for dy in 0..10usize {
            for dx in 0..(5 - dy / 2) {
                put(&mut px, (8 - dx) as i64, (10 + dy) as i64, accent, 220);
                put(&mut px, (23 + dx) as i64, (10 + dy) as i64, accent, 220);
            }
        }
    }

    encode_png(SIZE as u32, SIZE as u32, &px).expect("totem size hợp lệ")
}

/// Quick preset → VoxelSpec (mục 70 migration: Quick → Advanced).
/// Parity `voxel_spec()` — voxel 16³: thân + đầu + 2 mắt, khớp texture 32×32.
pub fn voxel_spec(spec: Option<&Value>) -> Value {
    let s = normalize(spec);
    let faces = |tex: &str| {
        json!({
            "north": {"texture": tex}, "south": {"texture": tex},
            "east": {"texture": tex}, "west": {"texture": tex},
            "up": {"texture": tex}, "down": {"texture": tex},
        })
    };
    json!({
        "grid": 16,
        "cubes": [
            {"id": "body", "from": [4, 0, 4], "to": [12, 10, 12], "faces": faces("base")},
            {"id": "head", "from": [4, 10, 4], "to": [12, 16, 12], "faces": faces("accent")},
            {"id": "eye-l", "from": [5, 14, 3], "to": [7, 15, 4],
             "faces": {"north": {"texture": "eye"}}},
            {"id": "eye-r", "from": [9, 14, 3], "to": [11, 15, 4],
             "faces": {"north": {"texture": "eye"}}},
        ],
        "textures": {
            "base": s["base"], "accent": s["accent"], "eye": s["eye"],
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_parity_order() {
        let p = presets();
        let arr = p["presets"].as_array().unwrap();
        assert_eq!(arr.len(), 8);
        let ids: Vec<&str> = arr.iter().map(|x| x["id"].as_str().unwrap()).collect();
        assert_eq!(
            ids,
            ["classic", "crystal", "minimal", "dark", "red", "glass", "lowpoly", "competitive"]
        );
        // crystal glow=true, minimal wing=false
        assert_eq!(arr[1]["spec"]["glow"], true);
        assert_eq!(arr[2]["spec"]["wing"], false);
        // merge giữ màu mặc định cho field không override (mọi preset override đủ 5 field)
        assert_eq!(p["default"]["base"], "#e8b23a");
    }

    #[test]
    fn normalize_clamps_colors() {
        assert_eq!(normalize(None), default_spec());
        let s = normalize(Some(&json!({
            "base": "không hex",
            "accent": "#f00",
            "eye": "#3ad0e8",
            "glow": true,
            "wing": false,
        })));
        assert_eq!(s["base"], "#e8b23a", "lạ → default");
        assert_eq!(s["accent"], "#f00");
        assert_eq!(s["eye"], "#3ad0e8");
        assert_eq!(s["glow"], true);
        assert_eq!(s["wing"], false);
    }

    #[test]
    fn render_deterministic_32x32() {
        let a = render_png(&json!({"glow": true}));
        let b = render_png(&json!({"glow": true}));
        assert_eq!(a, b);
        let (w, h) = antares_resources::decode_png_dims(&a).unwrap();
        assert_eq!((w, h), (32, 32));
        // preset khác nhau → texture khác nhau
        assert_ne!(render_png(&default_spec()), render_png(&json!({"wing": false})));
    }

    #[test]
    fn voxel_spec_shape() {
        let v = voxel_spec(None);
        assert_eq!(v["grid"], 16);
        let cubes = v["cubes"].as_array().unwrap();
        assert_eq!(cubes.len(), 4);
        assert_eq!(cubes[0]["id"], "body");
        assert_eq!(cubes[2]["faces"]["north"]["texture"], "eye");
        // màu lấy từ normalize
        assert_eq!(v["textures"]["base"], "#e8b23a");
        // input màu lạ → voxel vẫn giữ default màu
        let v = voxel_spec(Some(&json!({"base": "junk"})));
        assert_eq!(v["textures"]["base"], "#e8b23a");
    }
}
