//! Phase 4 — `ExitAnalyzer` (§116 Crash Analyzer 2.0) + `CompanionPairing` (§111).
//!
//! ## ExitAnalyzer §116
//!
//! ```text
//! ingest → normalize → fingerprint → classify → rank evidence → recommendation
//! ```
//!
//! Input: log text (latest.log), crash report, JVM/launcher stderr — gộp thành 1
//! corpus text. KHÔNG LLM, không tự bịa insight (mục 24) — chỉ match pattern đã
//! biết; **không kết luận nếu evidence yếu** (§116).
//!
//! ## CompanionPairing
//!
//! Parity `RuntimeService.write_pairing_for_instance` (services/runtime/service.py):
//! ghi `companion.json` vào `<instances>/<id>/game/` — `version: 1`, endpoint
//! (url+token+packetTypes), `writtenAt` epoch seconds. Server chưa start (url None)
//! → `Ok(None)` (không lỗi — parity trả None).

use std::path::{Path, PathBuf};

use serde::Serialize;

// ---------------------------------------------------------------------------
// ExitAnalyzer §116
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExitVerdict {
    Completed,
    Crash,
    OomKilled,
    JavaMissing,
    ModConflict,
    UserCancelled,
    /// Evidence yếu — không kết luận (§116).
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum EvidenceSeverity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub kind: &'static str,
    pub severity: EvidenceSeverity,
    pub weight: u32,
    pub sample: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitAnalysis {
    pub exit_code: i32,
    pub verdict: ExitVerdict,
    pub fingerprint: String,
    pub confidence: f64,
    pub evidence: Vec<Evidence>,
    pub recommendation: Option<String>,
}

/// Pattern classify — parity tinh thần LogAnalyzer (§41: không LLM). Weight quyết
/// định rank evidence; verdict = pattern có tổng weight cao nhất, confidence = share.
const PATTERNS: &[(&str, EvidenceSeverity, u32, &[&str], &str)] = &[
    (
        "oom",
        EvidenceSeverity::Error,
        40,
        &["java.lang.OutOfMemoryError"],
        "raise JVM max heap (-Xmx) or reduce render distance",
    ),
    (
        "mod-conflict",
        EvidenceSeverity::Error,
        30,
        &["DuplicateModsFoundException", "Mod resolution exception", "Incompatible mod set"],
        "remove or update the conflicting mods listed in the log",
    ),
    (
        "java-missing",
        EvidenceSeverity::Error,
        30,
        &["UnsupportedClassVersionError", "has been compiled by a more recent version"],
        "select a newer Java runtime for this instance",
    ),
    (
        "user-cancel",
        EvidenceSeverity::Warning,
        20,
        &["exit code -15", "terminated by signal 15", "SIGTERM"],
        "none — process was terminated intentionally",
    ),
];

const LOW_EVIDENCE_WEIGHT: u32 = 20;

pub struct ExitAnalyzer;

impl ExitAnalyzer {
    /// Phân tích exit: `exit_code` + corpus text (log/crash/stderr đã gộp).
    pub fn analyze(exit_code: i32, corpus: &str) -> ExitAnalysis {
        // 1-2. ingest + normalize (lowercase copy cho match, giữ gốc cho sample)
        let normalized = corpus.to_lowercase();
        // 3. fingerprint — hash ổn định theo exit_code + corpus (FNV-1a 64, hex 16)
        let fingerprint = fingerprint(&normalized);

        // 4. classify + 5. rank evidence
        let mut evidence: Vec<Evidence> = Vec::new();
        for (kind, severity, weight, needles, _rec) in PATTERNS {
            let Some(sample) = needles
                .iter()
                .find_map(|needle| find_line(&normalized, corpus, needle))
            else {
                continue;
            };
            evidence.push(Evidence {
                kind,
                severity: severity.clone(),
                weight: *weight,
                sample,
            });
        }

        // 6. verdict + recommendation — chỉ khi evidence đủ mạnh (§116).
        let best = evidence.iter().max_by_key(|e| e.weight);
        let verdict = match best {
            _ if exit_code == 0 && evidence.is_empty() => ExitVerdict::Completed,
            Some(e) if e.weight >= LOW_EVIDENCE_WEIGHT => Self::verdict_for(e.kind, exit_code),
            _ => {
                if exit_code == 0 {
                    ExitVerdict::Completed
                } else {
                    ExitVerdict::Unknown
                }
            }
        };
        let confidence = match (verdict, best) {
            (ExitVerdict::Completed, _) => 1.0,
            (_, Some(e)) if e.weight >= LOW_EVIDENCE_WEIGHT => {
                let total: u32 = evidence.iter().map(|x| x.weight).sum();
                (e.weight as f64 / total.max(1) as f64 * 10.0).round() / 10.0
            }
            _ => 0.0,
        };
        let recommendation = match (verdict, best) {
            (ExitVerdict::Completed, _) => None,
            (_, Some(e)) if e.weight >= LOW_EVIDENCE_WEIGHT => PATTERNS
                .iter()
                .find(|(kind, _, _, _, _)| *kind == e.kind)
                .map(|(_, _, _, _, rec)| rec.to_string()),
            _ => None,
        };

        ExitAnalysis {
            exit_code,
            verdict,
            fingerprint,
            confidence,
            evidence,
            recommendation,
        }
    }

    fn verdict_for(kind: &str, exit_code: i32) -> ExitVerdict {
        match kind {
            "oom" => ExitVerdict::OomKilled,
            "mod-conflict" => ExitVerdict::ModConflict,
            "java-missing" => ExitVerdict::JavaMissing,
            "user-cancel" => ExitVerdict::UserCancelled,
            _ if exit_code == 0 => ExitVerdict::Completed,
            _ => ExitVerdict::Unknown,
        }
    }
}

/// Tìm needle trong normalized, trả dòng tương ứng từ corpus gốc (cắt 160 ký tự —
/// parity `_short_err`). Byte-offset an toàn Unicode: lowercase có thể đổi độ dài
/// UTF-8, nên slice theo char-boundary tăng dần thay vì offset cứng.
fn find_line(normalized: &str, corpus: &str, needle: &str) -> Option<String> {
    // normalized đã lowercase — needle cũng phải lowercase mới match được
    // (PATTERNS giữ nguyên case để hiển thị).
    let needle_lc = needle.to_lowercase();
    let byte_idx = normalized.find(&needle_lc)?;
    // Map byte-offset của normalized về char-index, rồi dùng char-index trên corpus.
    let char_idx = normalized[..byte_idx].chars().count();
    let corpus_chars: Vec<(usize, char)> = corpus.char_indices().collect();
    let upto = corpus_chars
        .get(char_idx + needle_lc.chars().count())
        .map(|(i, _)| *i);
    let slice = match upto {
        Some(end) => &corpus[..end],
        None => corpus,
    };
    let mut line = slice.rsplit('\n').next().unwrap_or("").trim().to_string();
    if line.is_empty() {
        line = needle.to_string();
    }
    // Cắt theo char boundary (không byte-truncate giữa chừng ký tự Unicode).
    if line.chars().count() > 160 {
        line = line.chars().take(160).collect();
    }
    Some(line)
}

/// FNV-1a 64-bit → hex 16 ký tự. Ổn định giữa các lần chạy (không hash random).
fn fingerprint(data: &str) -> String {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in data.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("{hash:016x}")
}

// ---------------------------------------------------------------------------
// CompanionPairing — parity write_pairing_for_instance
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompanionEndpoint {
    pub url: String,
    pub token: String,
    pub packet_types: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PairingError {
    #[error("io: {0}")]
    Io(String),
}

impl PairingError {
    pub fn code(&self) -> &'static str {
        "PAIRING_WRITE_FAILED"
    }
}

pub struct CompanionPairing;

impl CompanionPairing {
    /// Ghi `companion.json` vào `game_dir`. Server chưa chạy → `Ok(None)`
    /// (parity `if not self._server.url: return None`). Trả path file đã ghi.
    pub fn write_pairing(
        endpoint: Option<&CompanionEndpoint>,
        game_dir: &Path,
        now_secs: f64,
    ) -> Result<Option<PathBuf>, PairingError> {
        let Some(endpoint) = endpoint else {
            return Ok(None);
        };
        std::fs::create_dir_all(game_dir)
            .map_err(|err| PairingError::Io(err.to_string()))?;
        let file = game_dir.join("companion.json");
        let payload = serde_json::json!({
            "version": 1,
            "endpoint": endpoint.url,
            "token": endpoint.token,
            "packetTypes": endpoint.packet_types,
            "writtenAt": now_secs,
        });
        // atomic write: tmp + rename (mục 62 — file lỗi không để lại rác).
        let tmp = game_dir.join("companion.json.tmp");
        std::fs::write(&tmp, payload.to_string())
            .map_err(|err| PairingError::Io(err.to_string()))?;
        std::fs::rename(&tmp, &file).map_err(|err| PairingError::Io(err.to_string()))?;
        Ok(Some(file))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exit_zero_with_empty_log_is_completed() {
        let analysis = ExitAnalyzer::analyze(0, "");
        assert_eq!(analysis.verdict, ExitVerdict::Completed);
        assert_eq!(analysis.confidence, 1.0);
        assert!(analysis.recommendation.is_none());
    }

    #[test]
    fn oom_detected_with_recommendation() {
        let log = "Exception in thread \"main\" java.lang.OutOfMemoryError: Java heap space\n\
                   at net.minecraft.client.main.Main.main(Main.java)";
        let analysis = ExitAnalyzer::analyze(1, log);
        assert_eq!(analysis.verdict, ExitVerdict::OomKilled);
        assert!(analysis.confidence >= 0.5);
        assert_eq!(
            analysis.recommendation.as_deref(),
            Some("raise JVM max heap (-Xmx) or reduce render distance")
        );
        let oom = analysis.evidence.iter().find(|e| e.kind == "oom").unwrap();
        assert_eq!(oom.severity, EvidenceSeverity::Error);
        assert!(oom.sample.contains("OutOfMemoryError"));
    }

    #[test]
    fn weak_evidence_is_unknown_not_guessed() {
        // exit != 0, log không match pattern nào → Unknown, confidence 0 (§116).
        let analysis = ExitAnalyzer::analyze(3, "something weird happened");
        assert_eq!(analysis.verdict, ExitVerdict::Unknown);
        assert_eq!(analysis.confidence, 0.0);
        assert!(analysis.recommendation.is_none());
    }

    #[test]
    fn java_missing_and_mod_conflict_classified() {
        let analysis = ExitAnalyzer::analyze(
            1,
            "java.lang.UnsupportedClassVersionError: net/minecraft/client/main/Main \
             has been compiled by a more recent version",
        );
        assert_eq!(analysis.verdict, ExitVerdict::JavaMissing);

        let analysis = ExitAnalyzer::analyze(1, "DuplicateModsFoundException: jei vs nei");
        assert_eq!(analysis.verdict, ExitVerdict::ModConflict);
    }

    #[test]
    fn strongest_pattern_wins() {
        // Log chứa cả OOM và cancel — OOM weight 40 > 20 → OomKilled.
        let analysis = ExitAnalyzer::analyze(
            -15,
            "terminated by signal 15\njava.lang.OutOfMemoryError: metaspace",
        );
        assert_eq!(analysis.verdict, ExitVerdict::OomKilled);
        assert_eq!(analysis.evidence.len(), 2);
    }

    #[test]
    fn fingerprint_stable_and_short() {
        let a = fingerprint("latest.log content");
        let b = fingerprint("latest.log content");
        let c = fingerprint("other content");
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(a.len(), 16);
        assert!(a.chars().all(|ch| ch.is_ascii_hexdigit()));
    }

    #[test]
    fn sample_line_truncated_at_160() {
        let long = format!("java.lang.OutOfMemoryError: {}", "x".repeat(300));
        let analysis = ExitAnalyzer::analyze(1, &long);
        let oom = analysis.evidence.iter().find(|e| e.kind == "oom").unwrap();
        assert!(oom.sample.len() <= 160);
    }

    #[test]
    fn pairing_none_when_server_off() {
        let dir = std::env::temp_dir().join(format!("antares-pair-off-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let result = CompanionPairing::write_pairing(None, &dir, 123.0).unwrap();
        assert!(result.is_none());
        assert!(!dir.join("companion.json").exists()); // không tạo dir thừa
    }

    #[test]
    fn pairing_writes_companion_json_parity_shape() {
        let dir = std::env::temp_dir().join(format!("antares-pair-on-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let endpoint = CompanionEndpoint {
            url: "http://127.0.0.1:51337/packet".into(),
            token: "tok".repeat(12),
            packet_types: vec!["runtime.performance".into(), "runtime.game_state".into()],
        };
        let file = CompanionPairing::write_pairing(Some(&endpoint), &dir, 1700.0)
            .unwrap()
            .expect("pairing written");
        assert!(file.ends_with("companion.json"));

        let data: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(data["version"], 1);
        assert_eq!(data["endpoint"], endpoint.url);
        assert_eq!(data["token"], endpoint.token);
        assert_eq!(data["packetTypes"], serde_json::json!(endpoint.packet_types));
        assert_eq!(data["writtenAt"], 1700.0);

        // Không só tmp
        assert!(!dir.join("companion.json.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
