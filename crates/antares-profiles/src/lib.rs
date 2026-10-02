//! antares-profiles — Batch 14, mục **profiles** (mục 40 Player Profiles, §107 diff,
//! §98 storage integration).
//!
//! - keys/coerce/diff/write_options — parity 1:1 `services/profiles/{keys,service}.py`
//! - `store` (phase 4) — ProfileStore CRUD qua `antares-storage` (state JSON atomic),
//!   parity `ProfileService` CRUD + sanitize/validate (plan/apply/revert cần runtime
//!   instances/accounts — nối ở tầng bridge)

pub mod store;

use std::collections::BTreeMap;

use serde::Serialize;

pub use store::{
    is_safe_name, sanitize_spec, validate_spec, JvmSpec, ProfilePatch, ProfileRecord,
    ProfileSpec, ProfileState, ProfileStore, ProfileStoreError, SectionId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyKind {
    Int,
    Float,
    Bool,
    Str,
}

pub const GAME_KEYS: &[(&str, KeyKind)] = &[
    ("gamma", KeyKind::Float),
    ("fov", KeyKind::Float),
    ("guiScale", KeyKind::Int),
    ("maxFps", KeyKind::Int),
    ("renderDistance", KeyKind::Int),
    ("simulationDistance", KeyKind::Int),
    ("brightness", KeyKind::Float),
    ("fullscreen", KeyKind::Bool),
    ("vsync", KeyKind::Bool),
    ("cloudStatus", KeyKind::Str),
    ("graphicsMode", KeyKind::Int),
    ("ao", KeyKind::Bool),
    ("entityShadows", KeyKind::Bool),
    ("screenEffectScale", KeyKind::Float),
    ("mipmapLevels", KeyKind::Int),
    ("biomeBlendRadius", KeyKind::Int),
    ("entityDistanceScaling", KeyKind::Float),
    ("particles", KeyKind::Int),
    ("sensitivity", KeyKind::Float),
    ("toggleSprint", KeyKind::Bool),
    ("toggleCrouch", KeyKind::Bool),
    ("invertYMouse", KeyKind::Bool),
    ("mouseRawInput", KeyKind::Bool),
    
    ("soundCategory_master", KeyKind::Float),
    ("soundCategory_music", KeyKind::Float),
    ("soundCategory_hostile", KeyKind::Float),
    ("soundCategory_players", KeyKind::Float),
    ("soundCategory_weather", KeyKind::Float),
];

pub const GAME_KEY_ORDER: &[&str] = &[
    "gamma",
    "fov",
    "guiScale",
    "maxFps",
    "renderDistance",
    "simulationDistance",
    "brightness",
    "fullscreen",
    "vsync",
    "cloudStatus",
    "graphicsMode",
    "ao",
    "entityShadows",
    "screenEffectScale",
    "mipmapLevels",
    "biomeBlendRadius",
    "entityDistanceScaling",
    "particles",
    "sensitivity",
    "toggleSprint",
    "toggleCrouch",
    "invertYMouse",
    "mouseRawInput",
    "soundCategory_master",
    "soundCategory_music",
    "soundCategory_hostile",
    "soundCategory_players",
    "soundCategory_weather",
];

pub fn coerce(key: &str, value: &serde_json::Value) -> String {
    let kind = GAME_KEYS
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, kind)| *kind)
        .unwrap_or(KeyKind::Str);
    match kind {
        KeyKind::Bool => match value {
            serde_json::Value::Bool(b) => bool_str(*b).into(),
            serde_json::Value::String(s) => {
                bool_str(matches!(s.trim().to_lowercase().as_str(), "true" | "1" | "yes" | "on")).into()
            }
            _ => bool_str(store::is_truthy(value)).into(),
        },
        KeyKind::Int => match value {
            serde_json::Value::Bool(b) => {
                if *b { "1".into() } else { "0".into() }
            }
            serde_json::Value::Number(n) => {
                let f = n.as_f64().unwrap_or(0.0);
                let rounded = python_round(f);
                if rounded == rounded.trunc() && rounded.abs() < 1e15 {
                    format!("{}", rounded as i64)
                } else {
                    format!("{rounded}")
                }
            }
            serde_json::Value::String(s) => match s.trim().parse::<f64>() {
                Ok(f) => {
                    let rounded = python_round(f);
                    format!("{}", rounded as i64)
                }
                Err(_) => s.clone(),
            },
            _ => String::new(),
        },
        KeyKind::Float => match value {
            // Parity keys.py: bool đưa vào key KHÔNG phải bool → "1"/"0" (không "1.0").
            serde_json::Value::Bool(b) => {
                if *b { "1".into() } else { "0".into() }
            }
            serde_json::Value::Number(n) => format_float(n.as_f64().unwrap_or(0.0)),
            serde_json::Value::String(s) => match s.trim().parse::<f64>() {
                Ok(f) => format_float(f),
                Err(_) => s.clone(),
            },
            _ => String::new(),
        },
        KeyKind::Str => match value {
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Bool(b) => {
                if *b { "1".into() } else { "0".into() }
            }
            other => other.to_string(),
        },
    }
}

fn bool_str(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}

/// Python `round()` = round-half-even (banker's). Rust `f64::round()` là half-away-from-zero —
/// sai parity. Implement round-half-even thủ công.
pub fn python_round(value: f64) -> f64 {
    let floor = value.floor();
    let diff = value - floor;
    if diff > 0.5 {
        floor + 1.0
    } else if diff < 0.5 {
        floor
    } else {
        // .5 đúng → về số chẵn
        if floor % 2.0 == 0.0 { floor } else { floor + 1.0 }
    }
}

fn format_float(v: f64) -> String {
    // Python str(float): 1.0 → "1.0", 0.5 → "0.5", 16.0 → "16.0"
    if v.is_finite() && v == v.trunc() && v.abs() < 1e15 {
        format!("{v:.1}")
    } else {
        format!("{v}")
    }
}

/// Key lạ bị chặn (parity validate_spec keys.py) — chỉ GAME_KEYS được ghi options.txt.
pub fn is_game_key(key: &str) -> bool {
    GAME_KEYS.iter().any(|(k, _)| *k == key)
}

// ---------------------------------------------------------------------------
// options.txt parser/serializer — key:value
// ---------------------------------------------------------------------------

/// Parse options.txt thô thành map (mọi key, không coerce) — dùng cho diff full-file.
/// Dùng `parse_game_options` khi cần đúng parity legacy (whitelist + coerce).
pub fn parse_options(content: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in content.lines() {
        if let Some((key, value)) = line.split_once(':') {
            map.insert(key.to_string(), value.to_string());
        }
    }
    map
}

pub fn serialize_options(map: &BTreeMap<String, String>) -> String {
    let mut out = String::new();
    for (key, value) in map {
        out.push_str(key);
        out.push(':');
        out.push_str(value);
        out.push('\n');
    }
    out
}

/// Parity `ProfileService._parse_options` (services/profiles/service.py):
/// chỉ giữ GAME_KEYS, value coerce theo whitelist; bỏ dòng rỗng/# và key lạ.
pub fn parse_game_options(content: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once(':') {
            if is_game_key(key) {
                map.insert(
                    key.to_string(),
                    coerce(key, &serde_json::Value::String(value.to_string())),
                );
            }
        }
    }
    map
}

/// Phase 2 — parity `ProfileService._write_options` (services/profiles/service.py):
/// ghi merged — giữ dòng cũ theo thứ tự file, cập nhật key trong patch (coerce +
/// whitelist), dedupe (dòng đầu thắng), append key mới cuối file.
///
/// Trả về nội dung file mới (`key:value\n`...) — caller ghi qua antares-storage
/// (atomic write). Trống `values` → nội dung giữ nguyên + newline cuối chuẩn hoá.
pub fn write_options_merged(content: &str, values: &BTreeMap<String, serde_json::Value>) -> String {
    // patch = {k: coerce(v)} chỉ với key trong whitelist (parity legacy).
    let patch: BTreeMap<&str, String> = values
        .iter()
        .filter(|(k, _)| is_game_key(k))
        .map(|(k, v)| (k.as_str(), coerce(k, v)))
        .collect();

    let mut out: Vec<String> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    for line in content.lines() {
        let key = line.split(':').next().unwrap_or("").trim().to_string();
        if patch.contains_key(key.as_str()) {
            if seen.contains(&key) {
                continue; // dedupe: dòng đầu thắng
            }
            out.push(format!("{}:{}", key, patch[key.as_str()]));
            seen.insert(key);
        } else {
            out.push(line.to_string());
        }
    }
    for (key, val) in &patch {
        if !seen.contains(*key) {
            out.push(format!("{}:{}", key, val));
        }
    }
    // Parity legacy: luôn ghi join + "\n" (không rỗng đặc biệt).
    let mut result = out.join("\n");
    result.push('\n');
    result
}

// ---------------------------------------------------------------------------
// §107 — Profile Diff Engine
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DiffOp {
    Add,
    Remove,
    Modify,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionDiff {
    pub op: DiffOp,
    pub key: String,
    pub current: Option<String>,
    pub desired: Option<String>,
}

/// Diff `current` vs `desired` — output `+ / - / ~` cho UI Preview changes (§107).
pub fn diff_options(
    current: &BTreeMap<String, String>,
    desired: &BTreeMap<String, String>,
) -> Vec<OptionDiff> {
    let mut diffs = Vec::new();
    for key in GAME_KEY_ORDER {
        let cur = current.get(*key);
        let des = desired.get(*key);
        match (cur, des) {
            (None, Some(d)) => diffs.push(OptionDiff {
                op: DiffOp::Add,
                key: (*key).to_string(),
                current: None,
                desired: Some(d.clone()),
            }),
            (Some(c), None) => diffs.push(OptionDiff {
                op: DiffOp::Remove,
                key: (*key).to_string(),
                current: Some(c.clone()),
                desired: None,
            }),
            (Some(c), Some(d)) if c != d => diffs.push(OptionDiff {
                op: DiffOp::Modify,
                key: (*key).to_string(),
                current: Some(c.clone()),
                desired: Some(d.clone()),
            }),
            _ => {}
        }
    }
    diffs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_keys_count_parity() {
        // keys.py có 28 key (18 video + 5 controls + 5 audio); GAME_KEY_ORDER khớp đúng.
        assert_eq!(GAME_KEYS.len(), 28);
        assert_eq!(GAME_KEY_ORDER.len(), 28);
        for (key, _) in GAME_KEYS {
            assert!(GAME_KEY_ORDER.contains(key), "{key} missing in order");
        }
    }

    #[test]
    fn coerce_bool_keys() {
        assert_eq!(coerce("fullscreen", &serde_json::json!(true)), "true");
        assert_eq!(coerce("fullscreen", &serde_json::json!(false)), "false");
        // bool đưa vào key không phải bool → 1/0 (parity keys.py)
        assert_eq!(coerce("gamma", &serde_json::json!(true)), "1");
        assert_eq!(coerce("gamma", &serde_json::json!(false)), "0");
        assert_eq!(coerce("guiScale", &serde_json::json!(true)), "1");
        assert_eq!(coerce("cloudStatus", &serde_json::json!(true)), "1");
    }

    #[test]
    fn coerce_bool_string_variants() {
        assert_eq!(coerce("vsync", &serde_json::json!("true")), "true");
        assert_eq!(coerce("vsync", &serde_json::json!("1")), "true");
        assert_eq!(coerce("vsync", &serde_json::json!("YES")), "true");
        assert_eq!(coerce("vsync", &serde_json::json!("on")), "true");
        assert_eq!(coerce("vsync", &serde_json::json!("nope")), "false");
    }

    #[test]
    fn coerce_int_rounds_half_even_like_python() {
        // Python round(0.5)=0, round(1.5)=2, round(2.5)=2 (banker's) — đã verify.
        assert_eq!(coerce("renderDistance", &serde_json::json!(10.4)), "10");
        assert_eq!(coerce("renderDistance", &serde_json::json!(10.6)), "11");
        assert_eq!(coerce("renderDistance", &serde_json::json!(0.5)), "0");
        assert_eq!(coerce("renderDistance", &serde_json::json!(1.5)), "2");
        assert_eq!(coerce("renderDistance", &serde_json::json!(2.5)), "2");
        assert_eq!(coerce("renderDistance", &serde_json::json!(-0.5)), "0");
        assert_eq!(coerce("guiScale", &serde_json::json!("12")), "12");
    }

    #[test]
    fn coerce_float_format() {
        assert_eq!(coerce("gamma", &serde_json::json!(1)), "1.0");
        assert_eq!(coerce("gamma", &serde_json::json!(0.5)), "0.5");
        assert_eq!(coerce("fov", &serde_json::json!("0.75")), "0.75");
    }

    #[test]
    fn unknown_key_untouched() {
        // key lạ → kind str, giữ nguyên (và is_game_key chặn ghi).
        assert!(!is_game_key("hackyKey"));
        assert_eq!(coerce("hackyKey", &serde_json::json!(123)), "123");
    }

    #[test]
    fn options_roundtrip() {
        let content = "version:4225\nrenderDistance:12\nsoundCategory_master:0.5\n";
        let map = parse_options(content);
        assert_eq!(map.get("renderDistance").map(String::as_str), Some("12"));
        let out = serialize_options(&map);
        let reparsed = parse_options(&out);
        assert_eq!(map, reparsed);
    }

    #[test]
    fn parse_game_options_whitelist_parity() {
        // Parity legacy: chỉ GAME_KEYS giữ lại, value coerce, dòng #/lạ bị bỏ.
        let content = concat!(
            "version:4225\n",
            "gamma:1\n",
            "renderDistance:12\n",
            "# comment\n",
            "soundCategory_master:0.5\n",
            "unknownKey:x\n",
        );
        let map = parse_game_options(content);
        assert_eq!(map.get("gamma").map(String::as_str), Some("1.0"));
        assert_eq!(map.get("renderDistance").map(String::as_str), Some("12"));
        assert_eq!(map.get("soundCategory_master").map(String::as_str), Some("0.5"));
        assert!(!map.contains_key("version"));
        assert!(!map.contains_key("unknownKey"));
    }

    #[test]
    fn write_options_merged_parity() {
        // Cập nhật key có sẵn: giữ vị trí dòng, đúng giá trị coerce.
        let content = "version:4225\ngamma:1.0\nrenderDistance:12\nsoundCategory_master:0.8\n";
        let mut values = BTreeMap::new();
        values.insert("renderDistance".to_string(), serde_json::json!(16));
        values.insert("gamma".to_string(), serde_json::json!(0.5));
        let out = write_options_merged(content, &values);
        assert_eq!(
            out,
            "version:4225\ngamma:0.5\nrenderDistance:16\nsoundCategory_master:0.8\n"
        );

        // Key lạ trong patch bị bỏ (whitelist).
        let mut values2 = BTreeMap::new();
        values2.insert("hackyKey".to_string(), serde_json::json!("x"));
        assert_eq!(write_options_merged(content, &values2), content);

        // Append key mới cuối file.
        let mut values3 = BTreeMap::new();
        values3.insert("vsync".to_string(), serde_json::json!(true));
        let out3 = write_options_merged(content, &values3);
        assert!(out3.ends_with("\nvsync:true\n"));
    }

    #[test]
    fn write_options_merges_dedupe_parity() {
        // Dòng trùng key: dòng đầu thắng (legacy `seen` logic).
        let content = "gamma:1.0\ngamma:0.5\n";
        let mut values = BTreeMap::new();
        values.insert("gamma".to_string(), serde_json::json!(0.25));
        let out = write_options_merged(content, &values);
        assert_eq!(out, "gamma:0.25\n");
    }

    #[test]
    fn write_options_empty_patch_preserves_content() {
        let content = "version:4225\ngamma:1.0\n";
        let values = BTreeMap::new();
        assert_eq!(write_options_merged(content, &values), content);
        // File chỉ whitespace → giữ dòng, thêm newline cuối.
        assert_eq!(write_options_merged("\n", &values), "\n");
    }

    #[test]
    fn diff_add_remove_modify() {
        let mut current = parse_options("gamma:1.0\nrenderDistance:12\noldMod:on\n");
        let mut desired = parse_options("gamma:0.5\nrenderDistance:16\nnewMod:on\n");

        // BTreeMap sort alphabetically — oldMod/newMod không nằm trong GAME_KEY_ORDER nên
        // bị bỏ qua; chỉ GAME_KEYS diff (§107 diff trên whitelist).
        desired.insert("newMod".to_string(), "on".to_string());
        current.insert("oldMod".to_string(), "on".to_string());

        let diffs = diff_options(&current, &desired);
        let keys: Vec<&str> = diffs.iter().map(|d| d.key.as_str()).collect();
        assert_eq!(keys, vec!["gamma", "renderDistance"]);
        assert_eq!(diffs[0].op, DiffOp::Modify);
        assert_eq!(diffs[0].current.as_deref(), Some("1.0"));
        assert_eq!(diffs[0].desired.as_deref(), Some("0.5"));
        assert_eq!(diffs[1].op, DiffOp::Modify);
        assert_eq!(diffs[1].current.as_deref(), Some("12"));
        assert_eq!(diffs[1].desired.as_deref(), Some("16"));
    }

    #[test]
    fn diff_add_and_remove() {
        let current = parse_options("fullscreen:false\n");
        let desired = parse_options("fullscreen:true\nvsync:true\n");
        let diffs = diff_options(&current, &desired);
        // fullscreen modify + vsync add
        assert_eq!(diffs.len(), 2);
        assert!(diffs
            .iter()
            .any(|d| d.op == DiffOp::Add && d.key == "vsync" && d.desired.as_deref() == Some("true")));
        let current2 = parse_options("fullscreen:true\nvsync:true\n");
        let desired2 = parse_options("fullscreen:true\n");
        let diffs2 = diff_options(&current2, &desired2);
        assert!(diffs2.iter().any(|d| d.op == DiffOp::Remove && d.key == "vsync"));
    }
}
