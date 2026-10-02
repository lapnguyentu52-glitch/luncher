//! Pack validator — parity 1:1 `services/resources/validator.py` (mục 10.5 + 76, 77).
//!
//! Checks: `invalid_json / invalid_metadata / missing_texture / duplicate_file /
//! invalid_model_reference / wrong_path / unsupported_asset` + archive structure
//! (pack.mcmeta tồn tại). Mỗi finding `{code, severity, path, detail}` (camelCase
//! qua serde khi client serialize). **FAIL chỉ khi có ERROR** (mục 42) — WARNING
//! không chặn build; heuristic mơ hồ không được coi là bằng chứng tuyệt đối.

use std::collections::HashMap;
use std::path::Path;

use serde::Serialize;

use crate::png::PNG_SIG;

/// Giới hạn decode ảnh PNG bằng header parse thủ công (mục 77: max size).
pub const MAX_PNG_DIM: u32 = 4096;
pub const MAX_FILE_BYTES: usize = 8 * 1024 * 1024;

/// Đường dẫn hợp lệ trong pack (prefix whitelist — chống wrong_path).
pub const VALID_PREFIXES: [&str; 2] = ["pack.png", "assets/"];

/// Extensions texture/model/sound hợp lệ.
pub const OK_EXTS: [&str; 8] = [
    ".png", ".mcmeta", ".json", ".ogg", ".fsb", ".txt", ".lang", ".cfg",
];

/// Models tham chiếu texture qua `"textures": {"layer0": "item/x"}` — check nhẹ.
const TEXTURABLE: [&str; 3] = ["models/item/", "models/block/", "models/gui/"];

/// Top-level dirs hợp lệ dưới assets/minecraft/ (mục 10.5 Wrong path).
pub const KNOWN_MC_DIRS: [&str; 9] = [
    "textures", "models", "sounds", "lang", "font", "shaders", "texts", "atlases", "items",
];

/// Extension hợp lệ theo từng loại thư mục.
const DIR_EXT_RULES: [(&str, &[&str]); 5] = [
    ("textures/", &[".mcmeta", ".png"]), // .png.mcmeta đi kèm texture animated (sort như legacy message)
    ("models/", &[".json"]),
    ("sounds/", &[".ogg"]),
    ("lang/", &[".json", ".lang"]),
    ("font/", &[".json"]),
];

/// Một finding — parity dict legacy `{code, severity, path, detail}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub code: &'static str,
    pub severity: &'static str,
    pub path: String,
    pub detail: String,
}

impl Finding {
    fn new(
        code: &'static str,
        severity: &'static str,
        path: impl Into<String>,
        detail: impl Into<String>,
    ) -> Self {
        Self {
            code,
            severity,
            path: path.into(),
            detail: detail.into(),
        }
    }

    /// Ít nhất một ERROR trong findings → validation FAIL (mục 42).
    pub fn has_errors(findings: &[Finding]) -> bool {
        findings.iter().any(|f| f.severity == "ERROR")
    }
}

/// Validate thư mục generated/ của project — parity `validate_project_dir`.
pub fn validate_dir(generated: &Path) -> Result<Vec<Finding>, crate::ResourceError> {
    let mut findings: Vec<Finding> = Vec::new();
    if !generated.is_dir() {
        return Ok(vec![Finding::new(
            "archive_structure",
            "ERROR",
            generated.to_string_lossy().to_string(),
            "generated/ missing",
        )]);
    }

    // pack.mcmeta: bắt buộc; pack_format phải int hoặc [major, minor] (mục 6.3)
    let mcmeta = generated.join("pack.mcmeta");
    if !mcmeta.is_file() {
        findings.push(Finding::new(
            "invalid_metadata",
            "ERROR",
            "pack.mcmeta",
            "missing pack.mcmeta",
        ));
    } else {
        if let Err(detail) = check_pack_meta(&mcmeta) {
            findings.push(Finding::new("invalid_json", "ERROR", "pack.mcmeta", detail));
        }
    }

    // Duyệt file sort theo rel path (rglob không đảm bảo thứ tự — legacy sort).
    let mut files: Vec<(String, std::path::PathBuf)> = Vec::new();
    collect_files(generated, generated, &mut files);
    files.sort_by(|a, b| a.0.cmp(&b.0));

    for (rel, abs) in &files {
        let rel_l = rel.to_lowercase();
        let ext = abs
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| format!(".{}", e.to_lowercase()))
            .unwrap_or_default();
        let size = std::fs::metadata(abs).map(|m| m.len()).unwrap_or(0);

        // wrong_path: ngoài whitelist prefix (pack.mcmeta miễn)
        if rel != "pack.mcmeta" && !valid_prefix(&rel_l) {
            findings.push(Finding::new(
                "wrong_path",
                "ERROR",
                rel,
                "outside assets/ (không đúng cấu trúc pack)",
            ));
        }
        // unsupported ext → WARNING (không chặn build; ext rỗng cũng WARNING — parity)
        if !OK_EXTS.contains(&ext.as_str()) {
            findings.push(Finding::new(
                "unsupported_asset",
                "WARNING",
                rel,
                format!("extension {ext}"),
            ));
        }
        // oversized > 8MB → ERROR
        if size > MAX_FILE_BYTES as u64 {
            findings.push(Finding::new(
                "oversized",
                "ERROR",
                rel,
                format!("{size} bytes > {MAX_FILE_BYTES}"),
            ));
        }
        // PNG header parse — corrupt/quá lớn (mục 77); read fail → detail như OSError legacy
        if ext == ".png" {
            match std::fs::read(abs) {
                Ok(data) => {
                    if let Some(err) = check_png(data) {
                        findings.push(Finding::new("missing_texture", "ERROR", rel, err));
                    }
                }
                Err(e) => findings.push(Finding::new("missing_texture", "ERROR", rel, e.to_string())),
            }
        }
        // JSON parse (pack.mcmeta đã check riêng)
        if ext == ".json" && rel != "pack.mcmeta" {
            if let Err(detail) = std::fs::read_to_string(abs)
                .map_err(|e| e.to_string())
                .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).map(|_| ()).map_err(|e| e.to_string()))
            {
                findings.push(Finding::new("invalid_json", "ERROR", rel, detail));
            }
        }
        // model refs
        if TEXTURABLE.contains(&rel_l.as_str()) || rel_l.contains("/models/") {
            check_model_refs(generated, abs, rel, &mut findings);
        }
        // cấu trúc assets/minecraft/<loại>/ + ext rules
        check_structure(rel, &ext, &mut findings);
    }

    // duplicate: cùng nội dung ở 2 path (mục 10.5 Duplicate file) — sha1
    let mut hash_map: HashMap<String, String> = HashMap::new();
    for (rel, abs) in &files {
        let size = std::fs::metadata(abs).map(|m| m.len()).unwrap_or(u64::MAX);
        if size >= MAX_FILE_BYTES as u64 {
            continue;
        }
        let Ok(data) = std::fs::read(abs) else {
            continue;
        };
        let h = antares_downloads::sha1_hex(&data);
        match hash_map.get(&h) {
            Some(first) => {
                if !rel.ends_with("pack.mcmeta") {
                    findings.push(Finding::new(
                        "duplicate_file",
                        "WARNING",
                        rel,
                        format!("duplicate of {first}"),
                    ));
                } else {
                    // quirk legacy: mcmeta trùng → ghi đè first-seen (file sau sẽ
                    // báo duplicate of mcmeta)
                    hash_map.insert(h, rel.clone());
                }
            }
            None => {
                hash_map.insert(h, rel.clone());
            }
        }
    }

    Ok(findings)
}

/// Validate ZIP build output qua danh sách entry names — parity `validate_zip`.
/// Caller mở zip (ví dụ `antares_mods::zip_entries`) và truyền names vào.
pub fn validate_zip_names(names: &[&str]) -> Vec<Finding> {
    let mut findings: Vec<Finding> = Vec::new();
    if !names.contains(&"pack.mcmeta") {
        findings.push(Finding::new(
            "invalid_metadata",
            "ERROR",
            "pack.mcmeta",
            "missing in archive",
        ));
    }
    for n in names {
        if n.ends_with('/') {
            continue;
        }
        // Root chỉ cho pack.mcmeta + pack.png; còn lại phải trong assets/
        if *n != "pack.mcmeta" && *n != "pack.png" && !n.to_lowercase().starts_with("assets/") {
            findings.push(Finding::new("wrong_path", "ERROR", *n, "outside assets/"));
        }
    }
    findings
}

// ---------------------------------------------------------------------------

/// pack.mcmeta phải parse được JSON và pack_format là int hoặc [major, minor].
fn check_pack_meta(mcmeta: &Path) -> Result<(), String> {
    let text = std::fs::read_to_string(mcmeta).map_err(|e| e.to_string())?;
    let meta: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let pf = meta.get("pack").and_then(|p| p.get("pack_format"));
    let ok = match pf {
        Some(serde_json::Value::Number(n)) => n.is_i64() || n.is_u64(),
        Some(serde_json::Value::Array(items)) => {
            items.len() == 2 && items.iter().all(|x| x.is_i64() || x.is_u64())
        }
        _ => false,
    };
    if ok {
        Ok(())
    } else {
        Err("pack_format must be int or [major, minor]".into())
    }
}

fn valid_prefix(rel_l: &str) -> bool {
    VALID_PREFIXES.iter().any(|p| rel_l.starts_with(p))
}

/// DFS đệ quy tất cả file — tương đương `rglob("*")` (dirs không vào kết quả).
fn collect_files(root: &Path, dir: &Path, out: &mut Vec<(String, std::path::PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let p = entry.path();
        if p.is_dir() {
            collect_files(root, &p, out);
        } else if p.is_file() {
            let rel = p
                .strip_prefix(root)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            out.push((rel, p));
        }
    }
}

/// Cấu trúc pack: `assets/minecraft/<loại>/...` — sai loại/sai ext = wrong_path.
fn check_structure(rel: &str, ext: &str, findings: &mut Vec<Finding>) {
    let prefix = "assets/minecraft/";
    if !rel.starts_with(prefix) || rel == "assets/minecraft" {
        return;
    }
    let rest = &rel[prefix.len()..];
    let parts: Vec<&str> = rest.split('/').collect();
    if parts.len() == 1 {
        // File nằm trực tiếp dưới assets/minecraft/ — phải là thư mục loại
        findings.push(Finding::new(
            "wrong_path",
            "ERROR",
            rel,
            "file trực tiếp dưới assets/minecraft/ (cần thư mục loại)",
        ));
        return;
    }
    let top = parts[0];
    if !KNOWN_MC_DIRS.contains(&top) {
        findings.push(Finding::new(
            "wrong_path",
            "ERROR",
            rel,
            format!("thư mục '{top}' không thuộc assets/minecraft/ chuẩn"),
        ));
        return;
    }
    for (dir_prefix, allowed) in DIR_EXT_RULES.iter() {
        if rest.starts_with(dir_prefix) && !allowed.contains(&ext) {
            findings.push(Finding::new(
                "wrong_path",
                "ERROR",
                rel,
                format!("{dir_prefix}* chỉ nhận {:?}, thấy '{ext}'", allowed),
            ));
            return;
        }
    }
}

/// `_check_png` parity — đọc header PNG thủ công, không decode full (mục 77).
/// Lưu ý: legacy đọc dims thẳng ở 16..24 mà không yêu cầu IHDR — giữ nguyên.
fn check_png(data: Vec<u8>) -> Option<String> {
    if data.len() < 33 || !data.starts_with(&PNG_SIG) {
        return Some("not a valid PNG".into());
    }
    let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
    let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
    if w > MAX_PNG_DIM || h > MAX_PNG_DIM {
        return Some(format!("too large: {w}x{h}"));
    }
    None
}

/// Model JSON tham chiếu texture không tồn tại → invalid_model_reference.
///
/// Ref namespaced `"ns:path"` → `assets/<ns>/textures/<path>.png`;
/// ref rút gọn `"item/x"` → `assets/minecraft/textures/item/x.png`.
fn check_model_refs(root: &Path, model_file: &Path, rel: &str, findings: &mut Vec<Finding>) {
    let Ok(text) = std::fs::read_to_string(model_file) else {
        return; // invalid_json đã bắt ở check riêng
    };
    let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) else {
        return;
    };
    let Some(textures) = data.get("textures").and_then(|t| t.as_object()) else {
        return;
    };
    for (key, value) in textures {
        let Some(r) = value.as_str() else { continue };
        if r.starts_with('#') {
            continue;
        }
        let tex_rel = match r.split_once(':') {
            // parity regex ^[a-z0-9_.-]+:(.+)$ — namespace chỉ [a-z0-9_.-]
            Some((ns, path))
                if !ns.is_empty()
                    && ns
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '.' | '_' | '-'))
                    && !path.is_empty() =>
            {
                format!("assets/{ns}/textures/{path}.png")
            }
            _ => format!("assets/minecraft/textures/{r}.png"),
        };
        if !root.join(&tex_rel).is_file() {
            findings.push(Finding::new(
                "invalid_model_reference",
                "ERROR",
                rel,
                format!("textures.{key} -> {tex_rel} missing"),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::png::encode_png;

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("antares-validator-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn png(w: u32, h: u32) -> Vec<u8> {
        // 4 byte RGBA (1,2,3,4) mỗi pixel — parity `vec([1,2,3,4]) * (w*h)` Python.
        let rgba: Vec<u8> = (0..w * h).flat_map(|_| [1u8, 2u8, 3u8, 4u8]).collect();
        encode_png(w, h, &rgba).unwrap()
    }

    fn write(gen: &Path, rel: &str, bytes: &[u8]) {
        let p = gen.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, bytes).unwrap();
    }

    fn mcmeta() -> &'static str {
        r#"{"pack": {"pack_format": 34, "description": "t"}}"#
    }

    fn codes(findings: &[Finding]) -> Vec<&'static str> {
        findings.iter().map(|f| f.code).collect()
    }

    #[test]
    fn constants_parity() {
        assert_eq!(MAX_PNG_DIM, 4096);
        assert_eq!(MAX_FILE_BYTES, 8 * 1024 * 1024);
        assert_eq!(VALID_PREFIXES, ["pack.png", "assets/"]);
        assert_eq!(OK_EXTS.len(), 8);
        assert_eq!(KNOWN_MC_DIRS.len(), 9);
    }

    #[test]
    fn missing_generated_dir_is_archive_structure_error() {
        let root = temp_root("missing");
        let findings = validate_dir(&root.join("nope")).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].code, "archive_structure");
        assert_eq!(findings[0].severity, "ERROR");
        assert!(Finding::has_errors(&findings));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_pack_mcmeta_is_invalid_metadata() {
        let root = temp_root("mcmeta");
        let gen = root.join("generated");
        std::fs::create_dir_all(&gen).unwrap();
        let findings = validate_dir(&gen).unwrap();
        assert_eq!(codes(&findings), vec!["invalid_metadata"]);
        assert!(Finding::has_errors(&findings));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn pack_format_accepts_int_or_pair_rejects_other() {
        let root = temp_root("packformat");
        let gen = root.join("generated");
        write(&root, "generated/pack.mcmeta", mcmeta().as_bytes());
        assert!(!Finding::has_errors(&validate_dir(&gen).unwrap()));

        write(&root, "generated/pack.mcmeta", br#"{"pack": {"pack_format": [69, 0]}}"#);
        assert!(validate_dir(&gen).unwrap().is_empty());

        write(&root, "generated/pack.mcmeta", br#"{"pack": {"pack_format": "34"}}"#);
        let findings = validate_dir(&gen).unwrap();
        assert_eq!(codes(&findings), vec!["invalid_json"]);
        assert!(findings[0].detail.contains("int or [major, minor]"));

        write(&root, "generated/pack.mcmeta", br#"{"pack": {"pack_format": [1, 2, 3]}}"#);
        assert_eq!(codes(&validate_dir(&gen).unwrap()), vec!["invalid_json"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn valid_pack_passes_clean() {
        let root = temp_root("clean");
        let gen = root.join("generated");
        write(&root, "generated/pack.mcmeta", mcmeta().as_bytes());
        write(&root, "generated/pack.png", &png(2, 2));
        write(
            &root,
            "generated/assets/minecraft/textures/item/sword.png",
            &png(16, 16),
        );
        write(
            &root,
            "generated/assets/minecraft/models/item/sword.json",
            br#"{"textures": {"layer0": "item/sword"}}"#,
        );
        write(
            &root,
            "generated/assets/minecraft/lang/en_us.json",
            br#"{"key": "value"}"#,
        );
        let findings = validate_dir(&gen).unwrap();
        assert!(findings.is_empty(), "clean pack, got {findings:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn wrong_path_outside_assets_is_error() {
        let root = temp_root("outside");
        let gen = root.join("generated");
        write(&root, "generated/pack.mcmeta", mcmeta().as_bytes());
        write(&root, "generated/stray.png", &png(2, 2));
        write(&root, "generated/assets/minecraft/loose.png", &png(2, 2));
        write(
            &root,
            "generated/assets/minecraft/weird/x.png",
            &png(2, 2),
        );
        let findings = validate_dir(&gen).unwrap();
        let wp: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.code == "wrong_path")
            .collect();
        // sort theo rel path: assets/... < stray.png
        assert_eq!(wp.len(), 3);
        assert_eq!(wp[0].path, "assets/minecraft/loose.png");
        assert_eq!(
            wp[0].detail, "file trực tiếp dưới assets/minecraft/ (cần thư mục loại)"
        );
        assert_eq!(wp[1].path, "assets/minecraft/weird/x.png");
        assert_eq!(
            wp[1].detail,
            "thư mục 'weird' không thuộc assets/minecraft/ chuẩn"
        );
        assert_eq!(wp[2].path, "stray.png");
        assert_eq!(wp[2].detail, "outside assets/ (không đúng cấu trúc pack)");
        assert!(Finding::has_errors(&findings));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dir_ext_rules_reject_bad_extension() {
        let root = temp_root("extrules");
        let gen = root.join("generated");
        write(&root, "generated/pack.mcmeta", mcmeta().as_bytes());
        write(
            &root,
            "generated/assets/minecraft/textures/item/x.txt",
            b"nope",
        );
        write(
            &root,
            "generated/assets/minecraft/models/item/x.txt",
            b"nope",
        );
        write(&root, "generated/assets/minecraft/sounds/x.mp3", b"nope");
        let findings = validate_dir(&gen).unwrap();
        let wp: Vec<_> = findings.iter().filter(|f| f.code == "wrong_path").collect();
        // sort: models < sounds < textures
        assert_eq!(wp.len(), 3);
        assert!(wp[0].detail.starts_with("models/* chỉ nhận"));
        assert!(wp[1].detail.starts_with("sounds/* chỉ nhận"));
        assert!(wp[2].detail.starts_with("textures/* chỉ nhận"));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unsupported_ext_is_warning_only() {
        let root = temp_root("unsupported");
        let gen = root.join("generated");
        write(&root, "generated/pack.mcmeta", mcmeta().as_bytes());
        // shaders/ không có ext rule → chỉ unsupported WARNING (không wrong_path)
        write(&root, "generated/assets/minecraft/shaders/anim.glsl", b"GLSL");
        let findings = validate_dir(&gen).unwrap();
        let unsup: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "unsupported_asset")
            .collect();
        assert_eq!(unsup.len(), 1);
        assert_eq!(unsup[0].severity, "WARNING");
        assert_eq!(unsup[0].detail, "extension .glsl");
        // WARNING đơn lẻ KHÔNG fail (mục 42)
        assert!(!Finding::has_errors(&findings));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn oversized_file_is_error() {
        let root = temp_root("oversize");
        let gen = root.join("generated");
        write(&root, "generated/pack.mcmeta", mcmeta().as_bytes());
        write(
            &root,
            "generated/assets/minecraft/textures/item/big.png",
            &vec![0u8; MAX_FILE_BYTES + 1],
        );
        let findings = validate_dir(&gen).unwrap();
        let over: Vec<_> = findings.iter().filter(|f| f.code == "oversized").collect();
        assert_eq!(over.len(), 1);
        assert_eq!(over[0].severity, "ERROR");
        assert!(over[0].detail.contains("bytes > 8388608"));
        // parity: file .png còn nhận thêm missing_texture (nội dung zero ≠ PNG)
        assert!(findings.iter().any(|f| f.code == "missing_texture"));
        assert!(Finding::has_errors(&findings));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn corrupt_png_is_missing_texture() {
        let root = temp_root("corruptpng");
        let gen = root.join("generated");
        write(&root, "generated/pack.mcmeta", mcmeta().as_bytes());
        write(
            &root,
            "generated/assets/minecraft/textures/item/bad.png",
            b"not a png at all........",
        );
        // png dims 5000x5000 → "too large"
        let mut big = PNG_SIG.to_vec();
        big.extend_from_slice(&13u32.to_be_bytes());
        big.extend_from_slice(b"IHDR");
        big.extend_from_slice(&5000u32.to_be_bytes());
        big.extend_from_slice(&5000u32.to_be_bytes());
        big.extend_from_slice(&[8, 6, 0, 0, 0]);
        big.extend_from_slice(&[0u8; 20]);
        write(&root, "generated/assets/minecraft/textures/item/huge.png", &big);

        let findings = validate_dir(&gen).unwrap();
        let mt: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "missing_texture")
            .collect();
        assert_eq!(mt.len(), 2);
        assert_eq!(mt[0].detail, "not a valid PNG");
        assert_eq!(mt[1].detail, "too large: 5000x5000");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn invalid_json_is_error() {
        let root = temp_root("badjson");
        let gen = root.join("generated");
        write(&root, "generated/pack.mcmeta", mcmeta().as_bytes());
        write(
            &root,
            "generated/assets/minecraft/models/item/broken.json",
            br#"{"textures": "#
        );
        let findings = validate_dir(&gen).unwrap();
        let ij: Vec<_> = findings.iter().filter(|f| f.code == "invalid_json").collect();
        assert_eq!(ij.len(), 1);
        assert_eq!(ij[0].severity, "ERROR");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn model_ref_missing_is_error() {
        let root = temp_root("modelref");
        let gen = root.join("generated");
        write(&root, "generated/pack.mcmeta", mcmeta().as_bytes());
        write(
            &root,
            "generated/assets/minecraft/textures/item/exists.png",
            &png(2, 2),
        );
        // ref rút gọn → assets/minecraft/textures/missing.png (thiếu)
        // ref namespaced → assets/custom/textures/ns_missing.png (thiếu)
        // ref #... → skip
        write(
            &root,
            "generated/assets/minecraft/models/item/sword.json",
            br##"{"textures": {"layer0": "missing", "layer1": "custom:ns_missing", "particle": "#existing"}}"##,
        );
        let findings = validate_dir(&gen).unwrap();
        let mr: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "invalid_model_reference")
            .collect();
        assert_eq!(mr.len(), 2);
        assert_eq!(
            mr[0].detail,
            "textures.layer0 -> assets/minecraft/textures/missing.png missing"
        );
        assert_eq!(
            mr[1].detail,
            "textures.layer1 -> assets/custom/textures/ns_missing.png missing"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn duplicate_content_is_warning_by_sha1() {
        let root = temp_root("dup");
        let gen = root.join("generated");
        write(&root, "generated/pack.mcmeta", mcmeta().as_bytes());
        let same = png(4, 4);
        write(&root, "generated/assets/minecraft/textures/item/a.png", &same);
        write(&root, "generated/assets/minecraft/textures/item/b.png", &same);
        // pack.mcmeta thứ 2 trùng nội dung — skip theo parity (shaders/ không ext rule)
        write(&root, "generated/assets/minecraft/shaders/pack.mcmeta", mcmeta().as_bytes());
        let findings = validate_dir(&gen).unwrap();
        let dup: Vec<_> = findings
            .iter()
            .filter(|f| f.code == "duplicate_file")
            .collect();
        assert_eq!(dup.len(), 1);
        assert_eq!(dup[0].severity, "WARNING");
        assert_eq!(dup[0].path, "assets/minecraft/textures/item/b.png");
        assert_eq!(
            dup[0].detail,
            "duplicate of assets/minecraft/textures/item/a.png"
        );
        // chỉ WARNING — không fail
        assert!(!Finding::has_errors(&findings));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn duplicate_skip_applies_to_pack_mcmeta_paths() {
        // mcmeta thứ 2 cùng content: skip duplicate + ext .mcmeta OK + shaders/ không rule
        let root = temp_root("mcdup");
        let gen = root.join("generated");
        write(&root, "generated/pack.mcmeta", mcmeta().as_bytes());
        write(&root, "generated/assets/minecraft/shaders/pack.mcmeta", mcmeta().as_bytes());
        let findings = validate_dir(&gen).unwrap();
        assert!(findings.is_empty(), "clean, got {findings:?}");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn validate_zip_names_parity() {
        // thiếu pack.mcmeta
        let f = validate_zip_names(&["pack.png", "assets/minecraft/x.png"]);
        assert_eq!(codes(&f), vec!["invalid_metadata"]);
        // root file lạ
        let f = validate_zip_names(&["pack.mcmeta", "readme.txt", "assets/minecraft/x.png"]);
        assert_eq!(codes(&f), vec!["wrong_path"]);
        assert_eq!(f[0].path, "readme.txt");
        assert_eq!(f[0].detail, "outside assets/");
        // sạch
        let f = validate_zip_names(&[
            "pack.mcmeta",
            "pack.png",
            "assets/minecraft/textures/x.png",
            "assets/minecraft/",
        ]);
        assert!(f.is_empty());
        // case-insensitive prefix
        let f = validate_zip_names(&["pack.mcmeta", "ASSETS/minecraft/x.png"]);
        assert!(f.is_empty());
    }

    #[test]
    fn has_errors_and_finding_shape() {
        let findings = vec![
            Finding {
                code: "unsupported_asset",
                severity: "WARNING",
                path: "a.bmp".into(),
                detail: "extension .bmp".into(),
            },
            Finding {
                code: "oversized",
                severity: "ERROR",
                path: "b.png".into(),
                detail: "9 bytes > 8388608".into(),
            },
        ];
        assert!(Finding::has_errors(&findings));
        assert!(!Finding::has_errors(&[]));
        // serialize camelCase — key ổn định cho client
        let json = serde_json::to_value(&findings[0]).unwrap();
        assert_eq!(json["code"], "unsupported_asset");
        assert_eq!(json["severity"], "WARNING");
        assert_eq!(json["path"], "a.bmp");
        assert_eq!(json["detail"], "extension .bmp");
    }
}
