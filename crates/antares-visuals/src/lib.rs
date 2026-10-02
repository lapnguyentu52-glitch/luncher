//! antares-visuals — B15.1f, parity `services/visuals/` (mục 8 Crosshair
//! Studio, mục 9 Totem, mục 11 HUD, mục 52 FX, mục 8.4 atlas, mục 15 model3d).
//!
//! Phase A (đã làm — logic thuần, render PNG qua `antares_resources::png`):
//! - `crosshair` — presets 7 mục 8.1 + normalize clamp mục 76 + render 16×16
//! - `totem`     — presets 8 + texture 32×32 + `voxel_spec` Quick→Advanced mục 70
//! - `fx`        — hit overlay 128×128 (5 kinds) + particle atlas 16×(16×N)
//!                 + `.mcmeta` animation chuẩn MC
//! - `hud`       — 14 widget catalogue + validate/sanitize/merge_default mục 76
//! - `model3d`   — VoxelSpec validate (severity INFO/WARNING/ERROR/FATAL mục
//!                 15.1, finding shape mục 15.2, bounds [-16,32], cap 32/64/256)
//! - `renderer`  — static isometric z-buffer deterministic (mục 9: cùng spec +
//!                 size → cùng bytes), không GPU
//! - `draft`     — totem-3d draft store `<data>/totem-3d/draft.json` atomic +
//!                 corrupt → None recovery (parity totem_model_get/save)
//!
//! Phase sau (defer): export_pack/export_totem_pack/export_fx_pack — cần RS
//! project store + builder ZIP + install (cùng phase với resource.* 18 method).

pub mod crosshair;
pub mod draft;
pub mod fx;
pub mod hud;
pub mod model3d;
pub mod renderer;
pub mod totem;

use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum VisualError {
    #[error("spec invalid: {0}")]
    SpecInvalid(String),
    #[error("config invalid: {0}")]
    ConfigInvalid(String),
    #[error("draft corrupt: {0}")]
    DraftCorrupt(String),
    #[error("io: {0}")]
    Io(String),
}

impl VisualError {
    /// Code taxonomy §117 — parity CONFIG_INVALID / VALIDATION_FAILED / APP_INTERNAL.
    pub fn code(&self) -> &'static str {
        match self {
            VisualError::SpecInvalid(_) | VisualError::DraftCorrupt(_) => "VALIDATION_FAILED",
            VisualError::ConfigInvalid(_) => "CONFIG_INVALID",
            VisualError::Io(_) => "APP_INTERNAL",
        }
    }
}

/// Finding chuẩn mục 15.2 — parity dict `{severity, code, path, message, fixable}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelFinding {
    pub severity: &'static str,
    pub code: String,
    pub path: String,
    pub message: String,
    pub fixable: bool,
}

/// `_is_hex` parity — `#` + 3 hoặc 6 hex digit (crosshair/totem/model3d).
pub fn is_hex_color(c: &str) -> bool {
    let c = c.trim();
    let body = c.strip_prefix('#').unwrap_or("");
    if body.len() != 3 && body.len() != 6 {
        return false;
    }
    body.chars().all(|ch| ch.is_ascii_hexdigit())
}

/// `hex_rgb` parity — `#rgb` tách thành `#rrggbb`; parse fail bất kỳ byte →
/// fallback vàng totem `#e8b23a` (parity `renderer._hex_rgb` try/except —
/// màu đã qua normalize nên luôn hợp lệ khi tới đây).
pub fn hex_rgb(color: &str) -> (u8, u8, u8) {
    let c = color.strip_prefix('#').unwrap_or(color);
    let c = if c.len() == 3 {
        c.chars()
            .flat_map(|ch| [ch, ch])
            .collect::<String>()
    } else {
        c.to_string()
    };
    let byte = |s: &str, i: usize| -> Option<u8> {
        u8::from_str_radix(s.get(i..i + 2)?, 16).ok()
    };
    if c.len() < 6 {
        return (232, 178, 58);
    }
    match (byte(&c, 0), byte(&c, 2), byte(&c, 4)) {
        (Some(r), Some(g), Some(b)) => (r, g, b),
        _ => (232, 178, 58),
    }
}

/// Truthiness Python `bool(x)` cho field boolean — parity: None→false,
/// bool→bản thân, number→≠0, string→non-empty, array/object→non-empty.
/// (legacy dùng `bool(spec.get(key, default))` — không phải as_bool.)
pub fn truthy(v: Option<&Value>, default: bool) -> bool {
    match v {
        None => default,
        Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Some(Value::String(s)) => !s.is_empty(),
        Some(Value::Array(a)) => !a.is_empty(),
        Some(Value::Object(o)) => !o.is_empty(),
    }
}

/// base64 encode tối giản (zero-dep) — parity `base64.b64encode(...).decode("ascii")`.
pub fn encode_b64(data: &[u8]) -> String {
    const TBL: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TBL[(n >> 18) as usize & 63] as char);
        out.push(TBL[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TBL[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TBL[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// PNG bytes → `data:image/png;base64,...` (parity `render_*_b64`).
pub fn png_data_uri(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", encode_b64(png))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_helpers_parity() {
        assert!(is_hex_color("#ff4655"));
        assert!(is_hex_color("#F00"));
        assert!(is_hex_color(" #e8b23a "));
        assert!(!is_hex_color("ff4655"));
        assert!(!is_hex_color("#ff46"));
        assert!(!is_hex_color("#ff465zz"));
        // expand 3 → 6
        assert_eq!(hex_rgb("#f00"), (255, 0, 0));
        assert_eq!(hex_rgb("#ff4655"), (255, 70, 85));
        // parse fail → FALLBACK vàng totem
        assert_eq!(hex_rgb("zz"), (232, 178, 58));
        assert_eq!(hex_rgb("#zzz"), (232, 178, 58));
        assert_eq!(hex_rgb("#ff46zz"), (232, 178, 58));
    }

    #[test]
    fn error_codes_parity() {
        assert_eq!(VisualError::SpecInvalid("x".into()).code(), "VALIDATION_FAILED");
        assert_eq!(VisualError::ConfigInvalid("x".into()).code(), "CONFIG_INVALID");
        assert_eq!(VisualError::Io("x".into()).code(), "APP_INTERNAL");
    }

    #[test]
    fn base64_golden_vectors() {
        // RFC 4648 test vectors
        assert_eq!(encode_b64(b""), "");
        assert_eq!(encode_b64(b"f"), "Zg==");
        assert_eq!(encode_b64(b"fo"), "Zm8=");
        assert_eq!(encode_b64(b"foo"), "Zm9v");
        assert_eq!(encode_b64(b"foob"), "Zm9vYg==");
        assert_eq!(encode_b64(b"fooba"), "Zm9vYmE=");
        assert_eq!(encode_b64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn data_uri_shape() {
        let uri = png_data_uri(&[0x89, b'P', b'N', b'G']);
        assert!(uri.starts_with("data:image/png;base64,"));
        assert_eq!(uri.len(), 22 + 8);
    }
}
