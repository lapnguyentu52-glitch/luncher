//! M5 — runtime flags: `ANTA_RUST_ONLY=1` cắt phụ thuộc sidecar Python.
//!
//! Khi bật: không auto-start sidecar, mọi lệnh `legacy_*` mutate bị từ chối
//! typed `LEGACY_DISABLED` ở tầng command → **không sinh Python subprocess**.
//! Flag đọc một lần khi process khởi động (env bất biến theo process).

/// Runtime flags của app — copy được, giữ trong `AppServices`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RuntimeFlags {
    /// true = rust-only mode (audit M5).
    pub rust_only: bool,
}

impl RuntimeFlags {
    /// Đọc `ANTA_RUST_ONLY` từ env hiện tại.
    pub fn from_env() -> Self {
        Self::from_value(std::env::var("ANTA_RUST_ONLY").ok().as_deref())
    }

    /// Parse giá trị flag — tách riêng để test không phải đụng env thật
    /// (env là global state, test song song sẽ đá nhau).
    /// Chuẩn hoá: trim + lowercase ∈ {"1", "true", "yes", "on"}.
    pub fn from_value(value: Option<&str>) -> Self {
        let rust_only = value.is_some_and(|raw| {
            matches!(
                raw.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        });
        Self { rust_only }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_only_truthy_values() {
        for value in ["1", "true", "TRUE", " yes ", "on", "On"] {
            assert!(
                RuntimeFlags::from_value(Some(value)).rust_only,
                "{value:?} phải bật rust-only"
            );
        }
    }

    #[test]
    fn rust_only_off_values() {
        for value in [
            None,
            Some("0"),
            Some("false"),
            Some(""),
            Some("no"),
            Some("garbage"),
            Some("11"),
        ] {
            assert!(
                !RuntimeFlags::from_value(value).rust_only,
                "{value:?} phải tắt rust-only"
            );
        }
    }

    #[test]
    fn from_env_agrees_with_var() {
        // Không set env thật trong test — chỉ đối chiếu với env hiện tại.
        let expected =
            std::env::var("ANTA_RUST_ONLY").is_ok_and(|v| RuntimeFlags::from_value(Some(&v)).rust_only);
        assert_eq!(RuntimeFlags::from_env().rust_only, expected);
    }
}
