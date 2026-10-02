//! Mrpack (Modrinth modpack) — parity `services/mods/mrpack.py`.
//!
//! Học được từ lib (giữ nguyên note legacy):
//! 1. `modrinth.index.json` chứa `files[]` với `env` (client: required/optional/
//!    unsupported)
//! 2. Path traversal phải check (`ensure_inside` — mục 76)
//! 3. `overrides/` + `client-overrides/` extract thẳng vào game dir
//! 4. `dependencies["minecraft"]` + loader cài sau khi files xong (loader registry
//!    là việc tầng caller — crate trả plan đủ thông tin)
//!
//! Download thật chạy qua `antares-downloads::download` ở caller — crate trả
//! `InstallPlan` (download list + override extract list + deps) thuần để test.

use serde::Serialize;

use crate::zipread::{zip_entries, zip_read_entry, ZipError};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MrpackError {
    #[error("invalid .mrpack file: {0}")]
    Invalid(String),
    #[error("path escapes game dir: {0}")]
    PathEscape(String),
    #[error("io: {0}")]
    Io(String),
}

impl MrpackError {
    pub fn code(&self) -> &'static str {
        match self {
            // Parity: AntaresError(codes.VALIDATION_FAILED, "Invalid .mrpack file")
            MrpackError::Invalid(_) | MrpackError::PathEscape(_) => "VALIDATION_FAILED",
            MrpackError::Io(_) => "APP_INTERNAL",
        }
    }
}

/// Thông tin modpack — parity `read_mrpack_info` shape.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MrpackInfo {
    pub name: String,
    pub summary: String,
    pub version_id: String,
    pub minecraft_version: String,
    pub loader: String,
    pub loader_version: Option<String>,
    pub optional_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadItem {
    pub path: String,
    pub url: String,
    pub sha1: Option<String>,
    pub size: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OverrideItem {
    pub zip_name: String,
    pub target_rel: String,
    pub content: Vec<u8>,
}

/// Plan cài đặt — caller thực thi download + extract + loader install.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPlan {
    pub info: MrpackInfo,
    pub downloads: Vec<DownloadItem>,
    pub overrides: Vec<OverrideItem>,
    pub mc_version: String,
    pub loader: String,
}

/// Đọc `modrinth.index.json` + info (parity `read_mrpack_info`).
pub fn read_mrpack_info(mrpack_bytes: &[u8]) -> Result<MrpackInfo, MrpackError> {
    let index = index_json(mrpack_bytes)?;
    let deps = index.get("dependencies").cloned().unwrap_or(serde_json::Value::Null);
    let deps_obj = deps.as_object();
    Ok(MrpackInfo {
        name: index
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        summary: index
            .get("summary")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        version_id: index
            .get("versionId")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        minecraft_version: deps_obj
            .and_then(|d| d.get("minecraft"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        loader: detect_loader(deps_obj).to_string(),
        loader_version: loader_version(deps_obj).map(str::to_string),
        optional_files: index
            .get("files")
            .and_then(|v| v.as_array())
            .map(|files| {
                files
                    .iter()
                    .filter(|f| {
                        f.get("env")
                            .and_then(|e| e.get("client"))
                            .and_then(|c| c.as_str())
                            == Some("optional")
                    })
                    .filter_map(|f| f.get("path").and_then(|p| p.as_str()))
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default(),
    })
}

fn index_json(mrpack_bytes: &[u8]) -> Result<serde_json::Value, MrpackError> {
    let bytes = zip_read_entry(mrpack_bytes, "modrinth.index.json")
        .map_err(|err| match err {
            ZipError::EntryNotFound(_) | ZipError::NotZip | ZipError::NoEocd => {
                MrpackError::Invalid(err.to_string())
            }
            other => MrpackError::Io(other.to_string()),
        })?;
    serde_json::from_slice(&bytes).map_err(|err| MrpackError::Invalid(err.to_string()))
}

/// Parity `_detect_loader`: fabric → quilt → neoforge → forge → vanilla.
pub fn detect_loader(deps: Option<&serde_json::Map<String, serde_json::Value>>) -> &'static str {
    let Some(deps) = deps else {
        return "vanilla";
    };
    for (key, loader) in [
        ("fabric-loader", "fabric"),
        ("quilt-loader", "quilt"),
        ("neoforge", "neoforge"),
        ("forge", "forge"),
    ] {
        if deps.contains_key(key) {
            return loader;
        }
    }
    "vanilla"
}

/// Parity `_loader_version`.
pub fn loader_version(deps: Option<&serde_json::Map<String, serde_json::Value>>) -> Option<&str> {
    let deps = deps?;
    for key in ["fabric-loader", "quilt-loader", "neoforge", "forge"] {
        if let Some(v) = deps.get(key).and_then(|v| v.as_str()) {
            return Some(v);
        }
    }
    None
}

/// Parity `_filter_files`: env None giữ; client required giữ; optional chỉ khi
/// được chọn; còn lại bỏ.
pub fn filter_files(
    files: &[serde_json::Value],
    optional_selected: &[String],
) -> Vec<serde_json::Value> {
    files
        .iter()
        .filter(|f| {
            // Path escape (parity ensure_inside): loại ../ và absolute ngay khi filter.
            if let Some(p) = f.get("path").and_then(|p| p.as_str()) {
                if p.starts_with("..") || p.starts_with('/') || p.starts_with('\\') {
                    return false;
                }
            }
            let env = f.get("env");
            let Some(env) = env else {
                return true;
            };
            match env.get("client").and_then(|c| c.as_str()) {
                Some("required") => true,
                Some("optional") => f
                    .get("path")
                    .and_then(|p| p.as_str())
                    .map(|p| optional_selected.iter().any(|s| s == p))
                    .unwrap_or(false),
                _ => false,
            }
        })
        .cloned()
        .collect()
}

/// `ensure_inside` parity (core/utils/paths.py): relative path phải nằm trong
/// game_dir — reject `../` tuyệt đối (mục 76). Trả path hợp lệ đã chuẩn.
pub fn ensure_inside(game_dir: &std::path::Path, target: &std::path::Path) -> Result<std::path::PathBuf, MrpackError> {
    let candidate = target
        .strip_prefix(game_dir)
        .map_err(|_| MrpackError::PathEscape(target.display().to_string()))?;
    for comp in candidate.components() {
        match comp {
            std::path::Component::Normal(_) => {}
            _ => {
                return Err(MrpacheEscapeHelper::err(target));
            }
        }
    }
    Ok(game_dir.join(candidate))
}

/// Helper nhỏ tránh dư code (dùng 1 nơi).
struct MrpacheEscapeHelper;
impl MrpacheEscapeHelper {
    fn err(target: &std::path::Path) -> MrpackError {
        MrpackError::PathEscape(target.display().to_string())
    }
}

/// Dựng install plan từ .mrpack bytes — download list (filter env) + overrides
/// (extract thẳng game dir, skip file 0 byte — parity) + deps info.
pub fn build_install_plan(
    mrpack_bytes: &[u8],
    optional_selected: &[String],
) -> Result<InstallPlan, MrpackError> {
    let index = index_json(mrpack_bytes)?;
    let info = read_mrpack_info(mrpack_bytes)?;
    let deps_obj = index.get("dependencies").and_then(|v| v.as_object());

    // 1. Download files (filter required + optional được chọn)
    let raw_files = index.get("files").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    // Validate traversal trên MỌI file TRƯỚC khi filter — pack chứa path escape
    // → reject cả plan (VALIDATION_FAILED), không chỉ bỏ qua entry đó.
    for file in &raw_files {
        if let Some(path) = file.get("path").and_then(|v| v.as_str()) {
            ensure_inside(
                std::path::Path::new("game/"),
                &std::path::Path::new("game/").join(path),
            )?;
        }
    }
    let mut downloads = Vec::new();
    for file in filter_files(&raw_files, optional_selected) {
        let path = file
            .get("path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| MrpackError::Invalid("file missing path".into()))?
            .to_string();
        // traversal check ngay khi plan (parity ensure_inside ở download loop)
        ensure_inside(std::path::Path::new("game/"), &std::path::Path::new("game/").join(&path))?;
        let url = file
            .get("downloads")
            .and_then(|d| d.as_array())
            .and_then(|d| d.first())
            .and_then(|u| u.as_str())
            .unwrap_or_default()
            .to_string();
        downloads.push(DownloadItem {
            path,
            url,
            sha1: file
                .get("hashes")
                .and_then(|h| h.get("sha1"))
                .and_then(|v| v.as_str())
                .map(str::to_string),
            size: file.get("fileSize").and_then(|v| v.as_u64()),
        });
    }

    // 2. Overrides entries — overrides/ + client-overrides/, skip 0 byte
    let mut overrides = Vec::new();
    for entry in zip_entries(mrpack_bytes).map_err(|err| MrpackError::Invalid(err.to_string()))? {
        if !entry.name.starts_with("overrides/") && !entry.name.starts_with("client-overrides/") {
            continue;
        }
        if entry.uncompressed_size == 0 {
            continue;
        }
        let prefix = if entry.name.starts_with("client-overrides/") {
            "client-overrides/"
        } else {
            "overrides/"
        };
        let rel = &entry.name[prefix.len()..];
        ensure_inside(std::path::Path::new("game/"), &std::path::Path::new("game/").join(rel))?;
        let content = zip_read_entry(mrpack_bytes, &entry.name)
            .map_err(|err| MrpackError::Io(err.to_string()))?;
        overrides.push(OverrideItem {
            zip_name: entry.name.clone(),
            target_rel: rel.to_string(),
            content,
        });
    }

    Ok(InstallPlan {
        mc_version: deps_obj
            .and_then(|d| d.get("minecraft"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string(),
        loader: detect_loader(deps_obj).to_string(),
        info,
        downloads,
        overrides,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jar_reader::tests::build_stored_zip;

    const INDEX: &str = r#"{
        "formatVersion": 1,
        "name": "Test Pack",
        "summary": "A test pack",
        "versionId": "1.2.3",
        "dependencies": {
            "minecraft": "1.21.4",
            "fabric-loader": "0.16.0"
        },
        "files": [
            {"path": "mods/required.jar", "env": {"client": "required", "server": "required"},
             "downloads": ["https://cdn.modrinth.com/required.jar"],
             "hashes": {"sha1": "aaa"}, "fileSize": 100},
            {"path": "mods/optional.jar", "env": {"client": "optional", "server": "unsupported"},
             "downloads": ["https://cdn.modrinth.com/optional.jar"],
             "hashes": {"sha1": "bbb"}, "fileSize": 50},
            {"path": "mods/unsupported.jar", "env": {"client": "unsupported"},
             "downloads": ["https://cdn.modrinth.com/x.jar"], "fileSize": 1},
            {"path": "mods/noenv.jar", "downloads": ["https://cdn.modrinth.com/y.jar"], "fileSize": 2},
            {"path": "../evil.jar", "downloads": ["https://cdn.modrinth.com/evil.jar"], "fileSize": 3}
        ]
    }"#;

    fn pack_zip() -> Vec<u8> {
        build_stored_zip(&[
            ("modrinth.index.json", INDEX.as_bytes().to_vec()),
            ("overrides/config/opts.txt", b"gamma:1.0".to_vec()),
            ("overrides/empty.txt", b"".to_vec()), // 0 byte → skip
            ("client-overrides/config/client.txt", b"client".to_vec()),
            ("unrelated/file.txt", b"no".to_vec()), // bỏ qua
        ])
    }

    #[test]
    fn read_mrpack_info_parity() {
        let info = read_mrpack_info(&pack_zip()).unwrap();
        assert_eq!(info.name, "Test Pack");
        assert_eq!(info.summary, "A test pack");
        assert_eq!(info.version_id, "1.2.3");
        assert_eq!(info.minecraft_version, "1.21.4");
        assert_eq!(info.loader, "fabric");
        assert_eq!(info.loader_version.as_deref(), Some("0.16.0"));
        assert_eq!(info.optional_files, vec!["mods/optional.jar"]);
        // không phải mrpack → VALIDATION_FAILED
        assert_eq!(
            read_mrpack_info(b"not a zip").unwrap_err().code(),
            "VALIDATION_FAILED"
        );
        // zip không có index → VALIDATION_FAILED
        assert_eq!(
            read_mrpack_info(&build_stored_zip(&[("other.txt", b"x".to_vec())]))
                .unwrap_err()
                .code(),
            "VALIDATION_FAILED"
        );
    }

    #[test]
    fn detect_loader_and_version_parity() {
        let mk = |pairs: &[(&str, &str)]| -> serde_json::Map<String, serde_json::Value> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), serde_json::json!(v)))
                .collect()
        };
        assert_eq!(detect_loader(Some(&mk(&[("fabric-loader", "1")]))), "fabric");
        assert_eq!(detect_loader(Some(&mk(&[("quilt-loader", "1")]))), "quilt");
        assert_eq!(detect_loader(Some(&mk(&[("neoforge", "1")]))), "neoforge");
        assert_eq!(detect_loader(Some(&mk(&[("forge", "1")]))), "forge");
        assert_eq!(detect_loader(Some(&mk(&[("minecraft", "1.21")]))), "vanilla");
        assert_eq!(detect_loader(None), "vanilla");
        // ưu tiên fabric trước các loader khác (parity thứ tự)
        assert_eq!(
            detect_loader(Some(&mk(&[("forge", "1"), ("fabric-loader", "1")]))),
            "fabric"
        );
    }

    #[test]
    fn filter_files_required_optional_selected() {
        let index: serde_json::Value = serde_json::from_str(INDEX).unwrap();
        let files = index["files"].as_array().unwrap().clone();
        // chỉ required + no-env
        let selected = filter_files(&files, &[]);
        let paths: Vec<&str> = selected
            .iter()
            .filter_map(|f| f.get("path").and_then(|p| p.as_str()))
            .collect();
        assert_eq!(paths, vec!["mods/required.jar", "mods/noenv.jar"]);
        // chọn optional
        let selected = filter_files(&files, &["mods/optional.jar".to_string()]);
        let paths: Vec<&str> = selected
            .iter()
            .filter_map(|f| f.get("path").and_then(|p| p.as_str()))
            .collect();
        assert_eq!(
            paths,
            vec!["mods/required.jar", "mods/optional.jar", "mods/noenv.jar"]
        );
        // unsupported luôn bị bỏ
        assert!(!paths.contains(&"mods/unsupported.jar"));
    }

    #[test]
    fn ensure_inside_rejects_traversal() {
        let game = std::path::Path::new("game/");
        assert_eq!(
            ensure_inside(game, &game.join("../evil.jar")).unwrap_err().code(),
            "VALIDATION_FAILED"
        );
        assert!(ensure_inside(game, &game.join("mods/ok.jar")).is_ok());
    }

    #[test]
    fn install_plan_downloads_overrides_and_traversal_reject() {
        // pack có path `../evil.jar` → cả plan phải reject (VALIDATION_FAILED)
        let err = build_install_plan(&pack_zip(), &["mods/optional.jar".to_string()]).unwrap_err();
        assert_eq!(err.code(), "VALIDATION_FAILED");
        assert!(err.to_string().contains("evil.jar"));

        // pack sạch (bỏ file evil) → plan đủ downloads + overrides
        let index: serde_json::Value = serde_json::from_str(INDEX).unwrap();
        let mut clean = index.clone();
        clean["files"] = serde_json::json!([
            {"path": "mods/required.jar", "env": {"client": "required"},
             "downloads": ["https://cdn.modrinth.com/required.jar"],
             "hashes": {"sha1": "aaa"}, "fileSize": 100},
            {"path": "mods/optional.jar", "env": {"client": "optional"},
             "downloads": ["https://cdn.modrinth.com/optional.jar"],
             "hashes": {"sha1": "bbb"}, "fileSize": 50}
        ]);
        let clean_index = serde_json::to_string(&clean).unwrap();
        let zip = build_stored_zip(&[
            ("modrinth.index.json", clean_index.into_bytes()),
            ("overrides/config/opts.txt", b"gamma:1.0".to_vec()),
            ("overrides/empty.txt", b"".to_vec()),
            ("client-overrides/client.txt", b"c".to_vec()),
        ]);
        let plan = build_install_plan(&zip, &["mods/optional.jar".to_string()]).unwrap();
        assert_eq!(plan.downloads.len(), 2);
        assert_eq!(plan.downloads[0].path, "mods/required.jar");
        assert_eq!(plan.downloads[0].sha1.as_deref(), Some("aaa"));
        // overrides: empty.txt bị skip (0 byte), unrelated bỏ
        assert_eq!(plan.overrides.len(), 2);
        assert!(plan
            .overrides
            .iter()
            .any(|o| o.target_rel == "config/opts.txt"));
        assert!(plan.overrides.iter().any(|o| o.target_rel == "client.txt"));
        assert_eq!(plan.mc_version, "1.21.4");
        assert_eq!(plan.loader, "fabric");
    }
}
