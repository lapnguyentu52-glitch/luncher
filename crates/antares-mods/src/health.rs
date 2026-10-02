//! Mod health checker — parity `services/mods/health.py` (mục 305, 451, 497).
//!
//! Metadata đọc từ jar: `fabric.mod.json` / `quilt.mod.json` / `mods.toml` /
//! `neoforge.mods.toml` / `mcmod.info` — đọc qua `zipread` thuần Rust.
//! `check_health`: thiếu dep, xung đột loader, breaks declaration. Outdated check
//! (Modrinth) là network — `version_newer` expose để caller dùng.

use serde::Serialize;
use std::collections::BTreeMap;

use crate::zipread::zip_read_entry;

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModInfo {
    pub filename: String,
    pub mod_id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    /// fabric | forge | quilt | neoforge | None
    pub loader: Option<&'static str>,
    pub depends: BTreeMap<String, String>,
    pub recommends: BTreeMap<String, String>,
    pub breaks: Vec<String>,
    pub readable: bool,
}

/// Đọc metadata mod từ jar bytes (đọc file trước bằng caller — khớp parity
/// `read_mod_info(jar_path)`).
pub fn read_mod_info(jar_bytes: &[u8], filename: &str) -> ModInfo {
    let mut info = ModInfo {
        filename: filename.to_string(),
        readable: true,
        ..Default::default()
    };

    // --- Fabric / Quilt ---
    for (marker, loader) in [("fabric.mod.json", "fabric"), ("quilt.mod.json", "quilt")] {
        let bytes = match zip_read_entry(jar_bytes, marker) {
            Ok(bytes) => bytes,
            // Jar không có entry này → thử loader khác.
            Err(crate::zipread::ZipError::EntryNotFound(_)) => continue,
            // Jar hỏng (không phải zip / corrupt) → unreadable, dừng (parity).
            Err(_) => {
                info.readable = false;
                return info;
            }
        };
        let Ok(data) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            info.readable = false;
            continue;
        };
        info.loader = Some(loader);
        let schema = if loader == "fabric" {
            &data
        } else {
            data.get("quilt_loader").unwrap_or(&serde_json::Value::Null)
        };
        info.mod_id = schema.get("id").and_then(|v| v.as_str()).map(str::to_string);
        info.name = schema
            .get("name")
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .or_else(|| info.mod_id.clone());
        info.version = schema
            .get("version")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        info.depends = clean_deps_map(schema.get("depends").or_else(|| schema.get("dependencies")));
        info.recommends = clean_deps_map(schema.get("recommends"));
        info.breaks = clean_deps_list(schema.get("breaks"));
        return info;
    }

    // --- Forge / NeoForge (mods.toml) ---
    for (marker, loader) in [
        ("META-INF/mods.toml", "forge"),
        ("META-INF/neoforge.mods.toml", "neoforge"),
    ] {
        let Ok(content) = zip_read_entry(jar_bytes, marker) else {
            continue;
        };
        let content = String::from_utf8_lossy(&content).into_owned();
        info.loader = Some(loader);
        if let Some(id) = find_toml_str(&content, "modId") {
            info.mod_id = Some(id);
        }
        if let Some(version) = find_toml_str(&content, "version") {
            info.version = Some(version);
        }
        if let Some(name) = find_toml_str(&content, "displayName") {
            info.name = Some(name);
        }
        // Parity: deps = mọi modId khác modId chính (spec "*")
        let self_id = info.mod_id.clone().unwrap_or_default();
        let mut deps = BTreeMap::new();
        for occ in toml_mod_ids(&content) {
            if occ != self_id {
                deps.insert(occ, "*".to_string());
            }
        }
        info.depends = deps;
        return info;
    }

    // --- Legacy (1.7/1.12 mcmod.info) ---
    if let Ok(bytes) = zip_read_entry(jar_bytes, "mcmod.info") {
        info.loader = Some("forge");
        if let Ok(data) = serde_json::from_slice::<serde_json::Value>(&bytes) {
            if let Some(first) = data.as_array().and_then(|a| a.first()) {
                info.mod_id = first.get("modid").and_then(|v| v.as_str()).map(str::to_string);
                info.name = first.get("name").and_then(|v| v.as_str()).map(str::to_string);
                info.version = first.get("version").and_then(|v| v.as_str()).map(str::to_string);
            }
        } else {
            info.readable = false;
        }
    }
    info
}

/// Fabric depends dạng {'fabric-api': '*', 'minecraft': '>=1.21'} — dict hoặc list.
fn clean_deps_map(raw: Option<&serde_json::Value>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    match raw {
        Some(serde_json::Value::Object(obj)) => {
            for (k, v) in obj {
                let spec = match v {
                    serde_json::Value::String(s) => s.clone(),
                    other => other.to_string(),
                };
                out.insert(k.clone(), spec);
            }
        }
        Some(serde_json::Value::Array(items)) => {
            for item in items {
                if let Some(k) = item.as_str() {
                    out.insert(k.to_string(), "*".to_string());
                }
            }
        }
        _ => {}
    }
    out
}

fn clean_deps_list(raw: Option<&serde_json::Value>) -> Vec<String> {
    match raw {
        Some(serde_json::Value::Object(obj)) => obj.keys().cloned().collect(),
        Some(serde_json::Value::Array(items)) => items
            .iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// Tìm giá trị `key = "value"` trong TOML nội tuyến (đơn giản — đủ mods.toml).
fn find_toml_str(content: &str, key: &str) -> Option<String> {
    for line in content.lines() {
        let line = line.trim();
        let prefix = format!("{key} =");
        let prefix2 = format!("{key}=");
        let rest = if let Some(r) = line.strip_prefix(&prefix) {
            r
        } else if let Some(r) = line.strip_prefix(&prefix2) {
            r
        } else {
            continue;
        };
        let rest = rest.trim();
        if let Some(value) = rest.strip_prefix('"').and_then(|r| r.split_once('"')) {
            return Some(value.0.to_string());
        }
    }
    None
}

/// Mọi `modId = "..."` trong TOML (parity re.findall — occurrence giữ trùng lặp
/// nhưng dedupe trong map caller).
fn toml_mod_ids(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in content.lines() {
        if let Some(id) = find_toml_str(line, "modId") {
            out.push(id);
        }
    }
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthIssue {
    pub r#mod: String,
    /// missing_dependency | wrong_loader | broken_dependency | unreadable
    pub kind: &'static str,
    pub detail: String,
    pub fixable: bool,
    pub suggestion: Option<String>,
}

/// Kiểm tra tập mod của instance: thiếu dep, lỗi loader, xung đột (parity
/// `check_health`). `minecraft`/`java` tự coi như đã cài.
pub fn check_health(mods: &[ModInfo], loader: &str) -> Vec<HealthIssue> {
    let mut issues = Vec::new();
    let mut installed_ids: std::collections::BTreeSet<String> = mods
        .iter()
        .filter_map(|m| m.mod_id.as_ref().map(|id| id.to_lowercase()))
        .collect();
    installed_ids.insert("minecraft".into());
    installed_ids.insert("java".into());

    for m in mods {
        // 1. Unreadable jar
        if !m.readable {
            issues.push(HealthIssue {
                r#mod: m.filename.clone(),
                kind: "unreadable",
                detail: "Jar hỏng hoặc không đọc được metadata".into(),
                fixable: false,
                suggestion: None,
            });
            continue;
        }
        // 2. Loader mismatch
        if let Some(mod_loader) = m.loader {
            if mod_loader != loader {
                issues.push(HealthIssue {
                    r#mod: m.filename.clone(),
                    kind: "wrong_loader",
                    detail: format!("Mod built cho {mod_loader} nhưng instance dùng {loader}"),
                    fixable: false,
                    suggestion: Some(mod_loader.to_string()),
                });
                continue;
            }
        }
        // 3. Missing dependencies (fabric depends)
        for (dep_id, dep_spec) in &m.depends {
            if !installed_ids.contains(&dep_id.to_lowercase()) {
                issues.push(HealthIssue {
                    r#mod: m.filename.clone(),
                    kind: "missing_dependency",
                    detail: format!("Thiếu dependency '{dep_id}' ({dep_spec})"),
                    fixable: true,
                    suggestion: Some(dep_id.clone()),
                });
            }
        }
        // 4. Breaks declaration
        for broken_id in &m.breaks {
            if installed_ids.contains(&broken_id.to_lowercase()) {
                issues.push(HealthIssue {
                    r#mod: m.filename.clone(),
                    kind: "broken_dependency",
                    detail: format!("Xung đột với mod '{broken_id}' đang cài"),
                    fixable: false,
                    suggestion: None,
                });
            }
        }
    }
    issues
}

/// So sánh semantic-ish: 1.2.10 > 1.2.9 (parity `_version_newer` — 4 phần số đầu).
pub fn version_newer(candidate: &str, current: &str) -> bool {
    fn parts(v: &str) -> Vec<u32> {
        let nums: Vec<u32> = v
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .take(4)
            .collect();
        if nums.is_empty() {
            vec![0]
        } else {
            nums
        }
    }
    parts(candidate) > parts(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::zipread::zip_read_entry;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// Golden stored zip từ Python — entry `fabric.mod.json` có mod mẫu.
    const STORED_ZIP: &str = "504b030414000000000054793d5d4f25b2af47000000470000000f0000006661627269632e6d6f642e6a736f6e7b22736368656d6156657273696f6e223a312c226964223a22746573742d6d6f64222c2276657273696f6e223a22312e302e30222c226e616d65223a2254657374204d6f64227d504b030414000000000054793d5d60080beb06000000060000000c0000006173736574732f782e706e6789504e470d0a504b0102140314000000000054793d5d4f25b2af47000000470000000f00000000000000000000008001000000006661627269632e6d6f642e6a736f6e504b0102140314000000000054793d5d60080beb06000000060000000c00000000000000000000008001740000006173736574732f782e706e67504b0506000000000200020077000000a40000000000";

    /// Builder jar stored nhiều entry (đơn giản — đủ test metadata).
    fn build_stored_zip(entries: &[(&str, &str)]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut offsets: Vec<(String, u32, u32, u32, u32)> = Vec::new(); // name, crc_placeholder, csize, usize, offset
        for (name, content) in entries {
            let offset = out.len() as u32;
            let crc = crc32(content.as_bytes());
            out.extend_from_slice(b"PK\x03\x04");
            out.extend_from_slice(&14u16.to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&0u16.to_le_bytes()); // method 0
            out.extend_from_slice(&0u16.to_le_bytes()); // time
            out.extend_from_slice(&0u16.to_le_bytes()); // date
            out.extend_from_slice(&crc.to_le_bytes());
            out.extend_from_slice(&(content.len() as u32).to_le_bytes());
            out.extend_from_slice(&(content.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes());
            out.extend_from_slice(name.as_bytes());
            out.extend_from_slice(content.as_bytes());
            offsets.push((
                name.to_string(),
                crc,
                content.len() as u32,
                content.len() as u32,
                offset,
            ));
        }
        let cd_offset = out.len() as u32;
        for (name, crc, csize, usize_, offset) in &offsets {
            out.extend_from_slice(b"PK\x01\x02");
            out.extend_from_slice(&14u16.to_le_bytes()); // 4..6
            out.extend_from_slice(&14u16.to_le_bytes()); // 6..8
            out.extend_from_slice(&0u16.to_le_bytes()); // 8..10 flags
            out.extend_from_slice(&0u16.to_le_bytes()); // 10..12 method
            out.extend_from_slice(&0u16.to_le_bytes()); // 12..14 time
            out.extend_from_slice(&0u16.to_le_bytes()); // 14..16 date
            out.extend_from_slice(&crc.to_le_bytes()); // 16..20
            out.extend_from_slice(&csize.to_le_bytes()); // 20..24
            out.extend_from_slice(&usize_.to_le_bytes()); // 24..28
            out.extend_from_slice(&(name.len() as u16).to_le_bytes()); // 28..30
            out.extend_from_slice(&0u16.to_le_bytes()); // 30..32 extra
            out.extend_from_slice(&0u16.to_le_bytes()); // 32..34 comment
            out.extend_from_slice(&0u16.to_le_bytes()); // 34..36 disk
            out.extend_from_slice(&0u16.to_le_bytes()); // 36..38 iattr
            out.extend_from_slice(&0u32.to_le_bytes()); // 38..42 eattr
            out.extend_from_slice(&offset.to_le_bytes()); // 42..46
            out.extend_from_slice(name.as_bytes());
        }
        let cd_size = out.len() as u32 - cd_offset;
        out.extend_from_slice(b"PK\x05\x06");
        out.extend_from_slice(&[0u8; 4]);
        out.extend_from_slice(&(offsets.len() as u16).to_le_bytes());
        out.extend_from_slice(&(offsets.len() as u16).to_le_bytes());
        out.extend_from_slice(&cd_size.to_le_bytes());
        out.extend_from_slice(&cd_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    fn crc32(data: &[u8]) -> u32 {
        let mut table = [0u32; 256];
        for (i, item) in table.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 { 0xEDB88320 ^ (c >> 1) } else { c >> 1 };
            }
            *item = c;
        }
        let mut crc = 0xFFFFFFFFu32;
        for &b in data {
            crc = table[((crc ^ b as u32) & 0xFF) as usize] ^ (crc >> 8);
        }
        crc ^ 0xFFFFFFFF
    }

    #[test]
    fn read_fabric_mod_json_metadata() {
        let data = hex(STORED_ZIP);
        let info = read_mod_info(&data, "test-mod.jar");
        assert!(info.readable);
        assert_eq!(info.loader, Some("fabric"));
        assert_eq!(info.mod_id.as_deref(), Some("test-mod"));
        assert_eq!(info.name.as_deref(), Some("Test Mod"));
        assert_eq!(info.version.as_deref(), Some("1.0.0"));
    }

    #[test]
    fn read_forge_toml_and_clean_deps() {
        // forge mod: mods.toml với modId chính + dependency
        let toml = r#"modLoader="javafml"
loaderVersion="[40,)"
[[mods]]
modId="mymod"
version="2.1.0"
displayName="My Mod"
[[dependencies.mymod]]
modId="forge"
[[dependencies.mymod]]
modId="librarymod"
"#;
        let jar = build_stored_zip(&[("META-INF/mods.toml", toml)]);
        let info = read_mod_info(&jar, "mymod.jar");
        assert_eq!(info.loader, Some("forge"));
        assert_eq!(info.mod_id.as_deref(), Some("mymod"));
        assert_eq!(info.version.as_deref(), Some("2.1.0"));
        assert_eq!(info.name.as_deref(), Some("My Mod"));
        // deps = mọi modId khác chính (forge + librarymod)
        assert!(info.depends.contains_key("forge"));
        assert!(info.depends.contains_key("librarymod"));
        assert!(!info.depends.contains_key("mymod"));
    }

    #[test]
    fn fabric_depends_list_and_dict_forms() {
        // depends dạng dict
        let fabric_json = r#"{"id":"a","version":"1.0","depends":{"fabric-api":"*","minecraft":">=1.21"}}"#;
        let jar = build_stored_zip(&[("fabric.mod.json", fabric_json)]);
        let info = read_mod_info(&jar, "a.jar");
        assert_eq!(info.depends.get("fabric-api").map(String::as_str), Some("*"));
        assert_eq!(info.depends.get("minecraft").map(String::as_str), Some(">=1.21"));
        // depends dạng list
        let fabric_list = r#"{"id":"b","version":"1.0","depends":["fabric-api"]}"#;
        let jar = build_stored_zip(&[("fabric.mod.json", fabric_list)]);
        let info = read_mod_info(&jar, "b.jar");
        assert_eq!(info.depends.get("fabric-api").map(String::as_str), Some("*"));
        // breaks dạng object keys
        let fabric_breaks = r#"{"id":"c","version":"1.0","breaks":{"badmod":"*"},"recommends":["recom"]}"#;
        let jar = build_stored_zip(&[("fabric.mod.json", fabric_breaks)]);
        let info = read_mod_info(&jar, "c.jar");
        assert_eq!(info.breaks, vec!["badmod".to_string()]);
        assert!(info.recommends.contains_key("recom"));
    }

    #[test]
    fn unreadable_and_missing_metadata() {
        // jar không metadata → readable true nhưng loader None (parity: không entry)
        let jar = build_stored_zip(&[("assets/x.png", "img")]);
        let info = read_mod_info(&jar, "plain.jar");
        assert_eq!(info.loader, None);
        assert!(info.readable);
        // fabric.mod.json JSON hỏng → readable false
        let jar = build_stored_zip(&[("fabric.mod.json", "{broken")]);
        let info = read_mod_info(&jar, "broken.jar");
        assert!(!info.readable);
        // jar hỏng hẳn → readable false
        let info = read_mod_info(b"not a zip", "bad.jar");
        assert!(!info.readable);
        // zip_read_entry entry không có
        assert!(zip_read_entry(&jar, "x").is_err());
    }

    #[test]
    fn check_health_matrix() {
        let mut dep_mod = ModInfo {
            filename: "dep.jar".into(),
            mod_id: Some("dep".into()),
            loader: Some("fabric"),
            readable: true,
            ..Default::default()
        };
        dep_mod
            .depends
            .insert("fabric-api".into(), "*".into());
        let wrong_loader = ModInfo {
            filename: "forge-mod.jar".into(),
            mod_id: Some("forgemod".into()),
            loader: Some("forge"),
            readable: true,
            ..Default::default()
        };
        let broken = ModInfo {
            filename: "conflict.jar".into(),
            mod_id: Some("conflict".into()),
            loader: Some("fabric"),
            readable: true,
            ..Default::default()
        };
        let mut with_breaks = broken.clone();
        with_breaks.breaks = vec!["dep".into()];
        let unreadable = ModInfo {
            filename: "bad.jar".into(),
            readable: false,
            ..Default::default()
        };

        let mods = vec![dep_mod, wrong_loader, with_breaks, unreadable];
        let issues = check_health(&mods, "fabric");
        let kinds: Vec<&str> = issues.iter().map(|i| i.kind).collect();
        assert!(kinds.contains(&"missing_dependency"), "fabric-api thiếu");
        assert!(kinds.contains(&"wrong_loader"));
        assert!(kinds.contains(&"broken_dependency"), "breaks dep đang cài");
        assert!(kinds.contains(&"unreadable"));
        // missing_dependency fixable + suggestion
        let missing = issues.iter().find(|i| i.kind == "missing_dependency").unwrap();
        assert!(missing.fixable);
        assert_eq!(missing.suggestion.as_deref(), Some("fabric-api"));
    }

    #[test]
    fn version_newer_semantic() {
        assert!(version_newer("1.2.10", "1.2.9"));
        assert!(version_newer("2.0", "1.9.9"));
        assert!(!version_newer("1.2.9", "1.2.10"));
        assert!(!version_newer("1.2.3", "1.2.3"));
        assert!(version_newer("1.10", "1.9"));
        assert!(!version_newer("abc", "1.0"), "no digits → [0] < [1]");
    }
}
