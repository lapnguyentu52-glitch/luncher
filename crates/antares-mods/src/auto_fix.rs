//! Mod auto-fix — parity `services/mods/auto_fix.py` (mục 176, 308).
//!
//! Flow (mục 176): resolve → plan → download → verify → commit.
//! `auto_fix_instance` orchestration dùng inject callbacks (read mods dir, fetch
//! versions, download file) — crate thuần test được, caller (Tauri command) nối
//! antares-downloads thật.

use std::path::Path;

use serde::Serialize;

use crate::health::{check_health, read_mod_info, ModInfo};
use crate::modrinth::{build_fix_plan, FixPlanItem};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoFixResult {
    pub issues_found: usize,
    pub dependencies_installed: Vec<String>,
    /// Plan không kèm url (parity: {k: v for ... if k != "url"}).
    pub plan: Vec<serde_json::Value>,
}

/// Port `apply_fix_plan` — mỗi item: download vào mods dir.
/// `download_one` inject: (url, target, sha1) → Ok khi thành công. Cancel được
/// qua trả Err (parity task.cancelled → DOWNLOAD_CANCELLED).
pub fn apply_fix_plan(
    mods_dir: &Path,
    plan: &[FixPlanItem],
    download_one: &mut dyn FnMut(&str, &Path, Option<&str>) -> Result<(), String>,
) -> Result<Vec<String>, String> {
    std::fs::create_dir_all(mods_dir).map_err(|err| err.to_string())?;
    let mut installed = Vec::new();
    for item in plan {
        let target = mods_dir.join(&item.filename);
        download_one(&item.url, &target, item.sha1.as_deref())?;
        installed.push(item.filename.clone());
    }
    Ok(installed)
}

/// Full auto-fix: scan mods dir → health → plan → download deps.
///
/// Callbacks inject:
/// - `fetch_versions(slug, loader, mc)` — None = lỗi resolve (parity log + continue)
/// - `download_one(url, target, sha1)` — Err = cancel/io
///
/// Parity `auto_fix_instance` return shape: issuesFound/dependenciesInstalled/plan.
pub fn auto_fix_instance(
    mods_dir: &Path,
    loader: &str,
    mc_version: &str,
    fetch_versions: &dyn Fn(&str, &str, &str) -> Option<Vec<serde_json::Value>>,
    download_one: &mut dyn FnMut(&str, &Path, Option<&str>) -> Result<(), String>,
) -> Result<AutoFixResult, String> {
    // Scan mods dir (sorted *.jar)
    let mut mods: Vec<ModInfo> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(mods_dir) {
        let mut names: Vec<String> = entries
            .flatten()
            .filter(|e| e.path().is_file())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "jar"))
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        for name in names {
            let bytes = std::fs::read(mods_dir.join(&name)).map_err(|err| err.to_string())?;
            mods.push(read_mod_info(&bytes, &name));
        }
    }

    let issues = check_health(&mods, loader);
    let plan = build_fix_plan(&issues, fetch_versions, loader, mc_version);
    let installed = apply_fix_plan(mods_dir, &plan, download_one)?;

    Ok(AutoFixResult {
        issues_found: issues.len(),
        dependencies_installed: installed,
        plan: plan
            .iter()
            .map(|p| {
                // parity: mọi field trừ url
                let mut obj = serde_json::to_value(p).unwrap_or(serde_json::Value::Null);
                if let Some(o) = obj.as_object_mut() {
                    o.remove("url");
                }
                obj
            })
            .collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jar_reader::tests::build_stored_zip;
    use std::cell::RefCell;

    #[test]
    fn auto_fix_full_flow_with_fabric_dep_missing() {
        let root = std::env::temp_dir()
            .join(format!("antares-autofix-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mods_dir = root.join("mods");
        std::fs::create_dir_all(&mods_dir).unwrap();

        // mod fabric thiếu fabric-api (depends dict)
        let fabric_json = r#"{"id":"mymod","version":"1.0","depends":{"fabric-api":"*"}}"#;
        let jar = build_stored_zip(&[("fabric.mod.json", fabric_json.as_bytes().to_vec())]);
        std::fs::write(mods_dir.join("mymod.jar"), &jar).unwrap();

        let fetched = RefCell::new(0);
        let fetch_versions = |slug: &str, loader: &str, _mc: &str| {
            assert_eq!(slug, "fabric-api");
            assert_eq!(loader, "fabric");
            *fetched.borrow_mut() += 1;
            Some(vec![serde_json::json!({
                "project_id": "P7dR8mSH",
                "version_number": "0.100.0",
                "files": [{
                    "filename": "fabric-api-0.100.0.jar",
                    "primary": true,
                    "url": "https://cdn.modrinth.com/fabric-api.jar",
                    "hashes": {"sha1": "abc"},
                }]
            })])
        };
        let downloaded = RefCell::new(Vec::new());
        let mut download_one = |url: &str, target: &Path, sha1: Option<&str>| {
            assert_eq!(url, "https://cdn.modrinth.com/fabric-api.jar");
            assert_eq!(sha1, Some("abc"));
            std::fs::write(target, b"jar bytes").unwrap();
            downloaded
                .borrow_mut()
                .push(target.file_name().unwrap().to_string_lossy().into_owned());
            Ok(())
        };

        let result = auto_fix_instance(
            &mods_dir,
            "fabric",
            "1.21.4",
            &fetch_versions,
            &mut download_one,
        )
        .unwrap();

        assert_eq!(result.issues_found, 1, "missing_dependency duy nhất");
        assert_eq!(result.dependencies_installed, vec!["fabric-api-0.100.0.jar"]);
        // plan không kèm url (parity)
        assert!(result.plan[0].get("url").is_none());
        assert_eq!(result.plan[0]["depId"], "fabric-api");
        // file thật đã tải vào mods dir
        assert!(mods_dir.join("fabric-api-0.100.0.jar").is_file());
        assert_eq!(*fetched.borrow(), 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn auto_fix_clean_dir_no_issue() {
        let root = std::env::temp_dir()
            .join(format!("antares-autofix-clean-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mods_dir = root.join("mods");
        std::fs::create_dir_all(&mods_dir).unwrap();

        let mut download = |_url: &str, _t: &Path, _s: Option<&str>| Ok(());
        let result = auto_fix_instance(
            &mods_dir,
            "fabric",
            "1.21.4",
            &|_s, _l, _m| None,
            &mut download,
        )
        .unwrap();
        assert_eq!(result.issues_found, 0);
        assert!(result.dependencies_installed.is_empty());
        assert!(result.plan.is_empty());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn apply_fix_propagates_cancel_error() {
        let root = std::env::temp_dir()
            .join(format!("antares-autofix-cancel-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mods_dir = root.join("mods");
        std::fs::create_dir_all(&mods_dir).unwrap();
        let plan = vec![FixPlanItem {
            for_mod: "a.jar".into(),
            dep_id: "fabric-api".into(),
            project_id: "P7".into(),
            filename: "fabric-api.jar".into(),
            url: "https://cdn/x.jar".into(),
            sha1: None,
            version: None,
        }];
        let mut cancelled = |_url: &str, _t: &Path, _s: Option<&str>| {
            Err("cancelled".to_string())
        };
        let err = apply_fix_plan(&mods_dir, &plan, &mut cancelled).unwrap_err();
        assert_eq!(err, "cancelled");
        let _ = std::fs::remove_dir_all(&root);
    }
}
