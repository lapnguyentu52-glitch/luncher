//! Modrinth provider — parity `services/mods/modrinth.py` + phần network của
//! `auto_fix.py` (mục 18, 491, 176, 308).
//!
//! HTTP chạy qua `antares-downloads::get` (TLS seam — https fail rõ ràng tới khi
//! bundle Batch 16). Search/versions chỉ build URL + parse JSON (thuần, test
//! được); `fetch_*` là phần I/O mỏng bọc get.

use antares_downloads::get;
use serde::Serialize;
use std::time::Duration;

pub const API: &str = "https://api.modrinth.com/v2";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModrinthError {
    #[error("http: {0}")]
    Http(String),
    #[error("invalid response: {0}")]
    Parse(String),
}

impl ModrinthError {
    pub fn code(&self) -> &'static str {
        match self {
            ModrinthError::Http(_) => "NET_UNREACHABLE",
            ModrinthError::Parse(_) => "CONFIG_INVALID",
        }
    }
}

/// Parity `search`: facets `[[categories:loader],[versions:mc]]` + query/index.
pub fn search_url(query: &str, loader: &str, mc_version: &str, limit: usize) -> String {
    let facets = format!(
        r#"[["categories:{}"],["versions:{}"]]"#,
        loader, mc_version
    );
    let mut url = format!(
        "{API}/search?facets={}&limit={}",
        urlencode(&facets),
        limit
    );
    if !query.is_empty() {
        url.push_str(&format!("&query={}", urlencode(query)));
    } else {
        url.push_str("&index=downloads");
    }
    url
}

/// Parity `get_versions` params: game_versions + loaders (JSON arrays).
pub fn versions_url(project_id: &str, loader: &str, mc_version: &str) -> String {
    format!(
        "{API}/project/{}/version?game_versions={}&loaders={}",
        urlencode(project_id),
        urlencode(&format!(r#"["{mc_version}"]"#)),
        urlencode(&format!(r#"["{loader}"]"#)),
    )
}

/// Minimal percent-encode cho query params (space/+ và ký tự đặc biệt).
pub fn urlencode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

/// `pick_file` parity: file `primary` đầu tiên, fallback file đầu, None nếu rỗng.
pub fn pick_file(version: &serde_json::Value) -> Option<serde_json::Value> {
    let files = version.get("files")?.as_array()?;
    files
        .iter()
        .find(|f| f.get("primary").and_then(|v| v.as_bool()).unwrap_or(false))
        .or_else(|| files.first())
        .cloned()
}

/// Parse hits từ response `search` — trả danh sách gọn (title/slug/id/downloads).
pub fn parse_search_hits(body: &[u8]) -> Result<Vec<serde_json::Value>, ModrinthError> {
    let data: serde_json::Value =
        serde_json::from_slice(body).map_err(|err| ModrinthError::Parse(err.to_string()))?;
    Ok(data
        .get("hits")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default())
}

/// GET search — trả hits. `timeout` inject để test nhanh.
pub fn fetch_search(
    query: &str,
    loader: &str,
    mc_version: &str,
    limit: usize,
    timeout: Duration,
) -> Result<Vec<serde_json::Value>, ModrinthError> {
    let response = get(&search_url(query, loader, mc_version, limit), &[], timeout)
        .map_err(|err| ModrinthError::Http(format!("{} ({})", err, err.code())))?;
    if response.status != 200 {
        return Err(ModrinthError::Http(format!("search HTTP {}", response.status)));
    }
    parse_search_hits(&response.body)
}

/// GET versions — trả array; rỗng = không có build cho loader/version (parity
/// raise VALIDATION_FAILED "No {loader} build" — caller đổi thành error).
pub fn fetch_versions(
    project_id: &str,
    loader: &str,
    mc_version: &str,
    timeout: Duration,
) -> Result<Vec<serde_json::Value>, ModrinthError> {
    let response = get(&versions_url(project_id, loader, mc_version), &[], timeout)
        .map_err(|err| ModrinthError::Http(format!("{} ({})", err, err.code())))?;
    if response.status != 200 {
        return Err(ModrinthError::Http(format!("versions HTTP {}", response.status)));
    }
    let data: serde_json::Value =
        serde_json::from_slice(&response.body).map_err(|err| ModrinthError::Parse(err.to_string()))?;
    Ok(data.as_array().cloned().unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Auto-fix plan — parity `build_fix_plan` (mục 176, 308)
// ---------------------------------------------------------------------------

/// Map dependency id phổ biến → Modrinth project slug; None = không auto-fix
/// (loader runtime/java/minecraft không phải mod). Parity `KNOWN_DEPS`.
pub fn known_dep_slug(dep_id: &str) -> Option<&'static str> {
    match dep_id.to_lowercase().as_str() {
        "fabric-api" => Some("fabric-api"),
        "fabricloader" | "java" | "minecraft" => None,
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FixPlanItem {
    pub for_mod: String,
    pub dep_id: String,
    pub project_id: String,
    pub filename: String,
    pub url: String,
    pub sha1: Option<String>,
    pub version: Option<String>,
}

/// Build fix plan từ issues (chỉ missing_dependency + suggestion nằm trong
/// KNOWN_DEPS; dedupe theo dep_id). `versions_of` inject để test (parity client).
pub fn build_fix_plan(
    issues: &[crate::health::HealthIssue],
    versions_of: &dyn Fn(&str, &str, &str) -> Option<Vec<serde_json::Value>>,
    loader: &str,
    mc_version: &str,
) -> Vec<FixPlanItem> {
    let mut plan = Vec::new();
    let mut seen: std::collections::BTreeSet<String> = Default::default();
    for issue in issues {
        if issue.kind != "missing_dependency" {
            continue;
        }
        let Some(suggestion) = &issue.suggestion else {
            continue;
        };
        let dep_id = suggestion.to_lowercase();
        if seen.contains(&dep_id) {
            continue;
        }
        let Some(slug) = known_dep_slug(&dep_id) else {
            continue;
        };
        let Some(versions) = versions_of(slug, loader, mc_version) else {
            continue;
        };
        let Some(first) = versions.first() else {
            continue;
        };
        let Some(file) = pick_file(first) else {
            continue;
        };
        plan.push(FixPlanItem {
            for_mod: issue.r#mod.clone(),
            dep_id: dep_id.clone(),
            project_id: first
                .get("project_id")
                .and_then(|v| v.as_str())
                .unwrap_or(slug)
                .to_string(),
            filename: file
                .get("filename")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            url: file
                .get("url")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string(),
            sha1: file
                .get("hashes")
                .and_then(|h| h.get("sha1"))
                .and_then(|v| v.as_str())
                .map(str::to_string),
            version: first
                .get("version_number")
                .and_then(|v| v.as_str())
                .map(str::to_string),
        });
        seen.insert(dep_id);
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_url_parity() {
        // parity: facets JSON + query hoặc index=downloads
        let url = search_url("jei", "fabric", "1.21.4", 40);
        assert!(url.starts_with("https://api.modrinth.com/v2/search?"));
        assert!(url.contains(&urlencode(r#"[["categories:fabric"],["versions:1.21.4"]]"#)));
        assert!(url.contains("limit=40"));
        assert!(url.contains(&format!("query={}", urlencode("jei"))));
        // query rỗng → index=downloads
        let url = search_url("", "forge", "1.20.1", 40);
        assert!(url.contains("index=downloads"));
        assert!(!url.contains("query="));
    }

    #[test]
    fn versions_url_parity() {
        let url = versions_url("fabric-api", "fabric", "1.21.4");
        assert!(url.starts_with("https://api.modrinth.com/v2/project/fabric-api/version"));
        assert!(url.contains(&urlencode(r#"["1.21.4"]"#)));
        assert!(url.contains(&urlencode(r#"["fabric"]"#)));
    }

    #[test]
    fn urlencode_special_chars() {
        assert_eq!(urlencode("a b&c=d/e"), "a%20b%26c%3Dd%2Fe");
        assert_eq!(urlencode("safe-name_1.2"), "safe-name_1.2");
        assert_eq!(urlencode(""), "");
    }

    #[test]
    fn pick_file_primary_first() {
        let version = serde_json::json!({
            "files": [
                {"filename": "secondary.jar", "primary": false},
                {"filename": "primary.jar", "primary": true},
            ]
        });
        assert_eq!(
            pick_file(&version).unwrap()["filename"],
            "primary.jar"
        );
        // không primary → file đầu
        let version = serde_json::json!({
            "files": [{"filename": "only.jar", "primary": false}]
        });
        assert_eq!(pick_file(&version).unwrap()["filename"], "only.jar");
        // rỗng → None
        assert!(pick_file(&serde_json::json!({"files": []})).is_none());
        assert!(pick_file(&serde_json::json!({})).is_none());
    }

    #[test]
    fn parse_search_hits_shape() {
        let body = serde_json::json!({
            "hits": [{"slug": "jei", "downloads": 1000}, {"slug": "sodium"}],
            "total_hits": 2
        });
        let hits = parse_search_hits(body.to_string().as_bytes()).unwrap();
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0]["slug"], "jei");
        // hits thiếu → rỗng (không lỗi)
        assert!(parse_search_hits(br#"{}"#).unwrap().is_empty());
        assert!(parse_search_hits(b"{bad").is_err());
    }

    #[test]
    fn known_deps_parity() {
        assert_eq!(known_dep_slug("fabric-api"), Some("fabric-api"));
        assert_eq!(known_dep_slug("FABRIC-API"), Some("fabric-api"));
        // loader runtime / java / minecraft → None (không auto-fix)
        assert_eq!(known_dep_slug("fabricloader"), None);
        assert_eq!(known_dep_slug("java"), None);
        assert_eq!(known_dep_slug("minecraft"), None);
        // dep lạ → None (parity KNOWN_DEPS.get → None)
        assert_eq!(known_dep_slug("randommod"), None);
    }

    #[test]
    fn build_fix_plan_dedupe_and_skip_non_fixable() {
        use crate::health::HealthIssue;
        let issues = vec![
            HealthIssue {
                r#mod: "mod-a.jar".into(),
                kind: "missing_dependency",
                detail: "missing fabric-api".into(),
                fixable: true,
                suggestion: Some("fabric-api".into()),
            },
            HealthIssue {
                r#mod: "mod-b.jar".into(),
                kind: "missing_dependency",
                detail: "missing fabric-api".into(),
                fixable: true,
                suggestion: Some("fabric-api".into()), // trùng → dedupe
            },
            HealthIssue {
                r#mod: "mod-c.jar".into(),
                kind: "missing_dependency",
                detail: "missing minecraft".into(),
                fixable: true,
                suggestion: Some("minecraft".into()), // KNOWN_DEPS → None → skip
            },
            HealthIssue {
                r#mod: "mod-d.jar".into(),
                kind: "wrong_loader",
                detail: "forge".into(),
                fixable: false,
                suggestion: Some("forge".into()), // kind khác → skip
            },
        ];
        let versions_of = |slug: &str, loader: &str, _mc: &str| {
            assert_eq!(slug, "fabric-api");
            assert_eq!(loader, "fabric");
            Some(vec![serde_json::json!({
                "project_id": "P7dR8mSH",
                "version_number": "0.100.0",
                "files": [{
                    "filename": "fabric-api-0.100.0.jar",
                    "primary": true,
                    "url": "https://cdn.modrinth.com/fabric-api.jar",
                    "hashes": {"sha1": "abc123"},
                }]
            })])
        };
        let plan = build_fix_plan(&issues, &versions_of, "fabric", "1.21.4");
        assert_eq!(plan.len(), 1, "dedupe + skip non-known + skip non-missing");
        assert_eq!(plan[0].dep_id, "fabric-api");
        assert_eq!(plan[0].for_mod, "mod-a.jar");
        assert_eq!(plan[0].filename, "fabric-api-0.100.0.jar");
        assert_eq!(plan[0].sha1.as_deref(), Some("abc123"));
    }
}
