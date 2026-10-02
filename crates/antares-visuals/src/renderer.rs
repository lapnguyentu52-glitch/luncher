//! Static renderer — parity 1:1 `services/visuals/renderer.py` (master plan v3
//! mục 9) — VoxelSpec → PNG.
//!
//! Dùng cho: thumbnail, pack preview, WebGL fallback, test render, CI.
//! Không cần GPU — thuần Rust (mục 9 acceptance).
//!
//! Camera isometric cố định (deterministic — không orbit):
//! - View từ hướng (+x, +y, +z); 3 mặt thấy được: up (+y), east (+x), south (+z).
//! - Chiếu dimetric: sx = (x - z), sy = (x + z) * 0.5 - y.
//! - Shading cố định: up 1.0, east 0.8, south 0.62 (không lighting engine).
//!
//! Raster: z-buffer per pixel. Depth = x + y + z; viewer nằm ở +∞ theo hướng
//! (1, 1, 1) nên điểm có tổng LỚN HƠN = gần camera hơn → z-buffer giữ MAX.
//!
//! Texture sampling (mục 9: không photorealistic):
//! - "#hex" → màu solid + shading.
//! - asset path / key thiếu → màu fallback (validator đã bắt riêng ở model3d).
//!
//! Render 2 lần cùng spec + size → cùng bytes (regression test mục 9).
//!
//! NOTE parity: thuật toán solve uv dùng det của basis KHÔNG scale trong khi
//! rx/ry ở hệ đã scale (u_computed = scale * u_true) — quirk legacy giữ nguyên
//! để A/B byte-parity với Python. Fix phải đồng bộ 2 bên trong 1 parity pass.

use serde_json::Value;

use crate::model3d;

/// Shading per visible face (cố định, deterministic).
pub const SHADE_UP: f64 = 1.0;
pub const SHADE_EAST: f64 = 0.8;
pub const SHADE_SOUTH: f64 = 0.62;

/// Màu fallback khi resolve texture thất bại (validator model3d sẽ bắt riêng).
pub const FALLBACK: (u8, u8, u8) = (232, 178, 58); // vàng totem #e8b23a

/// Camera isometric — chỉ thấy 3 mặt này.
pub const VISIBLE_FACES: [&str; 3] = ["south", "east", "up"];

fn shade_of(face: &str) -> f64 {
    match face {
        "up" => SHADE_UP,
        "east" => SHADE_EAST,
        _ => SHADE_SOUTH,
    }
}

/// `_hex_rgb` parity — parse fail → FALLBACK (không bao giờ None).
fn hex_rgb_or_fallback(color: &str) -> (u8, u8, u8) {
    crate::hex_rgb(color)
}

/// Face texture ref → RGB. None nếu không resolve được (dùng FALLBACK).
/// Parity `_resolve_color`.
fn resolve_color(value: Option<&Value>, textures: &serde_json::Map<String, Value>) -> Option<(u8, u8, u8)> {
    let v = value?.as_str()?.trim();
    if v.is_empty() {
        return None;
    }
    if v.starts_with('#') {
        return Some(hex_rgb_or_fallback(v));
    }
    match textures.get(v) {
        Some(t) if t.as_str().map(|s| s.trim().starts_with('#')).unwrap_or(false) => {
            Some(hex_rgb_or_fallback(t.as_str().unwrap().trim()))
        }
        Some(t) if t.as_str().map(|s| s.trim().starts_with("assets/")).unwrap_or(false) => None,
        _ => None,
    }
}

/// `float(v)` parity Python — Number hoặc numeric string (bool → 1.0/0.0).
fn f64_of(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::Bool(b) => Some(if *b { 1.0 } else { 0.0 }),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
}

type FaceGeom = (&'static str, [f64; 3], [f64; 3], [f64; 3]);

/// Cube → [(face, origin, e1, e2)] cho 3 mặt thấy được; [] nếu from/to hỏng
/// hoặc cube suy biến. Parity `_face_geometry`.
fn face_geometry(cube: &Value) -> Vec<FaceGeom> {
    let Some(from) = cube.get("from").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    let Some(to) = cube.get("to").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    if from.len() != 3 || to.len() != 3 {
        return Vec::new();
    }
    let mut f = [0.0f64; 3];
    let mut t = [0.0f64; 3];
    for i in 0..3 {
        let (Some(a), Some(b)) = (f64_of(&from[i]), f64_of(&to[i])) else {
            return Vec::new();
        };
        f[i] = a;
        t[i] = b;
    }
    let lo = [f[0].min(t[0]), f[1].min(t[1]), f[2].min(t[2])];
    let hi = [f[0].max(t[0]), f[1].max(t[1]), f[2].max(t[2])];
    let (dx, dy, dz) = (hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]);
    if dx <= 0.0 || dy <= 0.0 || dz <= 0.0 {
        return Vec::new();
    }
    let (x0, y0, z0) = (lo[0], lo[1], lo[2]);
    let (x1, y1, z1) = (hi[0], hi[1], hi[2]);
    vec![
        ("up", [x0, y1, z0], [dx, 0.0, 0.0], [0.0, 0.0, dz]),
        ("east", [x1, y0, z0], [0.0, 0.0, dz], [0.0, dy, 0.0]),
        ("south", [x0, y0, z1], [dx, 0.0, 0.0], [0.0, dy, 0.0]),
    ]
}

/// 3D → 2D isometric: sx = x - z, sy = (x + z) * 0.5 - y. Parity `_project`.
fn project(p: &[f64; 3]) -> (f64, f64) {
    (p[0] - p[2], (p[0] + p[2]) * 0.5 - p[1])
}

struct FaceDraw {
    p0: (f64, f64),
    a: (f64, f64),
    b: (f64, f64),
    d0: f64,
    da: f64,
    db: f64,
    color: (u8, u8, u8),
}

/// VoxelSpec → (size, rgba bytes) RGBA có nền trong suốt. Parity `render_rgba`.
pub fn render_rgba(spec: &Value, size: usize, pad: usize) -> (usize, Vec<u8>) {
    let size = size.clamp(16, 1024);
    let pad = pad as f64;
    let norm = model3d::normalize(Some(spec));
    let textures = norm
        .get("textures")
        .and_then(|t| t.as_object())
        .cloned()
        .unwrap_or_default();
    let mut px = vec![0u8; size * size * 4]; // transparent
    let mut zbuf = vec![f64::NEG_INFINITY; size * size]; // depth lớn hơn = gần camera

    let mut faces: Vec<FaceDraw> = Vec::new();
    if let Some(cubes) = norm.get("cubes").and_then(|c| c.as_array()) {
        for cube in cubes {
            for (fname, origin, e1, e2) in face_geometry(cube) {
                if !VISIBLE_FACES.contains(&fname) {
                    continue;
                }
                let p0 = project(&origin);
                let pa_o = [origin[0] + e1[0], origin[1] + e1[1], origin[2] + e1[2]];
                let pb_o = [origin[0] + e2[0], origin[1] + e2[1], origin[2] + e2[2]];
                let pa = project(&pa_o);
                let pb = project(&pb_o);
                let d0 = origin.iter().sum::<f64>();
                let da = e1.iter().sum::<f64>();
                let db = e2.iter().sum::<f64>();
                let tex_ref = cube
                    .get("faces")
                    .and_then(|f| f.get(fname))
                    .filter(|f| f.is_object())
                    .and_then(|f| f.get("texture"));
                let color =
                    resolve_color(tex_ref, &textures).unwrap_or(FALLBACK);
                let shade = shade_of(fname);
                faces.push(FaceDraw {
                    p0,
                    a: (pa.0 - p0.0, pa.1 - p0.1),
                    b: (pb.0 - p0.0, pb.1 - p0.1),
                    d0,
                    da,
                    db,
                    color: (
                        (color.0 as f64 * shade) as u8,
                        (color.1 as f64 * shade) as u8,
                        (color.2 as f64 * shade) as u8,
                    ),
                });
            }
        }
    }

    if faces.is_empty() {
        return (size, px);
    }

    // Bounds toàn cảnh → scale fit + center (deterministic)
    let mut xs: Vec<f64> = Vec::new();
    let mut ys: Vec<f64> = Vec::new();
    for f in &faces {
        for ex in [0.0, 1.0] {
            for ey in [0.0, 1.0] {
                xs.push(f.p0.0 + ex * f.a.0 + ey * f.b.0);
                ys.push(f.p0.1 + ex * f.a.1 + ey * f.b.1);
            }
        }
    }
    let (min_x, max_x) = (
        xs.iter().cloned().fold(f64::INFINITY, f64::min),
        xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    );
    let (min_y, max_y) = (
        ys.iter().cloned().fold(f64::INFINITY, f64::min),
        ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
    );
    let span = (max_x - min_x).max(max_y - min_y);
    let span = if span == 0.0 { 1.0 } else { span }; // parity `or 1.0`
    let scale = (size as f64 - 2.0 * pad) / span;
    let off_x = (size as f64 - (max_x - min_x) * scale) / 2.0 - min_x * scale;
    let off_y = (size as f64 - (max_y - min_y) * scale) / 2.0 - min_y * scale;

    // Raster từng face — z-buffer per pixel
    for f in &faces {
        let (ax, ay) = f.a;
        let (bx, by) = f.b;
        let (p0x, p0y) = f.p0;
        let corners = [
            (p0x, p0y),
            (p0x + ax, p0y + ay),
            (p0x + ax + bx, p0y + ay + by),
            (p0x + bx, p0y + by),
        ];
        let cx: Vec<f64> = corners.iter().map(|c| c.0 * scale + off_x).collect();
        let cy: Vec<f64> = corners.iter().map(|c| c.1 * scale + off_y).collect();
        let x_lo = (cx.iter().cloned().fold(f64::INFINITY, f64::min).floor() as i64).max(0);
        let x_hi = (cx.iter().cloned().fold(f64::NEG_INFINITY, f64::max).ceil() as i64)
            .min(size as i64 - 1);
        let y_lo = (cy.iter().cloned().fold(f64::INFINITY, f64::min).floor() as i64).max(0);
        let y_hi = (cy.iter().cloned().fold(f64::NEG_INFINITY, f64::max).ceil() as i64)
            .min(size as i64 - 1);
        if x_lo > x_hi || y_lo > y_hi {
            continue;
        }
        let det = ax * by - ay * bx;
        if det.abs() < 1e-12 {
            continue;
        }
        let (r, g, b) = f.color;
        let (d0, da, db) = (f.d0, f.da, f.db);
        for py in y_lo..=y_hi {
            let fy = py as f64 + 0.5;
            for pxx in x_lo..=x_hi {
                let fx = pxx as f64 + 0.5;
                // solve [a b] [u]   (fx - p0x')  — quirk parity: det unscaled
                //        [c d] [v] = (fy - p0y')
                let rx = fx - (p0x * scale + off_x);
                let ry = fy - (p0y * scale + off_y);
                let u = (rx * by - ry * bx) / det;
                let v = (ax * ry - ay * rx) / det;
                if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
                    continue;
                }
                let depth = d0 + u * da + v * db;
                let idx = (py as usize) * size + pxx as usize;
                if depth > zbuf[idx] {
                    // gần camera hơn (sum lớn hơn) → đè
                    zbuf[idx] = depth;
                    let i = idx * 4;
                    px[i] = r;
                    px[i + 1] = g;
                    px[i + 2] = b;
                    px[i + 3] = 255;
                }
            }
        }
    }
    (size, px)
}

/// VoxelSpec → PNG RGBA bytes (mục 9: PNG output, deterministic).
/// Parity `render_png(spec, size=256)`.
pub fn render_png(spec: &Value, size: usize) -> Vec<u8> {
    let (s, rgba) = render_rgba(spec, size, 12);
    antares_resources::encode_png(s as u32, s as u32, &rgba).expect("render size hợp lệ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn constants_parity() {
        assert_eq!(SHADE_UP, 1.0);
        assert_eq!(SHADE_EAST, 0.8);
        assert_eq!(SHADE_SOUTH, 0.62);
        assert_eq!(FALLBACK, (232, 178, 58));
        assert_eq!(VISIBLE_FACES, ["south", "east", "up"]);
    }

    #[test]
    fn project_isometric() {
        assert_eq!(project(&[0.0, 0.0, 0.0]), (0.0, 0.0));
        assert_eq!(project(&[4.0, 2.0, 2.0]), (2.0, 1.0));
        // y cao hơn → sy nhỏ hơn (lên màn hình)
        assert!(project(&[0.0, 5.0, 0.0]).1 < project(&[0.0, 0.0, 0.0]).1);
    }

    #[test]
    fn face_geometry_visible_and_degenerate() {
        let cube = json!({"from": [0, 0, 0], "to": [4, 4, 4]});
        let g = face_geometry(&cube);
        let names: Vec<&str> = g.iter().map(|(n, ..)| *n).collect();
        assert_eq!(names, ["up", "east", "south"]);
        // up face origin [x0, y1, z0]
        assert_eq!(g[0].1, [0.0, 4.0, 0.0]);
        assert_eq!(g[0].2, [4.0, 0.0, 0.0]);
        assert_eq!(g[0].3, [0.0, 0.0, 4.0]);
        // suy biến → rỗng
        assert!(face_geometry(&json!({"from": [1, 1, 1], "to": [1, 2, 2]})).is_empty());
        assert!(face_geometry(&json!({"from": [0, 0], "to": [1, 1, 1]})).is_empty());
        // numeric string parse như Python float()
        assert_eq!(face_geometry(&json!({"from": ["0", 0, 0], "to": [2, 2, 2]})).len(), 3);
    }

    #[test]
    fn render_deterministic_and_nonempty() {
        let spec = crate::totem::voxel_spec(None);
        let a = render_png(&spec, 256);
        let b = render_png(&spec, 256);
        assert_eq!(a, b, "cùng spec + size → cùng bytes (mục 9)");
        let (w, h) = antares_resources::decode_png_dims(&a).unwrap();
        assert_eq!((w, h), (256, 256));
        // có pixel vẽ (rgba từ render_rgba trực tiếp)
        let (_, rgba) = render_rgba(&spec, 256, 12);
        assert!(rgba.iter().skip(3).step_by(4).any(|&al| al == 255));
    }

    #[test]
    fn render_empty_or_invalid_spec_is_transparent() {
        for spec in [json!({"cubes": []}), json!("junk"), json!({"cubes": [{"nope": 1}]})] {
            let (size, rgba) = render_rgba(&spec, 64, 12);
            assert_eq!(size, 64);
            assert!(rgba.iter().all(|&b| b == 0), "trong suốt hoàn toàn");
        }
    }

    #[test]
    fn resolve_color_parity() {
        let mut textures = serde_json::Map::new();
        textures.insert("base".into(), json!("#e8b23a"));
        textures.insert("asset".into(), json!("assets/minecraft/t.png"));
        textures.insert("bad".into(), json!(42));
        // hex trực tiếp
        assert_eq!(resolve_color(Some(&json!("#ff0000")), &textures), Some((255, 0, 0)));
        // hex hỏng → FALLBACK (parity _hex_rgb)
        assert_eq!(resolve_color(Some(&json!("#zzz")), &textures), Some(FALLBACK));
        // key → màu của key
        assert_eq!(resolve_color(Some(&json!("base")), &textures), Some((232, 178, 58)));
        // asset path → None
        assert_eq!(resolve_color(Some(&json!("asset")), &textures), None);
        // key trỏ số → None
        assert_eq!(resolve_color(Some(&json!("bad")), &textures), None);
        // key không tồn tại → None
        assert_eq!(resolve_color(Some(&json!("nope")), &textures), None);
        // rỗng / không phải string → None
        assert_eq!(resolve_color(Some(&json!(42)), &textures), None);
        assert_eq!(resolve_color(Some(&json!("")), &textures), None);
        assert_eq!(resolve_color(None, &textures), None);
    }

    #[test]
    fn size_clamped() {
        let spec = crate::totem::voxel_spec(None);
        let (s, _) = render_rgba(&spec, 8, 12);
        assert_eq!(s, 16, "min 16");
        let (s, _) = render_rgba(&spec, 4096, 12);
        assert_eq!(s, 1024, "max 1024");
    }
}
