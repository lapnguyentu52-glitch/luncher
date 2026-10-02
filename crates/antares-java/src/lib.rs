//! antares-java — Batch 14, mục **Java manager** (§110 Java Runtime Model).
//!
//! Phase 1 — thuần logic:
//! - `JavaRuntime` — model §110 (source/path/major/minor/architecture/vendor/verified/capabilities)
//! - `JavaSource` — resolution order §110: explicit → profile → managed → mojang → system
//! - `resolve_java` — chọn runtime theo thứ tự ưu tiên
//! - `parse_java_version` — đọc major version từ output `java -version` (parity discovery.py)
//!
//! Phase 3 — `discovery`: scan known dirs + JAVA_HOME + PATH, `detect_major` qua
//! `java -showversion` có timeout, `java_info`/`scan_java_infos` (parity discovery.py).

pub mod discovery;

pub use discovery::{
    detect_major, java_executable, java_info, javaw_executable, known_java_dirs,
    scan_directory, scan_java_infos, scan_system_java, JavaInfo,
};

use serde::{Deserialize, Serialize};

/// §110 — Nguồn của một Java runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JavaSource {
    /// Instance explicit override.
    InstanceExplicit,
    /// Profile explicit override.
    ProfileExplicit,
    /// Runtime do launcher quản lý.
    Managed,
    /// Runtime Mojang tải kèm.
    Mojang,
    /// Java hệ thống.
    System,
}

/// §110 — JavaRuntime.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaRuntime {
    pub source: JavaSource,
    pub path: String,
    pub major: u16,
    pub minor: u8,
    pub architecture: String,
    pub vendor: String,
    pub verified: bool,
    pub capabilities: Vec<String>,
}

impl JavaRuntime {
    pub fn system(path: impl Into<String>, major: u16) -> Self {
        Self {
            source: JavaSource::System,
            path: path.into(),
            major,
            minor: 0,
            architecture: std::env::consts::ARCH.to_string(),
            vendor: "unknown".into(),
            verified: false,
            capabilities: Vec::new(),
        }
    }

    pub fn supports_game(&self, required_major: u16) -> bool {
        self.major >= required_major
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JavaError {
    #[error("no suitable java runtime found (required major {0})")]
    NotFound(u16),
    #[error("java runtime too old: have {have}, need {need}")]
    TooOld { have: u16, need: u16 },
    #[error("unparsable java version output: {0}")]
    UnparsableVersion(String),
}

impl JavaError {
    pub fn code(&self) -> &'static str {
        match self {
            JavaError::NotFound(_) => "JAVA_NOT_FOUND",
            JavaError::TooOld { .. } => "JAVA_TOO_OLD",
            JavaError::UnparsableVersion(_) => "JAVA_VERSION_UNPARSABLE",
        }
    }
}

/// Resolution order §110:
/// instance explicit → profile explicit → managed → mojang runtime → system Java.
///
/// Mỗi slot là `Option<JavaRuntime>`; slot trước có và đủ major thì thắng ngay.
/// Nếu slot nào tồn tại nhưng quá cũ → tiếp tục thử slot sau (fallback), không fail sớm.
pub fn resolve_java(
    required_major: u16,
    instance_explicit: Option<JavaRuntime>,
    profile_explicit: Option<JavaRuntime>,
    managed: Option<JavaRuntime>,
    mojang: Option<JavaRuntime>,
    system: Option<JavaRuntime>,
) -> Result<JavaRuntime, JavaError> {
    for (source_name, candidate) in [
        ("instance", instance_explicit),
        ("profile", profile_explicit),
        ("managed", managed),
        ("mojang", mojang),
        ("system", system),
    ] {
        if let Some(runtime) = candidate {
            if runtime.supports_game(required_major) {
                return Ok(runtime);
            }
            log::warn!(
                "java slot {source_name} too old (have {}, need {required_major}), falling back",
                runtime.major
            );
        }
    }
    Err(JavaError::NotFound(required_major))
}

/// Parse output `java -version` thành (major, minor).
///
/// Các dạng cần nhận biết:
/// - `java version "17.0.2" 2022-01-18` → (17, 0)
/// - `java version "1.8.0_392"`         → (8, 0)  — legacy 1.x mapping
/// - `openjdk version "21" 2023-09-19`  → (21, 0)
/// - `java version "21.0.1"` (raw)      → (21, 1) — minor là thành phần thứ hai
///
/// Parity `services/java/discovery.py`.
pub fn parse_java_version(output: &str) -> Result<(u16, u8), JavaError> {
    let marker = "version \"";
    let start = output
        .find(marker)
        .ok_or_else(|| JavaError::UnparsableVersion(output.trim().to_string()))?
        + marker.len();
    let rest = &output[start..];
    let end = rest
        .find('"')
        .ok_or_else(|| JavaError::UnparsableVersion(output.trim().to_string()))?;
    let version = &rest[..end];

    let mut parts = version.split('.');
    let first: u16 = parts
        .next()
        .and_then(|p| p.parse().ok())
        .ok_or_else(|| JavaError::UnparsableVersion(version.to_string()))?;

    // Legacy mapping: 1.8.0_x → 8; 1.x → x
    let (major, minor) = if first == 1 {
        let second: u16 = parts
            .next()
            .and_then(|p| p.parse().ok())
            .ok_or_else(|| JavaError::UnparsableVersion(version.to_string()))?;
        (second, 0)
    } else {
        let minor: u8 = parts
            .next()
            .and_then(|p| p.parse().ok())
            .unwrap_or(0);
        (first, minor)
    };
    Ok((major, minor))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime(source: JavaSource, major: u16) -> JavaRuntime {
        JavaRuntime {
            source,
            path: format!("/java/{source:?}-{major}"),
            major,
            minor: 0,
            architecture: "x86_64".into(),
            vendor: "vendor".into(),
            verified: true,
            capabilities: vec![],
        }
    }

    #[test]
    fn resolve_prefers_explicit_over_managed() {
        let resolved = resolve_java(
            17,
            Some(runtime(JavaSource::InstanceExplicit, 21)),
            Some(runtime(JavaSource::ProfileExplicit, 17)),
            Some(runtime(JavaSource::Managed, 17)),
            None,
            Some(runtime(JavaSource::System, 8)),
        )
        .unwrap();
        assert_eq!(resolved.source, JavaSource::InstanceExplicit);
        assert_eq!(resolved.major, 21);
    }

    #[test]
    fn resolve_falls_through_slots_in_order() {
        // instance/profile trống → managed thắng; mojang/system không bị hỏi.
        let resolved = resolve_java(
            17,
            None,
            None,
            Some(runtime(JavaSource::Managed, 17)),
            Some(runtime(JavaSource::Mojang, 21)),
            Some(runtime(JavaSource::System, 21)),
        )
        .unwrap();
        assert_eq!(resolved.source, JavaSource::Managed);
    }

    #[test]
    fn resolve_skips_too_old_slots() {
        // profile explicit quá cũ → rơi xuống system đủ mới.
        let resolved = resolve_java(
            17,
            None,
            Some(runtime(JavaSource::ProfileExplicit, 8)),
            None,
            None,
            Some(runtime(JavaSource::System, 17)),
        )
        .unwrap();
        assert_eq!(resolved.source, JavaSource::System);
        assert_eq!(resolved.major, 17);
    }

    #[test]
    fn resolve_not_found_code() {
        let err = resolve_java(21, None, None, None, None, Some(runtime(JavaSource::System, 8)))
            .unwrap_err();
        assert_eq!(err.code(), "JAVA_NOT_FOUND");
        assert_eq!(err.to_string(), "no suitable java runtime found (required major 21)");
    }

    #[test]
    fn parse_java_version_formats() {
        assert_eq!(
            parse_java_version("openjdk version \"17.0.2\" 2022-01-18").unwrap(),
            (17, 0)
        );
        assert_eq!(
            parse_java_version("java version \"1.8.0_392\" Java(TM) SE Runtime").unwrap(),
            (8, 0)
        );
        assert_eq!(
            parse_java_version("openjdk version \"21\" 2023-09-19").unwrap(),
            (21, 0)
        );
        // Minor = component[1] (Java versioning: 21.0.x → minor 0) — consistent
        // với "17.0.2" → (17, 0) phía trên; parity discovery.py chỉ lấy major.
        assert_eq!(parse_java_version("java version \"21.0.1\"").unwrap(), (21, 0));
    }

    #[test]
    fn parse_java_version_garbage() {
        let err = parse_java_version("not a java output").unwrap_err();
        assert_eq!(err.code(), "JAVA_VERSION_UNPARSABLE");
    }
}
