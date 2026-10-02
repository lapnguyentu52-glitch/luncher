//! Preflight — phần `LaunchPreflight` của orchestrator §111.
//!
//! Chạy các check fail-fast trước khi resolve/download; mỗi fail kèm remediation
//! để UI hiển thị hướng xử lý thay vì error thô.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PreflightSeverity {
    Info,
    Warning,
    Error,
}

/// Một preflight check kết quả.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightCheck {
    pub name: &'static str,
    pub severity: PreflightSeverity,
    pub ok: bool,
    pub detail: String,
    pub remediation: Option<String>,
}

impl PreflightCheck {
    fn pass(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            severity: PreflightSeverity::Info,
            ok: true,
            detail: detail.into(),
            remediation: None,
        }
    }

    fn fail(name: &'static str, detail: impl Into<String>, remediation: impl Into<String>) -> Self {
        Self {
            name,
            severity: PreflightSeverity::Error,
            ok: false,
            detail: detail.into(),
            remediation: Some(remediation.into()),
        }
    }

    fn warn(name: &'static str, detail: impl Into<String>, remediation: impl Into<String>) -> Self {
        Self {
            name,
            severity: PreflightSeverity::Warning,
            ok: true, // warning không chặn launch
            detail: detail.into(),
            remediation: Some(remediation.into()),
        }
    }
}

/// Report tổng hợp; `blocking` = số check Error.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreflightReport {
    pub checks: Vec<PreflightCheck>,
    pub blocking: usize,
}

impl PreflightReport {
    pub fn is_launchable(&self) -> bool {
        self.blocking == 0
    }
}

/// §111 — LaunchPreflight. Input là snapshot cấu hình đã resolve (không I/O ở skeleton).
pub struct LaunchPreflight;

impl LaunchPreflight {
    pub fn run(input: &PreflightInput) -> PreflightReport {
        let mut checks = Vec::new();

        // 1. Java path được cung cấp
        checks.push(if input.java_path.trim().is_empty() {
            PreflightCheck::fail(
                "java-present",
                "no java runtime resolved",
                "install a managed Java runtime in Settings → Java",
            )
        } else {
            PreflightCheck::pass("java-present", input.java_path.clone())
        });

        // 2. Java major đủ cho MC version (capabilities từ antares-java).
        // Không có path → version check cũng fail (2 blocking như parity).
        checks.push(if input.java_path.trim().is_empty() {
            PreflightCheck::fail(
                "java-version",
                "no java runtime to verify",
                "install a managed Java runtime in Settings → Java",
            )
        } else if input.java_major >= input.required_java_major {
            PreflightCheck::pass(
                "java-version",
                format!(
                    "java {} >= required {}",
                    input.java_major, input.required_java_major
                ),
            )
        } else {
            PreflightCheck::fail(
                "java-version",
                format!(
                    "java {} < required {} for minecraft {}",
                    input.java_major, input.required_java_major, input.mc_version
                ),
                format!("install Java {} or newer", input.required_java_major),
            )
        });

        // 3. Heap trong khoảng hợp lý (không quá 1.5GB dưới / trên 32GB)
        checks.push(if input.heap_max_mb < 512 {
            PreflightCheck::fail(
                "jvm-heap",
                format!("heap {}MB too small", input.heap_max_mb),
                "raise JVM max heap to at least 512MB",
            )
        } else if input.heap_max_mb > 32 * 1024 {
            PreflightCheck::warn(
                "jvm-heap",
                format!("heap {}MB unusually large", input.heap_max_mb),
                "lower JVM max heap below 32GB to avoid GC pauses",
            )
        } else {
            PreflightCheck::pass("jvm-heap", format!("heap {}MB", input.heap_max_mb))
        });

        // 4. Game directory chưa bị chiếm bởi session khác
        checks.push(if input.game_directory_locked {
            PreflightCheck::fail(
                "game-dir-free",
                format!("game dir already in use: {}", input.game_directory),
                "close the other launcher session using this instance",
            )
        } else {
            PreflightCheck::pass("game-dir-free", input.game_directory.clone())
        });

        // 5. Loader tuỳ chọn khai báo
        match input.loader.as_deref() {
            None => checks.push(PreflightCheck::pass("loader", "vanilla")),
            Some(loader) if loader.is_empty() => {
                checks.push(PreflightCheck::pass("loader", "vanilla"));
            }
            Some(loader) => checks.push(PreflightCheck::pass("loader", loader)),
        }

        let blocking = checks.iter().filter(|c| !c.ok).count();
        PreflightReport { checks, blocking }
    }
}

/// Input snapshot cho preflight — nền tảng để sau này lấy từ LaunchPlan + JavaResolver.
pub struct PreflightInput {
    pub mc_version: String,
    pub loader: Option<String>,
    pub java_path: String,
    pub java_major: u16,
    pub required_java_major: u16,
    pub heap_max_mb: u32,
    pub game_directory: String,
    pub game_directory_locked: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input() -> PreflightInput {
        PreflightInput {
            mc_version: "1.21.4".into(),
            loader: None,
            java_path: "/jvm/java-21/bin/java".into(),
            java_major: 21,
            required_java_major: 17,
            heap_max_mb: 4096,
            game_directory: "/game/inst-1".into(),
            game_directory_locked: false,
        }
    }

    fn by_name<'a>(report: &'a PreflightReport, name: &str) -> &'a PreflightCheck {
        report
            .checks
            .iter()
            .find(|c| c.name == name)
            .unwrap_or_else(|| panic!("missing check {name}"))
    }

    #[test]
    fn all_green_is_launchable() {
        let report = LaunchPreflight::run(&input());
        assert!(report.is_launchable());
        assert_eq!(report.blocking, 0);
        assert_eq!(report.checks.len(), 5);
        assert!(by_name(&report, "java-present").ok);
        assert!(by_name(&report, "java-version").ok);
        assert!(by_name(&report, "loader").ok);
    }

    #[test]
    fn java_too_old_blocks_with_remediation() {
        let mut input = input();
        input.java_major = 8;
        let report = LaunchPreflight::run(&input);
        assert!(!report.is_launchable());
        let check = by_name(&report, "java-version");
        assert!(!check.ok);
        assert_eq!(check.severity, PreflightSeverity::Error);
        assert!(check.remediation.as_deref().unwrap().contains("17"));
    }

    #[test]
    fn missing_java_blocks() {
        let mut input = input();
        input.java_path = "  ".into();
        let report = LaunchPreflight::run(&input);
        assert_eq!(report.blocking, 2); // java-present + java-version đều fail
        assert!(!by_name(&report, "java-present").ok);
    }

    #[test]
    fn heap_bounds() {
        let mut input = input();
        input.heap_max_mb = 256;
        assert!(!LaunchPreflight::run(&input).is_launchable());

        input.heap_max_mb = 64 * 1024;
        let report = LaunchPreflight::run(&input);
        assert!(report.is_launchable()); // warn không chặn
        assert_eq!(by_name(&report, "jvm-heap").severity, PreflightSeverity::Warning);
    }

    #[test]
    fn locked_game_dir_blocks() {
        let mut input = input();
        input.game_directory_locked = true;
        let report = LaunchPreflight::run(&input);
        assert!(!report.is_launchable());
        assert!(!by_name(&report, "game-dir-free").ok);
    }

    #[test]
    fn loader_empty_string_is_vanilla() {
        let mut input = input();
        input.loader = Some(String::new());
        let report = LaunchPreflight::run(&input);
        assert_eq!(by_name(&report, "loader").detail, "vanilla");
    }
}
