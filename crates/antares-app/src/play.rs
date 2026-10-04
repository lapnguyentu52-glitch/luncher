//! Batch 07a — group **play**, method `play.preflight` (§8/§30) parity
//! `sidecar.handle_play_preflight` — kiểm tra trước launch, KHÔNG spawn process.
//!
//! Contract giữ nguyên (PARITY evidence #3 — TS `PreflightResult` không sửa):
//!
//! ```json
//! { "instanceId", "checks": [{ "id", "label", "status", "detail" }],
//!   "blockers", "warnings", "canPlay" }
//! ```
//!
//! 5 check theo đúng thứ tự legacy: `java` → `version` → `account` → `disk`
//! → `mods`. `play.launch` (Batch 07b — orchestrator native) vẫn đi bridge.

use std::path::Path;

use antares_java::JavaInfo;
use serde::Serialize;

use crate::disk;
use crate::instances::Instance;

/// Parity `_check_disk_space(min_mb=512)` — dưới ngưỡng là blocker (error).
const MIN_FREE_MB: u64 = 512;

/// Parity TS `PreflightStatus` = 'pass' | 'warning' | 'error'.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PreflightStatus {
    Pass,
    Warning,
    Error,
}

/// Một dòng check — parity sidecar `{id, label, status, detail}`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightCheck {
    pub id: &'static str,
    pub label: String,
    pub status: PreflightStatus,
    pub detail: String,
}

/// Report — parity TS `PreflightResult`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightReport {
    pub instance_id: String,
    pub checks: Vec<PreflightCheck>,
    pub blockers: usize,
    pub warnings: usize,
    pub can_play: bool,
}

/// Parity sidecar `_required_java_major` — **chỉ** dùng cho preflight
/// heuristic (legacy over-strict: mọi 1.20.x → 21, snapshot → 21; khác rule
/// §108 của `antares-launch::required_java_major` dùng cho launch planning —
/// không trộn 2 rule với nhau).
fn required_java_major(mc_version: &str) -> u16 {
    // Python: `parts = [int(p) for p in split(".")[:2]]` — parse fail (snapshot
    // "24w14a") → ValueError → 21; thiếu phần thứ 2 → minor = 0.
    let parsed: Option<Vec<i64>> = mc_version
        .split('.')
        .take(2)
        .map(|part| part.parse::<i64>().ok())
        .collect();
    let Some(parts) = parsed else {
        return 21;
    };
    let minor = parts.get(1).copied().unwrap_or(0);
    if minor >= 20 {
        21
    } else if minor >= 18 {
        17
    } else if minor >= 17 {
        16
    } else {
        8
    }
}

/// Parity `_check_disk_space` + branch `except` của legacy:
/// - Ok(mb) → pass nếu ≥ 512, detail `"{mb} MB free"`
/// - Err → warning (không chặn) + message — parity sidecar nuốt exception
///   thành warning để UI vẫn render được các check còn lại.
fn disk_check(free: std::io::Result<u64>) -> PreflightCheck {
    match free {
        Ok(mb) => PreflightCheck {
            id: "disk",
            label: "Disk".into(),
            status: if mb >= MIN_FREE_MB {
                PreflightStatus::Pass
            } else {
                PreflightStatus::Error
            },
            detail: format!("{mb} MB free"),
        },
        Err(err) => PreflightCheck {
            id: "disk",
            label: "Disk".into(),
            status: PreflightStatus::Warning,
            detail: err.to_string(),
        },
    }
}

/// Đếm `*.jar` top-level trong `<instance>/game/mods` — parity
/// `len([p for p in mods_dir.glob("*.jar")])`.
fn count_mods(instances_dir: &Path, instance_id: &str) -> usize {
    let mods_dir = instances_dir.join(instance_id).join("game").join("mods");
    let Ok(entries) = std::fs::read_dir(mods_dir) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "jar"))
        .count()
}

/// Core preflight — thuần (không scan java, không đọc settings) để test
/// deterministic; caller (`AppServices::play_preflight`) nạp javas/account.
///
/// `instances_dir` = scope Instances root — legacy `ctx.paths.instances`
/// (dùng cho check `disk` + đếm mods).
pub fn preflight(
    instance: &Instance,
    account: Option<&serde_json::Value>,
    javas: &[JavaInfo],
    instances_dir: &Path,
) -> PreflightReport {
    let required = required_java_major(&instance.minecraft_version);

    // 1. Java — parity: `ok = any(j["major"] >= required)`.
    let java_ok = javas.iter().any(|j| j.major >= required);
    let java = PreflightCheck {
        id: "java",
        label: format!("Java ≥ {required}"),
        status: if java_ok {
            PreflightStatus::Pass
        } else {
            PreflightStatus::Error
        },
        detail: if javas.is_empty() {
            "no Java found".into()
        } else {
            format!("found {} runtime(s)", javas.len())
        },
    };

    // 2. Version — luôn pass (presence đã verify khi load instance).
    let version = PreflightCheck {
        id: "version",
        label: "Version".into(),
        status: PreflightStatus::Pass,
        detail: format!("{} / {}", instance.minecraft_version, instance.loader),
    };

    // 3. Account — thiếu là WARNING không chặn (§8 "Play Anyway").
    let account_check = match account {
        Some(value) => {
            let display = value
                .get("displayName")
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| value.get("id").and_then(|v| v.as_str()).unwrap_or(""));
            PreflightCheck {
                id: "account",
                label: "Account".into(),
                status: PreflightStatus::Pass,
                detail: display.to_string(),
            }
        }
        None => PreflightCheck {
            id: "account",
            label: "Account".into(),
            status: PreflightStatus::Warning,
            detail: "no account selected".into(),
        },
    };

    // 4. Disk — parity shutil.disk_usage trên instances root.
    let disk = disk_check(disk::free_mb(instances_dir));

    // 5. Mods — luôn pass, chỉ đếm.
    let mods = PreflightCheck {
        id: "mods",
        label: "Mods".into(),
        status: PreflightStatus::Pass,
        detail: format!("{} installed", count_mods(instances_dir, &instance.id)),
    };

    let checks = vec![java, version, account_check, disk, mods];
    let blockers = checks
        .iter()
        .filter(|c| c.status == PreflightStatus::Error)
        .count();
    let warnings = checks
        .iter()
        .filter(|c| c.status == PreflightStatus::Warning)
        .count();

    PreflightReport {
        instance_id: instance.id.clone(),
        can_play: blockers == 0,
        checks,
        blockers,
        warnings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::instances::InstanceMemory;

    fn instance(version: &str, loader: &str) -> Instance {
        Instance {
            id: "abc123".into(),
            name: "Test".into(),
            minecraft_version: version.into(),
            loader: loader.into(),
            directory: "/tmp/abc123".into(),
            memory: InstanceMemory {
                min_mb: 512,
                max_mb: 2048,
            },
            jvm_args: Vec::new(),
            jvm_preset: "auto".into(),
            created_at: Some("abc123".into()),
            last_played_at: None,
            launch_count: 0,
        }
    }

    fn java(major: u16) -> JavaInfo {
        JavaInfo {
            path: "/j".into(),
            exe: "/j/bin/java".into(),
            javaw: None,
            major,
            name: "jdk".into(),
        }
    }

    fn account() -> serde_json::Value {
        serde_json::json!({ "id": "a1", "displayName": "Steve" })
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("antares-play-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn required_java_major_matches_sidecar() {
        // mirror tests/unit/legacy/test_sidecar_flows.py::TestRequiredJavaMajor
        assert_eq!(required_java_major("1.8.9"), 8);
        assert_eq!(required_java_major("1.17"), 16);
        assert_eq!(required_java_major("1.18.2"), 17);
        assert_eq!(required_java_major("1.20.1"), 21);
        assert_eq!(required_java_major("1.21.11"), 21);
        assert_eq!(required_java_major("24w14a"), 21); // snapshot → newest
    }

    #[test]
    fn all_pass_with_java_and_account() {
        let dir = temp_dir("pass");
        let report = preflight(
            &instance("1.21.11", "fabric"),
            Some(&account()),
            &[java(21)],
            &dir,
        );

        assert!(report.can_play);
        assert_eq!(report.blockers, 0);
        assert_eq!(report.instance_id, "abc123");
        // parity: đủ 5 id theo đúng thứ tự legacy
        let ids: Vec<&str> = report.checks.iter().map(|c| c.id).collect();
        assert_eq!(ids, vec!["java", "version", "account", "disk", "mods"]);

        let java = &report.checks[0];
        assert_eq!(java.label, "Java ≥ 21");
        assert_eq!(java.status, PreflightStatus::Pass);
        assert_eq!(java.detail, "found 1 runtime(s)");
        assert_eq!(report.checks[1].detail, "1.21.11 / fabric");
        assert_eq!(report.checks[2].detail, "Steve");
        assert_eq!(report.checks[2].status, PreflightStatus::Pass);
        assert!(
            report.checks[3].detail.ends_with(" MB free"),
            "{}",
            report.checks[3].detail
        );
        assert_eq!(report.checks[4].detail, "0 installed");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_java_is_blocker() {
        let dir = temp_dir("nojava");
        let report = preflight(&instance("1.21.11", "fabric"), None, &[], &dir);

        let java = report.checks.iter().find(|c| c.id == "java").unwrap();
        assert_eq!(java.status, PreflightStatus::Error);
        assert_eq!(java.detail, "no Java found");
        assert!(!report.can_play);
        assert!(report.blockers >= 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn java_below_required_is_blocker_even_when_found() {
        let dir = temp_dir("oldjava");
        let report = preflight(&instance("1.21.11", "fabric"), None, &[java(17)], &dir);
        let java = report.checks.iter().find(|c| c.id == "java").unwrap();
        assert_eq!(java.status, PreflightStatus::Error);
        assert_eq!(java.detail, "found 1 runtime(s)");
        assert!(!report.can_play);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn no_account_is_warning_not_blocker() {
        let dir = temp_dir("noaccount");
        let report = preflight(&instance("1.21.11", "fabric"), None, &[java(21)], &dir);

        let account = report.checks.iter().find(|c| c.id == "account").unwrap();
        assert_eq!(account.status, PreflightStatus::Warning);
        assert_eq!(account.detail, "no account selected");
        assert_eq!(report.warnings, 1);
        assert!(report.can_play, "warning không chặn (§8 Play Anyway)");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn mods_counts_top_level_jars_only() {
        let dir = temp_dir("mods");
        let mods = dir.join("abc123/game/mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(mods.join("a.jar"), b"x").unwrap();
        std::fs::write(mods.join("b.jar"), b"x").unwrap();
        std::fs::write(mods.join("readme.txt"), b"x").unwrap();
        std::fs::create_dir_all(mods.join("nested.jar")).unwrap();

        let report = preflight(
            &instance("1.21.11", "vanilla"),
            Some(&account()),
            &[java(21)],
            &dir,
        );
        let mods_check = report.checks.iter().find(|c| c.id == "mods").unwrap();
        // parity `glob("*.jar")`: đếm mọi entry khớp pattern, kể cả dir cùng tên
        // (legacy cũng vậy — giữ nguyên hành vi, không tự suy diễn file/dir).
        assert_eq!(mods_check.detail, "3 installed");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn disk_check_threshold_and_error_paths() {
        // ≥512 → pass; <512 → error (blocker); missing path → warning.
        let ok = disk_check(Ok(600));
        assert_eq!(ok.status, PreflightStatus::Pass);
        assert_eq!(ok.detail, "600 MB free");

        let low = disk_check(Ok(100));
        assert_eq!(low.status, PreflightStatus::Error);
        assert_eq!(low.detail, "100 MB free");

        let missing = disk_check(Err(std::io::Error::from(
            std::io::ErrorKind::NotFound,
        )));
        assert_eq!(missing.status, PreflightStatus::Warning);
    }
}
