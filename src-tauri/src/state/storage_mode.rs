use serde::Serialize;

/// §218: runtime phải expose StorageMode thay vì từng service tự đoán path.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StorageMode {
    Installed,
    Portable,
}

impl StorageMode {
    pub fn detect() -> Self {
        if std::env::var("ANTARES_PORTABLE").ok().as_deref() == Some("1") {
            StorageMode::Portable
        } else {
            StorageMode::Installed
        }
    }
}
