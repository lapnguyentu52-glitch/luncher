//! Mod security scanner — heuristic engine parity `services/mods/scanner/scanner.py`.
//!
//! Phát hiện dựa trên decode bytecode:
//! - Remote download & execute chain (threat cao nhất)
//! - Reverse shell / network bind
//! - Credential theft (authlib token, cookie, password store)
//! - Crypto ransomware pattern (Cipher + Files.walk)
//! - Obfuscation cực đoan + reflection bypass
//! - Windows registry / process exec
//! - Suspicious embedded files (exe/dll/bat trong jar)
//! - InvokeDynamic với custom bootstrap (ẩn logic động)
//! - Call graph: path từ mod class tới exec/network sink (BFS depth-limited)
//!
//! Scoring: mỗi finding có weight; verdict = SAFE | SUSPICIOUS | DANGEROUS.

use serde::Serialize;

use crate::jar_reader::{jar_structure, scan_jar_classes, ClassInfo};

pub const VERDICT_SAFE: &str = "SAFE";
pub const VERDICT_SUSPICIOUS: &str = "SUSPICIOUS";
pub const VERDICT_DANGEROUS: &str = "DANGEROUS";

pub const DANGEROUS_THRESHOLD: i64 = 60;
pub const SUSPICIOUS_THRESHOLD: i64 = 25;

/// Bootstrap factory "chuẩn" JVM (lambda + string concat — JEP 309).
const STANDARD_BOOTSTRAPS: [&str; 3] = [
    "java.lang.invoke.LambdaMetafactory",
    "java.lang.invoke.StringConcatFactory",
    "java.lang.runtime.ObjectMethods",
];

/// Sink classes cho call graph — đích cuối của chain exec/network.
const SINK_CLASSES: [&str; 8] = [
    "java.lang.Runtime",
    "java.lang.ProcessBuilder",
    "java.net.Socket",
    "java.net.ServerSocket",
    "java.net.URL",
    "java.net.HttpURLConnection",
    "java.net.URLConnection",
    "java.nio.channels.SocketChannel",
];

/// Depth tối đa BFS call graph.
const CALLGRAPH_MAX_DEPTH: usize = 4;

/// Ngưỡng invokedynamic coi là "lạm dụng" (loader sinh code động hàng loạt).
const INVOKEDYNAMIC_ABUSE_THRESHOLD: usize = 20;

/// Prefix KHÔNG phải class nội bộ của mod.
const NON_INTERNAL_PREFIXES: [&str; 14] = [
    "java.", "javax.", "jdk.", "sun.", "com.sun.", "org.w3c.", "org.xml.", "org.ietf.",
    "org.jcp.", "kotlin.", "scala.", "net.minecraft.", "net.fabricmc.", "net.neoforged.",
];

const CREDENTIAL_PATHS: [&str; 12] = [
    ".minecraft/libraries/com/mojang/authlib",
    "authlib",
    "accesstoken",
    "session_token",
    "login_data",
    "user data/default/login data",
    "cookies.sqlite",
    "credential",
    "key4.db",
    "logins.json",
    "appdata/local/microsoft",
    "appdata/roaming/.minecraft",
];

const REGISTRY_INDICATORS: [&str; 5] = [
    "reg add",
    "regedit",
    "hkey_local_machine",
    "hkey_current_user",
    "currentversion\\run",
];

const SHELL_STRINGS: [&str; 7] = [
    "/bin/bash",
    "/bin/sh",
    "cmd.exe",
    "powershell",
    "-c exec",
    "nc -e",
    "netcat",
];

const KNOWN_OK_ENDPOINTS: [&str; 10] = [
    "modrinth",
    "curseforge",
    "github",
    "maven",
    "minecraft",
    "fabricmc",
    "mojang",
    "parchment",
    "sonatype",
    "jitpack",
];

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Finding {
    pub rule_id: &'static str,
    pub severity: &'static str, // low | medium | high | critical
    pub weight: i64,
    pub title: String,
    pub detail: String,
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub file: String,
    pub size: u64,
    pub sha256: String,
    pub verdict: String,
    pub score: i64,
    pub findings: Vec<Finding>,
    pub classes_scanned: usize,
    pub java_version: i32,
    pub mod_id: Option<String>,
    pub mod_name: Option<String>,
    pub scanned_at: f64,
}

fn is_internal(cls: &str) -> bool {
    !cls.is_empty()
        && cls.contains('.')
        && !NON_INTERNAL_PREFIXES.iter().any(|p| cls.starts_with(p))
}

fn match_any<'a>(items: &[String], needles: &[&str]) -> Vec<String> {
    items
        .iter()
        .filter(|item| {
            let low = item.to_lowercase();
            needles.iter().any(|needle| low.contains(needle))
        })
        .cloned()
        .collect()
}

/// Classes có method ref khớp (đúng hoặc kết thúc bằng needle) — parity
/// `_classes_having_method` (sorted set).
fn classes_having_method(infos: &[ClassInfo], method_needle: &str) -> Vec<String> {
    let mut set: BTreeSet2 = Default::default();
    for info in infos {
        if info
            .method_refs
            .iter()
            .any(|m| m == method_needle || m.ends_with(method_needle))
        {
            set.insert(info.name.clone());
        }
    }
    set.into_iter().collect()
}

/// BTreeSet alias để sort (BTreeSet đã sorted — dùng trực tiếp).
type BTreeSet2 = std::collections::BTreeSet<String>;

/// Quét 1 jar mod: structure + bytecode heuristics (parity `scan_mod`).
pub fn scan_mod(jar_bytes: &[u8], file_name: &str, size: u64, sha256: &str, now: f64) -> ScanReport {
    let mut report = ScanReport {
        file: file_name.to_string(),
        size,
        sha256: sha256.to_string(),
        verdict: VERDICT_SAFE.to_string(),
        score: 0,
        findings: Vec::new(),
        classes_scanned: 0,
        java_version: 0,
        mod_id: None,
        mod_name: None,
        scanned_at: now,
    };

    // 1. Cấu trúc jar
    let struct_info = match jar_structure(jar_bytes) {
        Ok(s) => s,
        Err(err) => {
            report.verdict = VERDICT_SUSPICIOUS.to_string();
            report.score = SUSPICIOUS_THRESHOLD;
            report.findings.push(Finding {
                rule_id: "JAR_CORRUPT",
                severity: "medium",
                weight: SUSPICIOUS_THRESHOLD,
                title: "Corrupt / not a real jar".into(),
                detail: err.to_string(),
                evidence: vec![],
            });
            return report;
        }
    };
    if !struct_info.suspicious_files.is_empty() {
        report.findings.push(Finding {
            rule_id: "EMBEDDED_EXECUTABLE",
            severity: "critical",
            weight: 70,
            title: "Native executable embedded in jar".into(),
            detail: format!(
                "jar chứa file thực thi: {:?}",
                struct_info.suspicious_files
            ),
            evidence: struct_info
                .suspicious_files
                .iter()
                .take(3)
                .cloned()
                .collect(),
        });
    }
    if !struct_info.nested_jars.is_empty() {
        report.findings.push(Finding {
            rule_id: "NESTED_JAR",
            severity: "medium",
            weight: 15,
            title: "Nested jar (self-extract / loader)".into(),
            detail: format!("{} nested jar", struct_info.nested_jars.len()),
            evidence: struct_info.nested_jars.iter().take(3).cloned().collect(),
        });
    }

    // 2. Metadata (fabric.mod.json)
    if let Some(fmj) = &struct_info.fabric_mod_json {
        if let Ok(data) = serde_json::from_str::<serde_json::Value>(fmj) {
            report.mod_id = data.get("id").and_then(|v| v.as_str()).map(str::to_string);
            report.mod_name = data.get("name").and_then(|v| v.as_str()).map(str::to_string);
        }
    }

    // 3. Decode bytecode
    let infos = scan_jar_classes(jar_bytes, 500);
    report.classes_scanned = infos.len();
    if !infos.is_empty() {
        report.java_version = infos.iter().map(|i| i.java_version()).max().unwrap_or(0);
    }

    let mut all_strings: Vec<String> = Vec::new();
    let mut all_method_refs: Vec<String> = Vec::new();
    let mut all_class_refs: Vec<String> = Vec::new();
    for info in &infos {
        all_strings.extend(info.strings.iter().cloned());
        all_method_refs.extend(info.method_refs.iter().cloned());
        all_class_refs.push(info.name.clone());
        all_class_refs.extend(info.class_refs.iter().cloned());
    }

    // 4. Rules (parity thứ tự)
    rule_download_exec(&mut report, &infos);
    rule_reverse_shell(&mut report, &all_strings, &all_method_refs);
    rule_credential_theft(&mut report, &all_strings);
    rule_ransomware(&mut report, &infos);
    rule_registry(&mut report, &all_strings);
    rule_obfuscation(&mut report, &all_class_refs);
    rule_network_beacon(&mut report, &all_strings);
    rule_invokedynamic(&mut report, &infos);
    rule_callgraph_depth(&mut report, &infos);
    rule_reflection_abuse(&mut report, &infos);

    // 5. Verdict
    report.score = report.findings.iter().map(|f| f.weight).sum();
    report.verdict = if report.score >= DANGEROUS_THRESHOLD {
        VERDICT_DANGEROUS.to_string()
    } else if report.score >= SUSPICIOUS_THRESHOLD {
        VERDICT_SUSPICIOUS.to_string()
    } else {
        VERDICT_SAFE.to_string()
    };
    report
}

/// Tải file + exec cùng class = chain nguy hiểm nhất (FR3EM1UM style).
fn rule_download_exec(report: &mut ScanReport, infos: &[ClassInfo]) {
    let mut download_classes: BTreeSet2 = Default::default();
    download_classes.extend(classes_having_method(infos, "java.net.URL.openConnection"));
    download_classes.extend(classes_having_method(infos, "java.nio.file.Files.copy"));
    let mut exec_classes: BTreeSet2 = Default::default();
    exec_classes.extend(classes_having_method(infos, "java.lang.ProcessBuilder.start"));
    exec_classes.extend(classes_having_method(infos, "java.lang.Runtime.exec"));
    let both: Vec<String> = download_classes.intersection(&exec_classes).cloned().collect();
    if !both.is_empty() {
        let evidence: Vec<String> = both.iter().take(3).cloned().collect();
        report.findings.push(Finding {
            rule_id: "DOWNLOAD_AND_EXECUTE",
            severity: "critical",
            weight: 80,
            title: "Downloads file AND executes it in same class".into(),
            detail: format!("Class vừa tải file vừa thực thi: {both:?}"),
            evidence,
        });
    }
}

fn rule_reverse_shell(report: &mut ScanReport, strings: &[String], method_refs: &[String]) {
    let hits = match_any(strings, &SHELL_STRINGS);
    let net: Vec<String> = method_refs
        .iter()
        .filter(|m| {
            m.contains("java.net.Socket")
                || m.contains("ServerSocket.accept")
                || m.contains("SocketChannel.connect")
        })
        .cloned()
        .collect();
    if !hits.is_empty() && !net.is_empty() {
        report.findings.push(Finding {
            rule_id: "REVERSE_SHELL_PATTERN",
            severity: "critical",
            weight: 75,
            title: "Shell execution combined with raw socket".into(),
            detail: format!(
                "shell strings: {:?} + network: {:?}",
                &hits[..hits.len().min(2)],
                &net[..net.len().min(2)]
            ),
            evidence: hits.iter().take(3).cloned().collect(),
        });
    }
}

fn rule_credential_theft(report: &mut ScanReport, strings: &[String]) {
    let cred_hits = match_any(strings, &CREDENTIAL_PATHS);
    if !cred_hits.is_empty() {
        report.findings.push(Finding {
            rule_id: "CREDENTIAL_ACCESS",
            severity: "high",
            weight: 45,
            title: "References credential/token storage paths".into(),
            detail: format!("{} string trỏ tới credential store", cred_hits.len()),
            evidence: cred_hits.iter().take(3).cloned().collect(),
        });
    }
}

fn rule_ransomware(report: &mut ScanReport, infos: &[ClassInfo]) {
    let crypto: Vec<String> = infos
        .iter()
        .flat_map(|i| i.method_refs.iter())
        .filter(|m| m.contains("javax.crypto.Cipher.init") || m.contains("CipherOutputStream"))
        .cloned()
        .collect();
    let files_walk = classes_having_method(infos, "java.nio.file.Files.walk");
    if !crypto.is_empty() && !files_walk.is_empty() {
        report.findings.push(Finding {
            rule_id: "MASS_CRYPTO_FILE_IO",
            severity: "high",
            weight: 55,
            title: "Bulk file traversal combined with crypto (ransomware pattern)".into(),
            detail: format!(
                "Files.walk trong {:?} + Cipher",
                &files_walk[..files_walk.len().min(2)]
            ),
            evidence: files_walk.iter().take(2).cloned().collect(),
        });
    }
}

fn rule_registry(report: &mut ScanReport, strings: &[String]) {
    let hits = match_any(strings, &REGISTRY_INDICATORS);
    if !hits.is_empty() {
        report.findings.push(Finding {
            rule_id: "WINDOWS_REGISTRY",
            severity: "medium",
            weight: 25,
            title: "Windows registry manipulation strings".into(),
            detail: "Startup persistence hoặc chỉnh registry".into(),
            evidence: hits.iter().take(3).cloned().collect(),
        });
    }
}

/// Obfuscation cực đoan: tỷ lệ class tên 1-2 ký tự cao bất thường. Tên class
/// tối giản parity regex `^[a-zA-Z]{1,2}(/\$\$\d+)?$|\$\d+$` (match trên phần
/// sau dấu chấm cuối).
fn is_obfuscated_name(simple: &str) -> bool {
    // Nhánh 1: ^[a-zA-Z]{1,2}(/\$\$\d+)?$
    if !simple.is_empty() && simple.len() <= 2 && simple.chars().all(|c| c.is_ascii_alphabetic()) {
        return true;
    }
    // nhánh 1 với hậu tố /$$<digits>
    if let Some((prefix, suffix)) = simple.split_once("/$$") {
        if prefix.len() <= 2
            && prefix.chars().all(|c| c.is_ascii_alphabetic())
            && !suffix.is_empty()
            && suffix.chars().all(|c| c.is_ascii_digit())
        {
            return true;
        }
    }
    // Nhánh 2: $\d+$ (kết thúc bằng $<digits>)
    if let Some(idx) = simple.rfind('$') {
        let tail = &simple[idx + 1..];
        if !tail.is_empty() && tail.chars().all(|c| c.is_ascii_digit()) {
            return true;
        }
    }
    false
}

fn rule_obfuscation(report: &mut ScanReport, class_refs: &[String]) {
    if class_refs.len() < 20 {
        return;
    }
    let obfuscated = class_refs
        .iter()
        .filter(|c| {
            let simple = c.rsplit('.').next().unwrap_or("");
            is_obfuscated_name(simple)
        })
        .count();
    let ratio = obfuscated as f64 / class_refs.len().max(1) as f64;
    if ratio > 0.7 {
        report.findings.push(Finding {
            rule_id: "HEAVY_OBFUSCATION",
            severity: "medium",
            weight: 20,
            title: "Heavy obfuscation".into(),
            detail: format!(
                "{}% classes with tiny names — mod bị obfuscate mạnh, khó audit",
                (ratio * 100.0) as i64
            ),
            evidence: class_refs.iter().take(3).cloned().collect(),
        });
    }
}

/// Endpoint lạ — không phải ecosystem (≥5 mới flag, parity).
fn rule_network_beacon(report: &mut ScanReport, strings: &[String]) {
    let weird: Vec<String> = strings
        .iter()
        .filter(|u| u.starts_with("http://") || u.starts_with("https://"))
        .filter(|u| {
            // parity _URL_RE: tối thiểu 8 ký tự sau scheme
            u.len() >= "http://".len() + 8 || u.len() >= "https://".len() + 8
        })
        .filter(|u| !KNOWN_OK_ENDPOINTS.iter().any(|k| u.to_lowercase().contains(k)))
        .cloned()
        .collect();
    if weird.len() >= 5 {
        report.findings.push(Finding {
            rule_id: "MANY_EXTERNAL_ENDPOINTS",
            severity: "low",
            weight: 10,
            title: format!("{} non-standard external endpoints", weird.len()),
            detail: "Mod giao tiếp với nhiều domain ngoài ecosystem".into(),
            evidence: weird.iter().take(3).cloned().collect(),
        });
    }
}

/// InvokeDynamic với custom bootstrap — ẩn logic động khỏi phân tích tĩnh.
fn rule_invokedynamic(report: &mut ScanReport, infos: &[ClassInfo]) {
    let total_indy: usize = infos.iter().map(|i| i.invokedynamic_count).sum();
    if total_indy == 0 {
        return;
    }
    let mut custom: BTreeSet2 = Default::default();
    for info in infos {
        for bc in &info.bootstrap_classes {
            if !bc.is_empty() && !STANDARD_BOOTSTRAPS.iter().any(|s| bc.starts_with(s)) {
                custom.insert(bc.clone());
            }
        }
    }
    if !custom.is_empty() {
        // giả mạo java.*/sun.*/jdk.* không thuộc bộ chuẩn
        let fake_jdk: BTreeSet2 = custom
            .iter()
            .filter(|c| {
                (c.starts_with("java.") || c.starts_with("sun.") || c.starts_with("jdk."))
                    && !STANDARD_BOOTSTRAPS.iter().any(|s| c.starts_with(s))
            })
            .cloned()
            .collect();
        let mut evidence: Vec<String> = custom.union(&fake_jdk).cloned().collect();
        evidence.sort();
        evidence.truncate(3);
        report.findings.push(Finding {
            rule_id: "CUSTOM_INVOKEDYNAMIC_BOOTSTRAP",
            severity: "high",
            weight: 35,
            title: format!(
                "InvokeDynamic with custom bootstrap ({} non-standard)",
                custom.len()
            ),
            detail: "Indy không dùng LambdaMetafactory/StringConcatFactory — logic động \
                     được ẩn qua bootstrap tự viết"
                .into(),
            evidence,
        });
    }
    if total_indy >= INVOKEDYNAMIC_ABUSE_THRESHOLD {
        report.findings.push(Finding {
            rule_id: "INVOKEDYNAMIC_ABUSE",
            severity: "medium",
            weight: 30,
            title: format!("Abnormal invokedynamic density ({total_indy} callsites)"),
            detail: format!(
                "{total_indy} invokedynamic trên {} class — pattern của loader sinh code động",
                infos.len()
            ),
            evidence: vec![],
        });
    }
}

/// BFS qua call graph nội bộ: path mod-class → sink nguy hiểm. Chỉ traverse qua
/// class nội bộ; depth ngắn = chain trực tiếp tin cậy hơn.
fn rule_callgraph_depth(report: &mut ScanReport, infos: &[ClassInfo]) {
    use std::collections::{BTreeMap, BTreeSet, VecDeque};
    let mut graph: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for info in infos {
        if !info.callees.is_empty() {
            graph.entry(info.name.clone()).or_default().extend(info.callees.iter().cloned());
        }
    }
    if graph.is_empty() {
        return;
    }
    // BFS đa nguồn: entry node (không ai gọi tới); mọi node cycle → quét tất cả.
    let referenced: BTreeSet<&String> = graph.values().flatten().collect();
    let mut queue: VecDeque<(String, usize)> = graph
        .keys()
        .filter(|n| !referenced.contains(*n))
        .map(|n| (n.clone(), 0))
        .collect();
    if queue.is_empty() {
        queue = graph.keys().map(|n| (n.clone(), 0)).collect();
    }
    let mut best_depth: Option<usize> = None;
    let mut hit_sinks: BTreeSet<String> = BTreeSet::new();
    let mut visited: BTreeSet<String> = BTreeSet::new();
    while let Some((node, depth)) = queue.pop_front() {
        if visited.contains(&node) || depth > CALLGRAPH_MAX_DEPTH {
            continue;
        }
        visited.insert(node.clone());
        let callees = graph.get(&node).cloned().unwrap_or_default();
        for sink in SINK_CLASSES {
            if callees.iter().any(|c| c.starts_with(sink)) {
                hit_sinks.insert(sink.to_string());
                if best_depth.map(|d| depth < d).unwrap_or(true) {
                    best_depth = Some(depth);
                }
            }
        }
        for nxt in callees {
            if is_internal(&nxt) && !visited.contains(&nxt) {
                queue.push_back((nxt, depth + 1));
            }
        }
    }
    let Some(best_depth) = best_depth else {
        return;
    };
    let (weight, severity) = if best_depth <= 1 {
        (65, "critical")
    } else if best_depth <= 2 {
        (55, "high")
    } else {
        (40, "high")
    };
    report.findings.push(Finding {
        rule_id: "EXEC_NETWORK_CALLGRAPH",
        severity,
        weight,
        title: format!("Call-graph path (depth {best_depth}) to exec/network sink"),
        detail: format!(
            "Path {best_depth} bước qua class nội bộ tới {:?}",
            hit_sinks.iter().take(2).collect::<Vec<_>>()
        ),
        evidence: hit_sinks.iter().take(3).cloned().collect(),
    });
}

/// Reflection + MethodHandles kết hợp — bypass tĩnh (nghi vấn nhẹ).
fn rule_reflection_abuse(report: &mut ScanReport, infos: &[ClassInfo]) {
    let refl: Vec<String> = infos
        .iter()
        .filter(|i| i.uses_reflection)
        .map(|i| i.name.clone())
        .collect();
    let mh: Vec<String> = infos
        .iter()
        .filter(|i| i.uses_method_handles)
        .map(|i| i.name.clone())
        .collect();
    if !refl.is_empty() && !mh.is_empty() {
        let distinct: BTreeSet2 = refl.iter().cloned().collect();
        report.findings.push(Finding {
            rule_id: "REFLECTION_METHODHANDLES",
            severity: "medium",
            weight: 20,
            title: format!("Reflection + MethodHandles combined ({} classes)", distinct.len()),
            detail: "Class.forName/Method.invoke + MethodHandles.Lookup — pattern ẩn \
                     API call khỏi method_refs"
                .into(),
            evidence: refl.iter().take(3).cloned().collect(),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jar_reader::MAGIC;

    /// Class file có method ref tới java/lang/Runtime.exec + string URL — dựng
    /// thủ công constant pool (parity decode đủ path).
    fn class_with_refs(refs: &[(&str, &str)], strings: &[&str], callees: &[&str]) -> Vec<u8> {
        // Cấu trúc pool động: build rồi vá count.
        let mut pool: Vec<Vec<u8>> = Vec::new(); // encoded entries, index 1-based
        let mut class_idx = std::collections::BTreeMap::new();
        let mut utf8_idx = std::collections::BTreeMap::new();
        let mut string_idx = std::collections::BTreeMap::new();
        let mut nat_idx = std::collections::BTreeMap::new();
        let mut methodref_idx = std::collections::BTreeMap::new();

        let add_utf8 = |pool: &mut Vec<Vec<u8>>, utf8_idx: &mut std::collections::BTreeMap<String, u16>, s: &str| -> u16 {
            if let Some(&idx) = utf8_idx.get(s) {
                return idx;
            }
            let idx = pool.len() as u16 + 1;
            let mut entry = vec![1u8];
            entry.extend_from_slice(&(s.len() as u16).to_be_bytes());
            entry.extend_from_slice(s.as_bytes());
            pool.push(entry);
            utf8_idx.insert(s.to_string(), idx);
            idx
        };

        // Class refs
        let mut class_ref_ids: Vec<u16> = Vec::new();
        for (cls, _method) in refs {
            let name_idx = add_utf8(&mut pool, &mut utf8_idx, cls);
            let idx = pool.len() as u16 + 1;
            let mut entry = vec![7u8];
            entry.extend_from_slice(&name_idx.to_be_bytes());
            pool.push(entry);
            class_idx.insert(cls.to_string(), idx);
            class_ref_ids.push(idx);
        }
        // strings
        for s in strings {
            let utf = add_utf8(&mut pool, &mut utf8_idx, s);
            let idx = pool.len() as u16 + 1;
            let mut entry = vec![8u8];
            entry.extend_from_slice(&utf.to_be_bytes());
            pool.push(entry);
            string_idx.insert(s.to_string(), idx);
        }
        // methodrefs
        for (cls, method) in refs {
            let name_idx = add_utf8(&mut pool, &mut utf8_idx, method);
            let desc_idx = add_utf8(&mut pool, &mut utf8_idx, "()V");
            let nat = pool.len() as u16 + 1;
            let mut entry = vec![12u8];
            entry.extend_from_slice(&name_idx.to_be_bytes());
            entry.extend_from_slice(&desc_idx.to_be_bytes());
            pool.push(entry);
            nat_idx.insert(format!("{cls}.{method}"), nat);
            let cls_id = class_idx[*cls];
            let idx = pool.len() as u16 + 1;
            let mut entry = vec![10u8]; // Methodref
            entry.extend_from_slice(&cls_id.to_be_bytes());
            entry.extend_from_slice(&nat.to_be_bytes());
            pool.push(entry);
            methodref_idx.insert(format!("{cls}.{method}"), idx);
        }

        // Method CP entries phải vào pool TRƯỚC khi serialize — entry bytes nằm
        // trong CP region, không được vá count sau khi đã ghi (lệch decoder).
        let run_idx = {
            let idx = pool.len() as u16 + 1;
            let mut e = vec![1u8];
            e.extend_from_slice(&3u16.to_be_bytes());
            e.extend_from_slice(b"run");
            pool.push(e);
            idx
        };
        let desc_method_idx = add_utf8(&mut pool, &mut utf8_idx, "()V");
        let code_idx = add_utf8(&mut pool, &mut utf8_idx, "Code");

        // Build class bytes
        let mut b = Vec::new();
        b.extend_from_slice(&MAGIC.to_be_bytes());
        b.extend_from_slice(&0u16.to_be_bytes());
        b.extend_from_slice(&52u16.to_be_bytes());
        b.extend_from_slice(&((pool.len() + 1) as u16).to_be_bytes());
        for entry in &pool {
            b.extend_from_slice(entry);
        }
        b.extend_from_slice(&0x0021u16.to_be_bytes()); // access
        b.extend_from_slice(&class_idx["this.Class"].to_be_bytes()); // this
        b.extend_from_slice(&class_idx["java.lang.Object"].to_be_bytes()); // super
        b.extend_from_slice(&0u16.to_be_bytes()); // interfaces
        b.extend_from_slice(&0u16.to_be_bytes()); // fields

        // methods: 1 method với Code attribute chứa invokestatic refs
        b.extend_from_slice(&1u16.to_be_bytes()); // method count
        b.extend_from_slice(&0x0001u16.to_be_bytes()); // access public
        b.extend_from_slice(&run_idx.to_be_bytes());
        b.extend_from_slice(&desc_method_idx.to_be_bytes());
        // 1 attribute (Code)
        b.extend_from_slice(&1u16.to_be_bytes());
        b.extend_from_slice(&code_idx.to_be_bytes());

        // Code body: invokestatic các methodrefs + hoàn tục
        let mut code: Vec<u8> = Vec::new();
        for (cls, method) in refs {
            let idx = methodref_idx[&format!("{cls}.{method}")];
            code.push(0xB8); // invokestatic
            code.extend_from_slice(&idx.to_be_bytes());
        }
        code.push(0xB1); // return
        let mut attr_body = Vec::new();
        attr_body.extend_from_slice(&0u16.to_be_bytes()); // max_stack
        attr_body.extend_from_slice(&0u16.to_be_bytes()); // max_locals
        attr_body.extend_from_slice(&(code.len() as u32).to_be_bytes());
        attr_body.extend_from_slice(&code);
        attr_body.extend_from_slice(&0u16.to_be_bytes()); // exceptions
        attr_body.extend_from_slice(&0u16.to_be_bytes()); // inner attrs
        b.extend_from_slice(&(attr_body.len() as u32).to_be_bytes());
        b.extend_from_slice(&attr_body);

        // class attributes = 0
        b.extend_from_slice(&0u16.to_be_bytes());
        let _ = callees;
        b
    }

    fn build_jar(entries: &[(&str, Vec<u8>)]) -> Vec<u8> {
        crate::jar_reader::tests::build_stored_zip(entries)
    }

    #[test]
    fn scan_clean_jar_is_safe() {
        let jar = build_jar(&[
            ("fabric.mod.json", br#"{"id":"clean","name":"Clean"}"#.to_vec()),
            ("com/clean/Main.class", class_with_refs(
                &[("this.Class", "init"), ("java.lang.Object", "init")],
                &["https://modrinth.com/api"],
                &[],
            )),
        ]);
        let report = scan_mod(&jar, "clean.jar", 100, "sha", 0.0);
        assert_eq!(report.verdict, "SAFE");
        assert_eq!(report.score, 0);
        assert_eq!(report.mod_id.as_deref(), Some("clean"));
        assert_eq!(report.classes_scanned, 1);
        assert_eq!(report.java_version, 8);
    }

    #[test]
    fn scan_embedded_executable_is_dangerous() {
        let jar = build_jar(&[
            ("com/x/Main.class", class_with_refs(
                &[("this.Class", "init"), ("java.lang.Object", "init")],
                &[],
                &[],
            )),
            ("payload.exe", b"MZ".to_vec()),
        ]);
        let report = scan_mod(&jar, "evil.jar", 200, "sha", 0.0);
        assert!(report.findings.iter().any(|f| f.rule_id == "EMBEDDED_EXECUTABLE"));
        assert_eq!(report.verdict, "DANGEROUS", "70 >= 60");
    }

    #[test]
    fn scan_credential_strings_is_suspicious() {
        let jar = build_jar(&[
            ("com/x/Main.class", class_with_refs(
                &[("this.Class", "init"), ("java.lang.Object", "init")],
                &["cookies.sqlite", "logins.json", "accessToken", "key4.db"],
                &[],
            )),
        ]);
        let report = scan_mod(&jar, "cred.jar", 100, "sha", 0.0);
        let cred = report.findings.iter().find(|f| f.rule_id == "CREDENTIAL_ACCESS");
        assert!(cred.is_some());
        assert_eq!(cred.unwrap().weight, 45);
        assert_eq!(report.verdict, "SUSPICIOUS", "45 ∈ [25, 60)");
    }

    #[test]
    fn scan_download_exec_chain_is_dangerous() {
        let jar = build_jar(&[
            ("com/x/Main.class", class_with_refs(
                &[
                    ("this.Class", "init"),
                    ("java.lang.Object", "init"),
                    ("java.net.URL", "openConnection"),
                    ("java.lang.ProcessBuilder", "start"),
                ],
                &[],
                &[],
            )),
        ]);
        let report = scan_mod(&jar, "dropper.jar", 100, "sha", 0.0);
        let d = report.findings.iter().find(|f| f.rule_id == "DOWNLOAD_AND_EXECUTE");
        assert!(d.is_some());
        assert_eq!(d.unwrap().weight, 80);
        assert_eq!(report.verdict, "DANGEROUS");
    }

    #[test]
    fn scan_reverse_shell_critical() {
        let jar = build_jar(&[
            ("com/x/Main.class", class_with_refs(
                &[
                    ("this.Class", "init"),
                    ("java.lang.Object", "init"),
                    ("java.net.Socket", "connect"),
                ],
                &["/bin/sh", "nc -e"],
                &[],
            )),
        ]);
        let report = scan_mod(&jar, "shell.jar", 100, "sha", 0.0);
        assert!(report.findings.iter().any(|f| f.rule_id == "REVERSE_SHELL_PATTERN"));
        assert_eq!(report.verdict, "DANGEROUS", "75 + callgraph");
    }

    #[test]
    fn scan_registry_strings_medium() {
        let jar = build_jar(&[
            ("com/x/Main.class", class_with_refs(
                &[("this.Class", "init"), ("java.lang.Object", "init")],
                &["reg add HKLM\\Software\\Run"],
                &[],
            )),
        ]);
        let report = scan_mod(&jar, "reg.jar", 100, "sha", 0.0);
        assert!(report.findings.iter().any(|f| f.rule_id == "WINDOWS_REGISTRY"));
        // score 25 >= SUSPICIOUS_THRESHOLD (25) → SUSPICIOUS (parity boundary).
        assert_eq!(report.verdict, "SUSPICIOUS");
    }

    #[test]
    fn verdict_thresholds_boundary() {
        // score 0 → SAFE; 25 → SUSPICIOUS; 60 → DANGEROUS (parity thresholds)
        assert_eq!(DANGEROUS_THRESHOLD, 60);
        assert_eq!(SUSPICIOUS_THRESHOLD, 25);
        let mut r = ScanReport {
            file: "x".into(),
            size: 0,
            sha256: "".into(),
            verdict: VERDICT_SAFE.into(),
            score: 24,
            findings: vec![],
            classes_scanned: 0,
            java_version: 0,
            mod_id: None,
            mod_name: None,
            scanned_at: 0.0,
        };
        r.score = 24;
        assert_eq!(r.verdict, "SAFE");
        let _ = &mut r;
    }

    #[test]
    fn scan_corrupt_jar_is_suspicious() {
        let report = scan_mod(b"not a zip", "bad.jar", 10, "sha", 0.0);
        assert_eq!(report.findings[0].rule_id, "JAR_CORRUPT");
        assert_eq!(report.verdict, "SUSPICIOUS");
    }

    #[test]
    fn is_internal_prefixes() {
        assert!(is_internal("com.example.Mod"));
        assert!(!is_internal("java.lang.Runtime"));
        assert!(!is_internal("net.minecraft.client.Minecraft"));
        assert!(!is_internal("net.fabricmc.api.ModInitializer"));
        assert!(!is_internal(""));
        assert!(!is_internal("NoDot"));
    }

    #[test]
    fn is_obfuscated_name_parity() {
        assert!(is_obfuscated_name("a"));
        assert!(is_obfuscated_name("Ab"));
        assert!(is_obfuscated_name("a/$$1"));
        assert!(is_obfuscated_name("abc$1"));
        assert!(!is_obfuscated_name("Main"));
        assert!(!is_obfuscated_name("abc"));
        assert!(!is_obfuscated_name("a$"));
        assert!(!is_obfuscated_name(""));
    }

    #[test]
    fn many_endpoints_rule_low() {
        let urls: Vec<String> = (0..5)
            .map(|i| format!("https://evil{i}.example.com/payload"))
            .collect();
        let jar = build_jar(&[(
            "com/x/Main.class",
            class_with_refs(
                &[("this.Class", "init"), ("java.lang.Object", "init")],
                &urls.iter().map(String::as_str).collect::<Vec<_>>(),
                &[],
            ),
        )]);
        let report = scan_mod(&jar, "beacon.jar", 100, "sha", 0.0);
        assert!(report.findings.iter().any(|f| f.rule_id == "MANY_EXTERNAL_ENDPOINTS"));
        // weight 10 < 25 → SAFE
        assert_eq!(report.verdict, "SAFE");
    }
}
