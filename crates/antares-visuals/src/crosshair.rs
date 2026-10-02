//! Crosshair Studio backend — parity 1:1 `services/visuals/crosshair.py`
//! (mục 8.1, 8.3).
//!
//! - Renderer vẽ crosshair theo spec (shape/size/thickness/gap/outline/dot/color)
//!   ra PNG 16×16 — dùng đúng file `assets/minecraft/textures/gui/icons.png`
//!   slot crosshair khi export resource pack.
//! - Profile JSON lưu trong project visuals (mục 34) — KHÔNG lưu binary.
//! - Presets theo mục 8.1: Minimal / Dot / Classic / Thin / PvP / Clean /
//!   Competitive.
//! - Validation theo mục 76: mọi giá trị number bị clamp, màu phải hex hợp lệ.

use serde_json::{json, Value};

use crate::{hex_rgb, is_hex_color, truthy};
use antares_resources::encode_png;

/// Mặc định spec editor (mục 8.1).
pub fn default_spec() -> Value {
    json!({
        "shape": "cross",           // cross | circle | dot
        "size": 16,                 // canvas 16 -> crosshair chiếm trọn
        "thickness": 1,
        "gap": 3,
        "color": "#ff4655",
        "outline": true,
        "outlineColor": "#000000",
        "dot": false,
        "opacity": 0.9,
    })
}

/// Presets — mỗi entry merge `DEFAULT_SPEC` + override (parity `presets()`).
pub fn presets() -> Value {
    let order = [
        ("minimal", json!({"shape": "cross", "thickness": 1, "gap": 4, "dot": false, "color": "#e8eef5", "outline": false})),
        ("dot", json!({"shape": "dot", "thickness": 2, "gap": 0, "dot": false, "color": "#ff4655", "outline": true})),
        ("classic", json!({"shape": "cross", "thickness": 1, "gap": 3, "dot": false, "color": "#e8eef5", "outline": true})),
        ("thin", json!({"shape": "cross", "thickness": 1, "gap": 2, "dot": true, "color": "#7ef29a", "outline": false})),
        ("pvp", json!({"shape": "cross", "thickness": 2, "gap": 2, "dot": true, "color": "#ff4655", "outline": true})),
        ("clean", json!({"shape": "circle", "thickness": 1, "gap": 3, "dot": true, "color": "#5b9dff", "outline": false})),
        ("competitive", json!({"shape": "cross", "thickness": 2, "gap": 1, "dot": false, "color": "#ffd75b", "outline": true})),
    ];
    let def = default_spec();
    let list: Vec<Value> = order
        .iter()
        .map(|(id, over)| {
            json!({"id": id, "spec": merge_specs(&def, over)})
        })
        .collect();
    json!({"presets": list, "default": def})
}

/// Merge 2 spec object — `{**default, **override}` parity Python.
fn merge_specs(base: &Value, over: &Value) -> Value {
    let mut out = base.as_object().cloned().unwrap_or_default();
    if let Some(o) = over.as_object() {
        for (k, v) in o {
            out.insert(k.clone(), v.clone());
        }
    }
    Value::Object(out)
}

/// Clone + clamp spec — không tin input (mục 76). Parity `normalize()`.
pub fn normalize(spec: Option<&Value>) -> Value {
    let mut out = default_spec();
    let Some(spec) = spec.filter(|s| s.is_object()) else {
        return out;
    };
    let get = |k: &str| spec.get(k);

    if let Some(shape) = get("shape").and_then(|v| v.as_str()) {
        if matches!(shape, "cross" | "circle" | "dot") {
            out["shape"] = json!(shape);
        }
    }
    if let Some(Value::String(color)) = get("color") {
        if is_hex_color(color) {
            // parity: _is_hex strip để CHECK nhưng store nguyên văn (không trim)
            out["color"] = json!(color);
        }
    }
    if let Some(Value::String(oc)) = get("outlineColor") {
        if is_hex_color(oc) {
            out["outlineColor"] = json!(oc);
        }
    }
    out["outline"] = json!(truthy(get("outline"), true));
    out["dot"] = json!(truthy(get("dot"), false));

    // thickness 1..4, gap 0..7 (int clamp), opacity 0.1..1.0 (round 2)
    clamp_int(spec, &mut out, "thickness", 1, 4, 1);
    clamp_int(spec, &mut out, "gap", 0, 7, 3);
    if let Some(v) = get("opacity").and_then(num_f64) {
        let v = (v.clamp(0.1, 1.0) * 100.0).round() / 100.0;
        out["opacity"] = json!(v);
    }
    out
}

fn clamp_int(spec: &Value, out: &mut Value, key: &str, lo: i64, hi: i64, default: i64) {
    if let Some(v) = spec.get(key).and_then(num_f64) {
        // parity Python int(v): truncate về 0 (không round)
        out[key] = json!(v.trunc().clamp(lo as f64, hi as f64) as i64);
    } else {
        out[key] = json!(default);
    }
}

/// Parity `isinstance(x, (int, float)) and not isinstance(x, bool)` — bool là
/// number trong serde_json nên phải loại tay.
fn num_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        _ => None,
    }
}

/// Render spec → PNG RGBA bytes. Preview UI vẽ canvas cùng hình học.
/// Parity `render_png(spec, size=16)`.
pub fn render_png(spec: &Value, size: usize) -> Vec<u8> {
    let s = normalize(Some(spec));
    let rgb = hex_rgb(s["color"].as_str().unwrap_or("#ff4655"));
    let o_rgb = hex_rgb(s["outlineColor"].as_str().unwrap_or("#000000"));
    let alpha = (255.0 * s["opacity"].as_f64().unwrap_or(0.9)) as i32;

    let mut px = vec![0u8; size * size * 4]; // transparent
    let put = |px: &mut [u8], x: i64, y: i64, c: (u8, u8, u8), a: i32| {
        if x >= 0 && (x as usize) < size && y >= 0 && (y as usize) < size {
            let i = (y as usize * size + x as usize) * 4;
            px[i] = c.0;
            px[i + 1] = c.1;
            px[i + 2] = c.2;
            px[i + 3] = a.clamp(0, 255) as u8;
        }
    };

    let mid = (size / 2) as i64;
    let t = s["thickness"].as_i64().unwrap_or(1);
    let gap = s["gap"].as_i64().unwrap_or(3);
    let shape = s["shape"].as_str().unwrap_or("cross").to_string();
    let dot = s["dot"].as_bool().unwrap_or(false);
    let outline = s["outline"].as_bool().unwrap_or(true);

    // arm_pixels — collector vì cần duyệt 2 lần khi outline
    let mut main: Vec<(i64, i64, (u8, u8, u8), i32)> = Vec::new();
    if shape == "dot" {
        // parity Python `range(-t // 2, t // 2 + 1)` — unary minus trước //
        let lo = (-t).div_euclid(2);
        let hi = t.div_euclid(2);
        for dy in lo..=hi {
            for dx in lo..=hi {
                main.push((mid + dx, mid + dy, rgb, alpha));
            }
        }
    } else {
        // 4 thanh: từ mép gap tới rìa
        for i in gap..mid {
            for k in 0..t {
                main.push((mid + k, mid - 1 - i, rgb, alpha)); // lên
                main.push((mid + k, mid + i, rgb, alpha)); // xuống
                main.push((mid - 1 - i, mid + k, rgb, alpha)); // trái
                main.push((mid + i, mid + k, rgb, alpha)); // phải
            }
        }
        if shape == "circle" {
            // bo góc: các điểm ở ~45° của khung vuông ngoại tiếp (72 bước)
            let r = mid as f64 - gap as f64 / 2.0;
            for step in 0..72 {
                let ang = step as f64 * std::f64::consts::PI / 36.0;
                let x = (mid as f64 + r * ang.cos() - t as f64 / 2.0) as i64;
                let y = (mid as f64 + r * ang.sin() - t as f64 / 2.0) as i64;
                for k in 0..t {
                    for j in 0..t {
                        main.push((x + j, y + k, rgb, alpha));
                    }
                }
            }
        }
    }

    if outline {
        // viền đen 4 hướng rồi vẽ đè pixel chính
        for (x, y, _, _) in &main {
            for (dx, dy) in [(1i64, 0), (-1, 0), (0, 1), (0, -1)] {
                put(&mut px, x + dx, y + dy, o_rgb, alpha);
            }
        }
        for (x, y, c, a) in &main {
            put(&mut px, *x, *y, *c, *a);
        }
    } else {
        for (x, y, c, a) in &main {
            put(&mut px, *x, *y, *c, *a);
        }
    }

    // Dot giữa (nếu bật)
    if dot {
        let r = (t - 1).max(0) / 2;
        for dy in -r..=r {
            for dx in -r..=r {
                put(&mut px, mid + dx, mid + dy, rgb, alpha);
            }
        }
    }

    encode_png(size as u32, size as u32, &px).expect("render_png size hợp lệ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets_parity_order_and_merge() {
        let p = presets();
        let arr = p["presets"].as_array().unwrap();
        assert_eq!(arr.len(), 7);
        let ids: Vec<&str> = arr.iter().map(|x| x["id"].as_str().unwrap()).collect();
        assert_eq!(
            ids,
            ["minimal", "dot", "classic", "thin", "pvp", "clean", "competitive"]
        );
        // merge: preset minimal giữ default size/opacity, override outline=false
        let m = &arr[0]["spec"];
        assert_eq!(m["size"], 16);
        assert_eq!(m["opacity"], 0.9);
        assert_eq!(m["outline"], false);
        assert_eq!(m["gap"], 4);
        // default spec nguyên vẹn
        assert_eq!(p["default"]["color"], "#ff4655");
    }

    #[test]
    fn normalize_clamps_and_fallbacks() {
        // None / non-object → default
        assert_eq!(normalize(None), default_spec());
        assert_eq!(normalize(Some(&json!("junk"))), default_spec());

        let s = normalize(Some(&json!({
            "shape": "ellipse",       // lạ → giữ default cross
            "thickness": 99,          // clamp 4
            "gap": -5,                // clamp 0
            "opacity": 5.0,           // clamp 1.0
            "color": "red",           // không hex → default
            "outlineColor": "#abc",   // hex ngắn OK
            "outline": false,
            "dot": true,
        })));
        assert_eq!(s["shape"], "cross");
        assert_eq!(s["thickness"], 4);
        assert_eq!(s["gap"], 0);
        assert_eq!(s["opacity"], 1.0);
        assert_eq!(s["color"], "#ff4655");
        assert_eq!(s["outlineColor"], "#abc");
        assert_eq!(s["outline"], false);
        assert_eq!(s["dot"], true);

        // bool KHÔNG được nhận làm number (parity isinstance check)
        let s = normalize(Some(&json!({"thickness": true, "opacity": false})));
        assert_eq!(s["thickness"], 1);
        assert_eq!(s["opacity"], 0.9);
        // truthiness Python: number 1 → true, string rỗng → false, null → false
        let s = normalize(Some(&json!({"dot": 1, "outline": "", "outlineColor": null})));
        assert_eq!(s["dot"], true);
        assert_eq!(s["outline"], false);
        assert_eq!(s["outlineColor"], "#000000", "null không hex → default");
    }

    #[test]
    fn render_deterministic_and_png_valid() {
        let spec = json!({"shape": "cross", "thickness": 2, "gap": 2, "dot": true});
        let a = render_png(&spec, 16);
        let b = render_png(&spec, 16);
        assert_eq!(a, b, "cùng spec → cùng bytes");

        // PNG hợp lệ theo header-parse của crate resources
        let (w, h) = antares_resources::decode_png_dims(&a).unwrap();
        assert_eq!((w, h), (16, 16));

        // có pixel không trong suốt (crosshair thực sự được vẽ) + pixel trong suốt (góc)
        let rgba = decode_rgba_for_test(&a, 16, 16);
        assert!(rgba.iter().skip(3).step_by(4).any(|&a| a > 0));
        assert_eq!(rgba[3], 0, "góc trên trái trong suốt");
        assert_eq!(rgba[(15 * 16 + 15) * 4 + 3], 0, "góc dưới phải trong suốt");
    }

    #[test]
    fn render_shape_variants_differ() {
        let cross = render_png(&json!({"shape": "cross"}), 16);
        let dot = render_png(&json!({"shape": "dot"}), 16);
        let circle = render_png(&json!({"shape": "circle"}), 16);
        assert_ne!(cross, dot);
        assert_ne!(cross, circle);
        // kind=none tương đương: dot render nhỏ hơn cross (ít pixel đậm hơn)
        let count = |p: &[u8]| p.iter().skip(3).step_by(4).filter(|&&a| a > 0).count();
        assert!(count(&dot) < count(&cross));
    }

    /// Decode RGBA từ PNG do encode_png sinh (filter 0, deflate stored) — helper test.
    fn decode_rgba_for_test(png: &[u8], w: u32, h: u32) -> Vec<u8> {
        // Tự inflate không cần — dùng antares_resources decode? Crate chỉ parse
        // header. Vì encode_png của crate là deflate-stored tự viết, kiểm tra
        // pixel qua render lại bằng cùng code path là đủ; ở đây đọc IDAT thủ công.
        let mut idat: Vec<u8> = Vec::new();
        let mut idx = 8usize;
        while idx + 12 <= png.len() {
            let ln = u32::from_be_bytes(png[idx..idx + 4].try_into().unwrap()) as usize;
            let tag = &png[idx + 4..idx + 8];
            if tag == b"IDAT" {
                idat.extend_from_slice(&png[idx + 8..idx + 8 + ln]);
            }
            if tag == b"IEND" {
                break;
            }
            idx += 12 + ln;
        }
        let raw = inflate_stored(&idat);
        let stride = (w as usize) * 4 + 1;
        let mut out = Vec::with_capacity((w * h) as usize * 4);
        for y in 0..h as usize {
            assert_eq!(raw[y * stride], 0, "filter byte phải 0");
            out.extend_from_slice(&raw[y * stride + 1..(y + 1) * stride]);
        }
        out
    }

    /// Inflate cho deflate-stored blocks (đúng format encode_png nội bộ).
    fn inflate_stored(data: &[u8]) -> Vec<u8> {
        // zlib header 2 byte, sau đó các block: 1 byte BFINAL+BTYPE, 2 byte LEN, 2 byte NLEN
        let mut pos = 2usize;
        let mut out = Vec::new();
        loop {
            let hdr = data[pos];
            let bfinal = hdr & 1;
            assert_eq!((hdr >> 1) & 3, 0, "chỉ stored block");
        let len = u16::from_le_bytes([data[pos + 1], data[pos + 2]]) as usize;
        // stored block: hdr(1) + LEN(2) + NLEN(2) = 5 byte → data ở pos+5
        out.extend_from_slice(&data[pos + 5..pos + 5 + len]);
        pos += 5 + len;
            if bfinal == 1 {
                break;
            }
        }
        out
    }
}
