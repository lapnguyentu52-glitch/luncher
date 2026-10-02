//! fx.rs — Hit Effects + Particles renderer — parity 1:1
//! `services/visuals/fx.py` (spec 3.0 mục 52 subsections).
//!
//! Mỗi kind export ra **1 texture PNG (+ .mcmeta cho particle animation)**:
//!
//! - **hit**: overlay màn hình khi trúng đòn (mục 8.1 "Damage flash" /
//!   mục 52 "Hit Effects") → `textures/gui/sprites/hit.png` (128×128, alpha
//!   theo loại effect). Texture override thuần resource pack — không cần mod.
//! - **particles**: texture atlas animation (chuẩn MC: frame 16×16 xếp dọc
//!   1 cột) + `.mcmeta` khai báo frames — `textures/particle/glow.png`.
//!
//! Sanitize + clamp mọi field (path safety, mục 76): màu hex, số nguyên bounded.

use serde_json::{json, Value};

use crate::{hex_rgb, truthy};
use antares_resources::encode_png;

// ------------------------------------------------------------------
// Hit Effects (mục 8.1 damage flash / mục 52 Hit Effects)
// ------------------------------------------------------------------

pub const HIT_KINDS: [&str; 5] = ["none", "flash", "vignette", "arrow", "cross"];
pub const HIT_SIZE: usize = 128;
pub const HIT_MAX_ALPHA: i64 = 200;

/// Mặc định + ranh giới (mục 76: clamp mọi input).
pub fn default_hit() -> Value {
    json!({
        "kind": "flash",
        "color": "#ff3b30",
        "alpha": 120,           // 0..HIT_MAX_ALPHA
        "size": 70,             // % kích thước ảnh (vignette/cross/arrow)
    })
}

fn num(v: Option<&Value>) -> Option<f64> {
    v.and_then(|x| x.as_f64()) // bool không phải Number trong serde_json → tự loại
}

/// parity `fx._is_hex`: CHỈ nhận `#rrggbb` 6 chữ số (khác crosshair/totem
/// nhận thêm `#rgb` 3 chữ số).
fn is_hex7(c: &str) -> bool {
    let body = c.strip_prefix('#').unwrap_or("");
    body.len() == 6 && body.chars().all(|ch| ch.is_ascii_hexdigit())
}

/// Parity `normalize_hit()`.
pub fn normalize_hit(spec: Option<&Value>) -> Value {
    let mut out = default_hit();
    let Some(spec) = spec.filter(|s| s.is_object()) else {
        return out;
    };
    if let Some(k) = spec.get("kind").and_then(|v| v.as_str()) {
        if HIT_KINDS.contains(&k) {
            out["kind"] = json!(k);
        }
    }
    if let Some(Value::String(c)) = spec.get("color") {
        if is_hex7(c) {
            out["color"] = json!(c.trim());
        }
    }
    if let Some(a) = num(spec.get("alpha")) {
        out["alpha"] = json!(a.trunc().clamp(0.0, HIT_MAX_ALPHA as f64) as i64);
    }
    if let Some(z) = num(spec.get("size")) {
        out["size"] = json!(z.trunc().clamp(20.0, 100.0) as i64);
    }
    out
}

/// Hit overlay 128×128 RGBA. kind=none → texture trong suốt hoàn toàn
/// (export 'none' chỉ có ý nghĩa reset về mặc định game). Parity `render_hit_png`.
pub fn render_hit_png(spec: &Value) -> Vec<u8> {
    let s = normalize_hit(Some(spec));
    let (r, g, b) = hex_rgb(s["color"].as_str().unwrap_or("#ff3b30"));
    let a_hit = s["alpha"].as_i64().unwrap_or(120).clamp(0, 255) as i32;
    let size = HIT_SIZE;
    let mut px = vec![0u8; size * size * 4];

    let kind = s["kind"].as_str().unwrap_or("flash").to_string();
    if kind != "none" {
        let frac = s["size"].as_f64().unwrap_or(70.0) / 100.0;
        let half = size as f64 / 2.0;
        let inner = half * frac; // bán kính vùng effect chính

        for y in 0..size {
            for x in 0..size {
                let dx = x as f64 + 0.5 - half;
                let dy = y as f64 + 0.5 - half;
                let dist = (dx * dx + dy * dy).sqrt();
                let alpha: i32 = match kind.as_str() {
                    "flash" => {
                        // radial fade: đặc ở tâm → suốt ở rìa
                        if dist < inner {
                            let t = dist / inner;
                            (a_hit as f64 * (1.0 - t * 0.85)) as i32
                        } else {
                            continue;
                        }
                    }
                    "vignette" => {
                        // ngược flash: rìa đậm, tâm giữ
                        if dist > inner {
                            let t = ((dist - inner) / (half - inner).max(1.0)).min(1.0);
                            (a_hit as f64 * t) as i32
                        } else {
                            continue;
                        }
                    }
                    "arrow" => {
                        // directional hit marker: tam giác từ mép vào tâm
                        if dx.abs() < half * 0.06 && 0.0 < dy && dy < inner {
                            let t = 1.0 - dy / inner;
                            (a_hit as f64 * (0.4 + 0.6 * t)) as i32
                        } else {
                            continue;
                        }
                    }
                    "cross" => {
                        // 4 vạch chéo hướng tâm (hit marker X)
                        let (adx, ady) = (dx.abs(), dy.abs());
                        if inner * 0.35 < dist && dist < inner && (adx - ady).abs() < half * 0.05 {
                            a_hit
                        } else {
                            continue;
                        }
                    }
                    _ => continue,
                };
                let i = (y * size + x) * 4;
                px[i] = r;
                px[i + 1] = g;
                px[i + 2] = b;
                px[i + 3] = alpha.clamp(0, 255) as u8;
            }
        }
    }
    encode_png(size as u32, size as u32, &px).expect("hit size hợp lệ")
}

// ------------------------------------------------------------------
// Particles — atlas animation (chuẩn MC: frame 16×16 xếp dọc 1 cột)
// ------------------------------------------------------------------

pub const PARTICLE_SHAPES: [&str; 5] = ["orb", "spark", "star", "ring", "smoke"];
pub const P_FRAME: usize = 16;
pub const P_MAX_FRAMES: usize = 8;

pub fn default_particle() -> Value {
    json!({
        "shape": "orb",
        "color": "#7fd4ff",
        "glow": true,
        "frames": 4,            // 1..8
    })
}

/// Parity `normalize_particle()`.
pub fn normalize_particle(spec: Option<&Value>) -> Value {
    let mut out = default_particle();
    let Some(spec) = spec.filter(|s| s.is_object()) else {
        return out;
    };
    if let Some(shape) = spec.get("shape").and_then(|v| v.as_str()) {
        if PARTICLE_SHAPES.contains(&shape) {
            out["shape"] = json!(shape);
        }
    }
    if let Some(Value::String(c)) = spec.get("color") {
        if is_hex7(c) {
            out["color"] = json!(c.trim());
        }
    }
    out["glow"] = json!(truthy(spec.get("glow"), true));
    if let Some(f) = num(spec.get("frames")) {
        out["frames"] = json!(f.trunc().clamp(1.0, P_MAX_FRAMES as f64) as i64);
    }
    out
}

/// Atlas particle: N frame 16×16 xếp DỌC (chuẩn MC) → PNG 16×(16×N).
/// Frame tiên triển khai đơn giản: shape co lại/dịu dần theo t (0..1).
/// Parity `render_particle_png`.
pub fn render_particle_png(spec: &Value) -> Vec<u8> {
    let s = normalize_particle(Some(spec));
    let (r, g, b) = hex_rgb(s["color"].as_str().unwrap_or("#7fd4ff"));
    let n = s["frames"].as_i64().unwrap_or(4).clamp(1, P_MAX_FRAMES as i64) as usize;
    let w = P_FRAME;
    let h = P_FRAME * n;
    let mut px = vec![0u8; w * h * 4];

    let half = w as f64 / 2.0;
    let shape = s["shape"].as_str().unwrap_or("orb").to_string();
    let glow = s["glow"].as_bool().unwrap_or(true);

    for f in 0..n {
        let top = f * P_FRAME;
        let t = f as f64 / (n.saturating_sub(1)).max(1) as f64; // 0..1 qua các frame
        // bán kính co dần theo frame; smoke thì loe ra
        let grow = if shape == "smoke" { t } else { 1.0 - t };
        let rad = half * (0.35 + 0.6 * grow);
        for y in 0..P_FRAME {
            for x in 0..w {
                let dx = x as f64 + 0.5 - half;
                let dy = y as f64 + 0.5 - half;
                let dist = (dx * dx + dy * dy).sqrt();
                let mut alpha: i32 = match shape.as_str() {
                    "ring" => {
                        if rad * 0.72 < dist && dist < rad {
                            (230.0 * (1.0 - t * 0.5)) as i32
                        } else {
                            continue;
                        }
                    }
                    "spark" => {
                        // 4 tia + lõi
                        let (adx, ady) = (dx.abs(), dy.abs());
                        if dist < rad * 0.3 {
                            235
                        } else if (adx < 1.2 || ady < 1.2) && dist < rad {
                            (200.0 * (1.0 - t * 0.4)) as i32
                        } else {
                            continue;
                        }
                    }
                    "star" => {
                        // 4 cánh: gần trục chéo
                        let (adx, ady) = (dx.abs(), dy.abs());
                        if dist < rad && ((adx - ady).abs() < 1.3 || dist < rad * 0.35) {
                            (225.0 * (1.0 - t * 0.35)) as i32
                        } else {
                            continue;
                        }
                    }
                    "smoke" => {
                        if dist < rad {
                            (120.0 * (1.0 - t) + 25.0) as i32
                        } else {
                            continue;
                        }
                    }
                    _ => {
                        // orb
                        if dist < rad {
                            (235.0 * (1.0 - (dist / rad).powi(2))) as i32
                        } else {
                            continue;
                        }
                    }
                };
                if alpha > 0 {
                    if glow && dist < rad * 0.55 {
                        alpha = (alpha + 25).min(255); // lõi sáng hơn
                    }
                    let i = ((top + y) * w + x) * 4;
                    px[i] = r;
                    px[i + 1] = g;
                    px[i + 2] = b;
                    px[i + 3] = alpha.clamp(0, 255) as u8;
                }
            }
        }
    }
    encode_png(w as u32, h as u32, &px).expect("particle size hợp lệ")
}

/// pack.mcmeta sibling `glow.png.mcmeta` — animation dọc chuẩn MC.
/// Parity `particle_mcmeta(frames, frametime=2)`.
pub fn particle_mcmeta(frames: i64, frametime: i64) -> String {
    let frames = frames.clamp(1, P_MAX_FRAMES as i64);
    let frametime = frametime.clamp(1, 10);
    let mut out = serde_json::to_string_pretty(&json!({
        "animation": {
            "frames": (0..frames).collect::<Vec<i64>>(),
            "frametime": frametime,
        }
    }))
    .unwrap_or_default();
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_parity() {
        assert_eq!(HIT_KINDS.len(), 5);
        assert_eq!(HIT_SIZE, 128);
        assert_eq!(HIT_MAX_ALPHA, 200);
        assert_eq!(PARTICLE_SHAPES.len(), 5);
        assert_eq!(P_FRAME, 16);
        assert_eq!(P_MAX_FRAMES, 8);
    }

    #[test]
    fn normalize_hit_clamps() {
        assert_eq!(normalize_hit(None), default_hit());
        let s = normalize_hit(Some(&json!({
            "kind": "explosion",   // lạ → flash
            "color": "red",        // không hex → default
            "alpha": 999,          // clamp 200
            "size": 5,             // clamp 20
        })));
        assert_eq!(s["kind"], "flash");
        assert_eq!(s["color"], "#ff3b30");
        assert_eq!(s["alpha"], 200);
        assert_eq!(s["size"], 20);
        // alpha 0 hợp lệ; bool không ăn; hex 3 chữ số KHÔNG được nhận (khác crosshair)
        let s = normalize_hit(Some(&json!({"alpha": 0, "size": true, "color": "#abc"})));
        assert_eq!(s["alpha"], 0);
        assert_eq!(s["size"], 70);
        assert_eq!(s["color"], "#ff3b30");
    }

    #[test]
    fn hit_kinds_render_deterministic_and_differ() {
        for kind in HIT_KINDS {
            let spec = json!({"kind": kind});
            let a = render_hit_png(&spec);
            let b = render_hit_png(&spec);
            assert_eq!(a, b, "deterministic {kind}");
            let (w, h) = antares_resources::decode_png_dims(&a).unwrap();
            assert_eq!((w, h), (128, 128));
        }
        // Check alpha trên pixel RGBA đã decode (raw PNG bytes ≠ RGBA).
        let none = decode_rgba_for_test(&render_hit_png(&json!({"kind": "none"})), 128, 128);
        assert!(none.iter().skip(3).step_by(4).all(|&a| a == 0), "none → trong suốt");
        let flash_png = render_hit_png(&json!({"kind": "flash"}));
        let cross_png = render_hit_png(&json!({"kind": "cross"}));
        assert_ne!(flash_png, cross_png);
        let flash = decode_rgba_for_test(&flash_png, 128, 128);
        // flash có pixel đậm gần tâm (alpha ≥ a*0.15) và pixel suốt ở góc
        let c = (64 * 128 + 64) * 4 + 3;
        assert!(flash[c] > 0);
        assert_eq!(flash[3], 0);
    }

    /// Decode RGBA từ PNG do `encode_png` sinh (deflate-stored, filter 0) — helper test.
    fn decode_rgba_for_test(png: &[u8], w: u32, h: u32) -> Vec<u8> {
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
        // Inflate deflate-stored: hdr(1) + LEN(2) + NLEN(2) = 5 byte.
        let mut pos = 2usize;
        let mut raw = Vec::new();
        loop {
            let hdr = idat[pos];
            let bfinal = hdr & 1;
            assert_eq!((hdr >> 1) & 3, 0, "chỉ stored block");
            let len = u16::from_le_bytes([idat[pos + 1], idat[pos + 2]]) as usize;
            raw.extend_from_slice(&idat[pos + 5..pos + 5 + len]);
            pos += 5 + len;
            if bfinal == 1 {
                break;
            }
        }
        let stride = (w as usize) * 4 + 1;
        let mut out = Vec::with_capacity((w * h) as usize * 4);
        for y in 0..h as usize {
            assert_eq!(raw[y * stride], 0, "filter byte phải 0");
            out.extend_from_slice(&raw[y * stride + 1..(y + 1) * stride]);
        }
        out
    }

    #[test]
    fn normalize_particle_clamps() {
        assert_eq!(normalize_particle(None), default_particle());
        let s = normalize_particle(Some(&json!({
            "shape": "cube",     // lạ → orb
            "frames": 99,        // clamp 8
            "glow": false,
        })));
        assert_eq!(s["shape"], "orb");
        assert_eq!(s["frames"], 8);
        assert_eq!(s["glow"], false);
        let s = normalize_particle(Some(&json!({"frames": 0})));
        assert_eq!(s["frames"], 1);
    }

    #[test]
    fn particle_atlas_dims_and_determinism() {
        let a = render_particle_png(&json!({"frames": 3}));
        let b = render_particle_png(&json!({"frames": 3}));
        assert_eq!(a, b);
        let (w, h) = antares_resources::decode_png_dims(&a).unwrap();
        assert_eq!((w, h), (16, 48), "atlas dọc 16×(16×3)");
        // frames khác nhau → atlas khác nhau
        assert_ne!(a, render_particle_png(&json!({"frames": 4})));
    }

    #[test]
    fn mcmeta_parity_shape() {
        let m = particle_mcmeta(4, 2);
        assert!(m.ends_with('\n'));
        let v: Value = serde_json::from_str(&m).unwrap();
        assert_eq!(v["animation"]["frames"], json!([0, 1, 2, 3]));
        assert_eq!(v["animation"]["frametime"], 2);
        // clamp
        let m = particle_mcmeta(99, 0);
        let v: Value = serde_json::from_str(&m).unwrap();
        assert_eq!(v["animation"]["frames"].as_array().unwrap().len(), 8);
        assert_eq!(v["animation"]["frametime"], 1);
    }
}
