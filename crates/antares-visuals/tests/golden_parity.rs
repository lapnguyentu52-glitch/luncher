//! A/B parity goldens — visual renderers (evidence #1 trong PARITY.md).
//!
//! So `antares-visuals` render với fixtures sinh từ Python legacy
//! (`tests/parity/gen_visuals_golden.py`) ở **tầng RGBA pixel** — PNG container
//! khác byte là chấp nhận được (zlib.compress vs stored-blocks), geometry phải
//! khớp 100%. Golden file: `tests/parity/golden/visuals/<case>.json`
//! `{w, h, rgba, spec, size?, kind}`.

use serde_json::Value;

/// Golden fixture — payload format giống Python `rgba_of` manifest.
fn golden(case: &str) -> Value {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../tests/parity/golden/visuals/"
    )
    .to_string()
        + case
        + ".json";
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("thiếu golden {path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("golden {case} hỏng: {e}"))
}

/// Render spec qua crate (path tuỳ kind), decode PNG → (w, h, rgba hex).
fn render_rgba_hex(kind: &str, spec: &Value, size: u32) -> (u32, u32, String) {
    let png = match kind {
        "crosshair" => antares_visuals::crosshair::render_png(spec, size as usize),
        "totem" => antares_visuals::totem::render_png(spec),
        "hit" => antares_visuals::fx::render_hit_png(spec),
        "particle" => antares_visuals::fx::render_particle_png(spec),
        other => panic!("kind không hỗ trợ: {other}"),
    };
    let (w, h) =
        antares_resources::decode_png_dims(&png).expect("PNG crate sinh ra phải parse được");
    (w, h, decode_idat_rgba(&png, w as usize, h as usize))
}

/// Decode IDAT (deflate-stored của encode_png nội bộ) → raw rgba hex.
fn decode_idat_rgba(png: &[u8], w: usize, h: usize) -> String {
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
    let stride = w * 4 + 1;
    let mut hex = String::with_capacity(w * h * 8);
    for y in 0..h {
        assert_eq!(raw[y * stride], 0, "filter byte phải 0");
        for b in &raw[y * stride + 1..(y + 1) * stride] {
            hex.push_str(&format!("{b:02x}"));
        }
    }
    hex
}

/// Inflate cho deflate-stored blocks (zlib header 2 byte + adler32 cuối).
fn inflate_stored(data: &[u8]) -> Vec<u8> {
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

/// So khớp pixel: (w, h, rgba) golden == render crate. Fail → diff chi tiết.
fn assert_parity(case: &str) {
    let g = golden(case);
    let kind = g["kind"].as_str().expect("golden.kind");
    // size render nằm ở toplevel golden (mặc định 16) — KHÔNG nằm trong spec
    let size = g.get("size").and_then(|s| s.as_u64()).unwrap_or(16) as u32;
    let empty = serde_json::json!({});
    let spec = g.get("spec").unwrap_or(&empty);

    let expected = (
        g["w"].as_u64().unwrap() as u32,
        g["h"].as_u64().unwrap() as u32,
        g["rgba"].as_str().unwrap(),
    );
    let actual = render_rgba_hex(kind, spec, size);
    assert_eq!(actual.0, expected.0, "{case}: width mismatch");
    assert_eq!(actual.1, expected.1, "{case}: height mismatch");
    if actual.2 != expected.2 {
        // diff đầu tiên để debug nhanh
        let eb = expected.2.as_bytes();
        let ab = actual.2.as_bytes();
        let diff = eb.iter().zip(ab).position(|(a, b)| a != b).unwrap_or(0);
        let px = diff / 8;
        let w = expected.0 as usize;
        panic!(
            "{case}: RGBA mismatch tại hex offset {diff} (pixel {px}, x={} y={})\nexpected: {}...\nactual:   {}...",
            px % w,
            px / w,
            &expected.2[diff.saturating_sub(32)..(diff + 32).min(expected.2.len())],
            &actual.2[diff.saturating_sub(32)..(diff + 32).min(actual.2.len())],
        );
    }
}

macro_rules! parity_cases {
    ($($name:ident => $case:expr;)*) => {
        $(#[test]
        fn $name() {
            assert_parity($case);
        })*
    };
}

parity_cases! {
    crosshair_default => "crosshair_default";
    crosshair_minimal => "crosshair_minimal";
    crosshair_pvp => "crosshair_pvp";
    crosshair_clean_circle => "crosshair_clean_circle";
    crosshair_dot_shape => "crosshair_dot_shape";
    crosshair_thin_odd_thickness => "crosshair_thin_odd_thickness";
    crosshair_clamped => "crosshair_clamped";
    crosshair_size32 => "crosshair_size32";
    totem_default => "totem_default";
    totem_glow_wing => "totem_glow_wing";
    totem_no_wing => "totem_no_wing";
    totem_dark => "totem_dark";
    totem_clamped_colors => "totem_clamped_colors";
    hit_flash => "hit_flash";
    hit_vignette => "hit_vignette";
    hit_arrow => "hit_arrow";
    hit_cross => "hit_cross";
    hit_none => "hit_none";
    hit_clamped => "hit_clamped";
    particle_orb4 => "particle_orb4";
    particle_spark3 => "particle_spark3";
    particle_star2 => "particle_star2";
    particle_ring1 => "particle_ring1";
    particle_smoke8 => "particle_smoke8";
    particle_clamped => "particle_clamped";
}

#[test]
fn manifest_covers_all_25_cases() {
    let m = golden("manifest");
    let cases = m["cases"].as_object().expect("manifest.cases object");
    assert_eq!(cases.len(), 25, "manifest phải đủ 25 case");
}
