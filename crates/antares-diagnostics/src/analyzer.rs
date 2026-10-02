//! LogAnalyzer — parity 1:1 `services/diagnostics/log_analyzer.py` (mục 41).
//!
//! Blueprint matching — KHÔNG LLM (mục 24): pattern → id/seed/recommend/action.
//! Match theo từng dòng (contains, case-insensitive như re.IGNORECASE), gộp insights
//! theo id: count/severity/seed/firstSeen/lastSeen/sources[]/lines[] (tối đa 5,
//! text cắt 240 ký tự). Sort: count giảm, rồi severity rank error(1) > warning(0).

use serde::Serialize;

use crate::{crash_reports, minecraft_log, DiagPaths, DiagnosticsError};

/// Thứ tự cố định để preview ổn định (mục 10.5).
pub const LOG_SOURCES: &[&str] = &["launcher", "minecraft", "crash"];

pub const BLUEPRINT_COUNT: usize = 11;

/// Một blueprint — parity entry trong `_BLUEPRINTS`.
#[derive(Debug, Clone, Copy)]
pub struct Blueprint {
    pub id: &'static str,
    /// Needle case-insensitive (legacy pattern là alternation — matcher contains).
    pub needles: &'static [&'static str],
    pub seed: &'static str,
    pub recommend: &'static str,
    pub action_kind: &'static str,
    pub action_target: &'static str,
}

pub const BLUEPRINTS: &[Blueprint] = &[
    Blueprint {
        id: "java_out_of_memory",
        needles: &["OutOfMemoryError"],
        seed: "oom",
        recommend: "profiles.ramMax",
        action_kind: "route",
        action_target: "gameOptimization",
    },
    Blueprint {
        id: "java_class_version",
        needles: &["UnsupportedClassVersionError", "bad major version", "class file version"],
        seed: "javaVersion",
        recommend: "nav.diagnostics",
        action_kind: "route",
        action_target: "diagnostics",
    },
    Blueprint {
        id: "auth_failed",
        needles: &[
            "Invalid session",
            "Invalid token",
            "Failed to verify",
            "AUTH_FAILED",
            "authserver.ely.by",
        ],
        seed: "auth",
        recommend: "nav.accounts",
        action_kind: "route",
        action_target: "accounts",
    },
    Blueprint {
        id: "java_missing",
        needles: &["JAVA_NOT_FOUND", "Java not found", "No Java installation"],
        seed: "javaMissing",
        recommend: "nav.diagnostics",
        action_kind: "route",
        action_target: "diagnostics",
    },
    Blueprint {
        id: "mod_conflict",
        needles: &[
            "DuplicateModsFoundException",
            "ModResolutionException",
            "LoaderExceptionModCrash",
            "FabricException",
        ],
        seed: "modConflict",
        recommend: "nav.mods",
        action_kind: "route",
        action_target: "mods",
    },
    Blueprint {
        id: "connection_refused",
        needles: &[
            "Connection refused",
            "ConnectionRefusedError",
            "Failed to connect to the server",
        ],
        seed: "connRefused",
        recommend: "profiles.server",
        action_kind: "net",
        action_target: "tcp",
    },
    Blueprint {
        id: "connection_timeout",
        needles: &[
            "Connection timed out",
            "connect timed out",
            "SocketTimeoutException",
        ],
        seed: "connTimeout",
        recommend: "profiles.server",
        action_kind: "net",
        action_target: "tcp",
    },
    Blueprint {
        id: "disk_full",
        needles: &["No space left on device", "IOException"],
        seed: "diskFull",
        recommend: "nav.systemOptimization",
        action_kind: "route",
        action_target: "systemOptimization",
    },
    Blueprint {
        id: "gpu_driver",
        needles: &[
            "GL_INVALID",
            "OpenGL error",
            "OpenGL context",
            "dxgi",
            "WGL",
            "EGL_NOT_INITIALIZED",
            "Failed to create OpenGL",
            "Unable to initialize OpenGL",
        ],
        seed: "gpuDriver",
        recommend: "nav.gameOptimization",
        action_kind: "route",
        action_target: "gameOptimization",
    },
    Blueprint {
        id: "asset_corrupt",
        needles: &[
            "Invalid pack.mcmeta",
            "invalid pack.mcmeta",
            "zipfile.BadZipFile",
            "Bad CRC-32",
            "checksum mismatch",
            "DOWNLOAD_CHECKSUM_MISMATCH",
        ],
        seed: "assetCorrupt",
        recommend: "nav.resourceStudio",
        action_kind: "route",
        action_target: "resourceStudio",
    },
    Blueprint {
        id: "version_json_bad",
        needles: &["Failed to parse version", "Failed to load version", "MojangAPI", "version manifest"],
        seed: "versionBad",
        recommend: "nav.repair",
        action_kind: "route",
        action_target: "repair",
    },
];

/// Tìm blueprint match 1 dòng (case-insensitive, blueprint đầu tiên thắng — parity
/// vòng `for bp in _BLUEPRINTS: if search: return bp`).
pub fn find_blueprint(line: &str) -> Option<&'static Blueprint> {
    let lower = line.to_lowercase();
    BLUEPRINTS
        .iter()
        .find(|bp| bp.needles.iter().any(|n| lower.contains(&n.to_lowercase())))
}

pub(crate) fn severity(bp_id: &str) -> &'static str {
    // Parity `_severity` — trả đúng "error"/"warning" như legacy.
    match bp_id {
        "java_out_of_memory"
        | "java_class_version"
        | "java_missing"
        | "mod_conflict"
        | "disk_full"
        | "gpu_driver"
        | "asset_corrupt"
        | "version_json_bad" => "error",
        // auth_failed / connection_refused / connection_timeout → warning
        _ => "warning",
    }
}

fn severity_rank(bp_id: &str) -> u8 {
    if severity(bp_id) == "error" { 1 } else { 0 }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightLine {
    pub source: String,
    pub line: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Insight {
    pub id: &'static str,
    pub count: usize,
    pub severity: &'static str,
    pub seed: &'static str,
    pub recommend: &'static str,
    pub action: serde_json::Value,
    pub first_seen: Option<String>,
    pub last_seen: Option<String>,
    pub sources: Vec<String>,
    pub lines: Vec<InsightLine>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogAnalysis {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    pub sources: std::collections::BTreeMap<String, usize>,
    pub insights: Vec<Insight>,
    pub error_count: usize,
    pub warn_count: usize,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadResult {
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    pub lines: Vec<serde_json::Value>,
    pub truncated: bool,
}

/// Nguồn dòng log nội bộ — launcher inject qua snapshot, file đọc từ disk.
pub(crate) struct SourceLines {
    pub source: &'static str,
    /// (line_no, text) — line_no bắt đầu 1 như legacy enumerate(i+1).
    pub pairs: Vec<(usize, String)>,
}

/// `console.analyze` parity — quét nguồn (source=None = cả 3) → insights gộp.
pub fn analyze(
    paths: &DiagPaths,
    launcher_lines: Option<&[(usize, String)]>,
    instance_id: Option<&str>,
    source: Option<&str>,
) -> Result<LogAnalysis, DiagnosticsError> {
    let started = std::time::Instant::now();
    if let Some(s) = source {
        if !LOG_SOURCES.contains(&s) {
            return Err(DiagnosticsError::UnknownSource(s.to_string()));
        }
    }
    let mut collected: Vec<SourceLines> = Vec::new();
    for src in LOG_SOURCES {
        if let Some(filter) = source {
            if src != &filter {
                continue;
            }
        }
        collected.push(read_source(paths, launcher_lines, instance_id, src));
    }

    // Gộp insights — parity vòng for line_no, line in lines.
    let mut insights: std::collections::BTreeMap<&'static str, Insight> = Default::default();
    let mut totals: std::collections::BTreeMap<String, usize> = Default::default();
    for collected_src in &collected {
        totals.insert(
            collected_src.source.to_string(),
            collected_src.pairs.len(),
        );
        for (line_no, line) in &collected_src.pairs {
            let Some(bp) = find_blueprint(line) else { continue };
            let entry = insights.entry(bp.id).or_insert_with(|| Insight {
                id: bp.id,
                count: 0,
                severity: severity(bp.id),
                seed: bp.seed,
                recommend: bp.recommend,
                action: serde_json::json!({"kind": bp.action_kind, "target": bp.action_target}),
                first_seen: None,
                last_seen: None,
                sources: Vec::new(),
                lines: Vec::new(),
            });
            entry.count += 1;
            let src_name = collected_src.source.to_string();
            if !entry.sources.contains(&src_name) {
                entry.sources.push(src_name);
            }
            entry.sources.sort();
            let ts = extract_ts(line);
            if entry.first_seen.is_none() {
                entry.first_seen = ts.clone();
            }
            entry.last_seen = ts;
            if entry.lines.len() < 5 {
                entry.lines.push(InsightLine {
                    source: collected_src.source.to_string(),
                    line: *line_no,
                    // Parity line[:240] — cắt theo char boundary.
                    text: line.chars().take(240).collect(),
                });
            }
        }
    }

    let mut items: Vec<Insight> = insights.into_values().collect();
    // Parity sort: (-count, -severity_rank) — count giảm, error trước.
    items.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then(severity_rank(b.id).cmp(&severity_rank(a.id)))
    });
    let error_count = items.iter().filter(|i| i.severity == "error").map(|i| i.count).sum();
    let warn_count = items.iter().filter(|i| i.severity == "warning").map(|i| i.count).sum();
    Ok(LogAnalysis {
        instance_id: instance_id.map(str::to_string),
        sources: totals,
        insights: items,
        error_count,
        warn_count,
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

/// `console.read` parity — đọc 1 nguồn, limit + truncated, text cắt 400.
pub fn analyzer_read_source(
    paths: &DiagPaths,
    launcher_lines: Option<&[(usize, String)]>,
    instance_id: Option<&str>,
    source: &str,
    limit: usize,
) -> Result<ReadResult, DiagnosticsError> {
    if !LOG_SOURCES.contains(&source) {
        return Err(DiagnosticsError::UnknownSource(source.to_string()));
    }
    let collected = read_source(paths, launcher_lines, instance_id, source);
    let truncated = collected.pairs.len() > limit;
    let lines = collected
        .pairs
        .iter()
        .rev()
        .take(limit)
        .rev()
        .map(|(n, t)| {
            serde_json::json!({
                "n": n,
                // Parity t[:400] — char boundary.
                "text": t.chars().take(400).collect::<String>(),
            })
        })
        .collect();
    Ok(ReadResult {
        source: source.to_string(),
        instance_id: instance_id.map(str::to_string),
        lines,
        truncated,
    })
}

/// Đọc 1 nguồn: launcher = snapshot inject; minecraft = latest.log (5000 dòng cuối);
/// crash = 3 report mới nhất, mỗi file 600 dòng cuối (parity).
pub(crate) fn read_source(
    paths: &DiagPaths,
    launcher_lines: Option<&[(usize, String)]>,
    instance_id: Option<&str>,
    source: &str,
) -> SourceLines {
    match source {
        "launcher" => SourceLines {
            source: "launcher",
            pairs: launcher_lines.unwrap_or(&[]).to_vec(),
        },
        "minecraft" => {
            let path = minecraft_log(paths, instance_id);
            let pairs = path
                .filter(|p| p.is_file())
                .map(|p| read_file_pairs(&p, 5000))
                .unwrap_or_default();
            SourceLines {
                source: "minecraft",
                pairs,
            }
        }
        "crash" => {
            let mut pairs = Vec::new();
            for path in crash_reports(paths, instance_id) {
                pairs.extend(read_file_pairs(&path, 600));
            }
            SourceLines { source: "crash", pairs }
        }
        _ => SourceLines {
            source: "launcher",
            pairs: Vec::new(),
        },
    }
}

/// Đọc file → (line_no, text) của `limit` dòng cuối — parity `_read_file_pairs`
/// (line_no tính trên TOÀN BỘ file, không chỉ phần cắt).
pub(crate) fn read_file_pairs(path: &std::path::Path, limit: usize) -> Vec<(usize, String)> {
    let Ok(text) = std::fs::read_to_string(path) else {
        return Vec::new();
    };
    let total = text.lines().count();
    let start = total.saturating_sub(limit) + 1;
    text.lines()
        .skip(total.saturating_sub(limit))
        .enumerate()
        .map(|(i, s)| (start + i, s.to_string()))
        .collect()
}

/// Timestamp đầu dòng 'YYYY-MM-DD HH:MM:SS' (launcher fmt) nếu có.
fn extract_ts(line: &str) -> Option<String> {
    let bytes = line.as_bytes();
    if bytes.len() < 19 {
        return None;
    }
    let candidate = &line[..19];
    let is_ts = candidate.as_bytes()[4] == b'-'
        && candidate.as_bytes()[7] == b'-'
        && candidate.as_bytes()[10] == b' '
        && candidate.as_bytes()[13] == b':'
        && candidate.as_bytes()[16] == b':'
        && candidate
            .chars()
            .enumerate()
            .all(|(i, c)| matches!(i, 4 | 7 | 10 | 13 | 16) || c.is_ascii_digit());
    is_ts.then(|| candidate.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DiagPaths;

    fn temp_root(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir()
            .join(format!("antares-analyzer-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn paths(root: &std::path::Path) -> DiagPaths {
        DiagPaths {
            instances: root.join("instances"),
            data: root.join("data"),
            cache: root.join("cache"),
            config: root.join("app-data"),
        }
    }

    #[test]
    fn blueprint_count_and_match_order() {
        assert_eq!(BLUEPRINTS.len(), BLUEPRINT_COUNT);
        assert_eq!(BLUEPRINTS.len(), 11);
        // Blueprint đầu tiên thắng khi nhiều pattern khớp (parity vòng for).
        let bp = find_blueprint("java.lang.OutOfMemoryError happened").unwrap();
        assert_eq!(bp.id, "java_out_of_memory");
        // Case-insensitive như re.IGNORECASE
        let bp = find_blueprint("OUTOFMEMORYERROR: heap").unwrap();
        assert_eq!(bp.id, "java_out_of_memory");
        // Không match → None
        assert!(find_blueprint("all good here").is_none());
        // Multi-needle: auth
        assert_eq!(find_blueprint("Invalid session token").unwrap().id, "auth_failed");
        assert_eq!(find_blueprint("authserver.ely.by rejected").unwrap().id, "auth_failed");
    }

    #[test]
    fn severity_matrix_parity() {
        assert_eq!(severity("java_out_of_memory"), "error");
        assert_eq!(severity("java_class_version"), "error");
        assert_eq!(severity("java_missing"), "error");
        assert_eq!(severity("mod_conflict"), "error");
        assert_eq!(severity("disk_full"), "error");
        assert_eq!(severity("gpu_driver"), "error");
        assert_eq!(severity("asset_corrupt"), "error");
        assert_eq!(severity("version_json_bad"), "error");
        // warning group
        assert_eq!(severity("auth_failed"), "warning");
        assert_eq!(severity("connection_refused"), "warning");
        assert_eq!(severity("connection_timeout"), "warning");
    }

    #[test]
    fn analyze_groups_and_sorts() {
        let root = temp_root("group");
        let p = paths(&root);
        let launcher = vec![
            (1usize, "2026-09-26 12:00:00 [Render thread] OutOfMemoryError: heap".to_string()),
            (2, "2026-09-26 12:00:01 GL_INVALID_OPERATION".to_string()),
            (3, "2026-09-26 12:00:02 Connection refused: mc.example.com".to_string()),
            (4, "2026-09-26 12:00:03 OutOfMemoryError again".to_string()),
            (5, "nothing special".to_string()),
        ];
        let analysis = analyze(&p, Some(&launcher), Some("i1"), None).unwrap();
        assert_eq!(analysis.sources.get("launcher"), Some(&5));
        assert_eq!(analysis.error_count, 3, "2 oom + 1 gpu");
        assert_eq!(analysis.warn_count, 1);
        // sort: oom count 2 đứng trước gpu/conn (count 1)
        assert_eq!(analysis.insights[0].id, "java_out_of_memory");
        assert_eq!(analysis.insights[0].count, 2);
        assert_eq!(analysis.insights[0].first_seen.as_deref(), Some("2026-09-26 12:00:00"));
        assert_eq!(analysis.insights[0].last_seen.as_deref(), Some("2026-09-26 12:00:03"));
        // lines giữ tối đa 5, text cắt 240
        assert_eq!(analysis.insights[0].lines.len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn analyze_file_sources_and_source_filter() {
        let root = temp_root("files");
        let p = paths(&root);
        let log = p.instances.join("i1/game/logs/latest.log");
        std::fs::create_dir_all(log.parent().unwrap()).unwrap();
        std::fs::write(&log, "SocketTimeoutException: read timed out\nnormal line\n").unwrap();
        let cr = p.instances.join("i1/game/crash-reports");
        std::fs::create_dir_all(&cr).unwrap();
        std::fs::write(cr.join("crash-2026.txt"), "DuplicateModsFoundException: jei/nei\n").unwrap();

        // Mặc định cả 3 nguồn
        let full = analyze(&p, None, Some("i1"), None).unwrap();
        assert_eq!(full.sources.get("minecraft"), Some(&2));
        assert_eq!(full.sources.get("crash"), Some(&1));
        assert!(full.insights.iter().any(|i| i.id == "connection_timeout"));
        assert!(full.insights.iter().any(|i| i.id == "mod_conflict"));
        // mod_conflict sources = ["crash"]
        let mc = full.insights.iter().find(|i| i.id == "mod_conflict").unwrap();
        assert_eq!(mc.sources, vec!["crash".to_string()]);

        // source filter chỉ quét 1 nguồn
        let only_mc = analyze(&p, None, Some("i1"), Some("minecraft")).unwrap();
        assert!(!only_mc.insights.iter().any(|i| i.id == "mod_conflict"));
        assert_eq!(only_mc.sources.len(), 1);

        // read() — limit + truncated + cắt 400
        let read = analyzer_read_source(&p, None, Some("i1"), "minecraft", 1).unwrap();
        assert!(read.truncated);
        assert_eq!(read.lines.len(), 1);
        let read_all = analyzer_read_source(&p, None, Some("i1"), "minecraft", 5000).unwrap();
        assert!(!read_all.truncated);
        assert_eq!(read_all.lines.len(), 2);
        // source lạ → lỗi
        assert_eq!(
            analyzer_read_source(&p, None, None, "bogus", 10).unwrap_err().code(),
            "VALIDATION_FAILED"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn extract_ts_shape() {
        assert_eq!(
            extract_ts("2026-09-26 12:00:00 hello"),
            Some("2026-09-26 12:00:00".to_string())
        );
        assert_eq!(extract_ts("no timestamp"), None);
        assert_eq!(extract_ts("2026-09-26T12:00:00 iso"), None);
    }

    #[test]
    fn read_file_pairs_line_numbers_span_whole_file() {
        let root = temp_root("pairs");
        let file = root.join("big.log");
        let content: String = (0..100).map(|i| format!("line-{i}\n")).collect();
        std::fs::write(&file, &content).unwrap();
        let pairs = read_file_pairs(&file, 10);
        assert_eq!(pairs.len(), 10);
        assert_eq!(pairs[0].0, 91, "line_no tính trên toàn bộ file");
        assert_eq!(pairs[9].0, 100);
        assert_eq!(pairs[9].1, "line-99");
        let _ = std::fs::remove_dir_all(&root);
    }
}
