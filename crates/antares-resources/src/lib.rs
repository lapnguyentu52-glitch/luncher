//! antares-resources — B15.1e, parity `services/resources/` (mục 12 Asset
//! Library, mục 10 Resource Studio; Batch 8a/8b).
//!
//! Phase A (đã làm — nền tảng parity mạnh nhất):
//! - `assets` — AssetStore content-addressed (mục 12.1): import PNG validate
//!   signature/IHDR/dims (8MB/4096px — mục 12.2, 34), sha256 → `assets/<sha>.png`
//!   (traversal không thể — filename không chạm filesystem raw), catalog JSON
//!   atomic + corrupt → backup + reset, list filter/sort, read/delete, assign
//!   vào project `assets/<ns>/textures/` (regex + `..` + `//` reject)
//! - `png` — encode/decode tối giản: `encode_png` (IHDR/IDAT/IEND + CRC32) cho
//!   template generation; `png_dims` header parse (không decode full — mục 77)
//! - `validator` — pack validate (mục 10.5 + 76, 77): pack.mcmeta, wrong_path
//!   prefix whitelist, unsupported ext (WARNING), oversized, PNG header,
//!   invalid_json, model texture refs, duplicate file (sha1), cấu trúc
//!   `assets/minecraft/<loại>/` + ext rules; **FAIL chỉ khi có ERROR** (mục 42)
//!
//! Phase sau: project store CRUD, builder ZIP + build_task, layering effective
//! order + sync options.txt (§173), installer install/backup/uninstall.

pub mod assets;
pub mod png;
pub mod validator;

pub use assets::{
    assign_to_project, import_png, list_assets, read_png, AssetStore, MAX_DIM, MAX_FILE_BYTES,
};
pub use png::{decode_png_dims, encode_png, PngError};
pub use validator::{
    validate_dir, validate_zip_names, Finding, KNOWN_MC_DIRS, MAX_PNG_DIM, OK_EXTS,
    VALID_PREFIXES,
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ResourceError {
    #[error("asset data trống hoặc sai")]
    Empty,
    #[error("asset quá lớn: {size} > {limit} bytes")]
    TooLarge { size: usize, limit: usize },
    #[error("asset invalid: {0}")]
    Invalid(String),
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("file not found: {0}")]
    NotFound(String),
    #[error("io: {0}")]
    Io(String),
}

impl ResourceError {
    /// Code taxonomy §117 — parity codes.ASSET_TOO_LARGE / ASSET_INVALID /
    /// VALIDATION_FAILED / FILE_NOT_FOUND.
    pub fn code(&self) -> &'static str {
        match self {
            ResourceError::TooLarge { .. } => "ASSET_TOO_LARGE",
            ResourceError::Invalid(_) => "ASSET_INVALID",
            ResourceError::Empty | ResourceError::Validation(_) => "VALIDATION_FAILED",
            ResourceError::NotFound(_) => "FILE_NOT_FOUND",
            ResourceError::Io(_) => "APP_INTERNAL",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetEntry {
    pub id: String,
    pub name: String,
    pub sha256: String,
    pub mime: String,
    pub width: u32,
    pub height: u32,
    pub bytes: usize,
    pub category: String,
    pub tags: Vec<String>,
    pub source: String,
    pub created_at: f64,
    pub updated_at: f64,
}

/// 8 category chuẩn mục 12.3.
pub const ASSET_CATEGORIES: [&str; 8] = [
    "block",
    "item",
    "entity",
    "gui",
    "particle",
    "font",
    "sound",
    "lang",
];

/// Parity `_safe_name`: lowercase, space→dash, chỉ [a-z0-9_-], max 64.
pub fn safe_name(name: &str) -> String {
    let lowered = name.trim().to_lowercase().replace(' ', "-");
    let clean: String = lowered
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-' || *c == '_')
        .collect();
    clean.chars().take(64).collect()
}

/// Parity `_safe_category`: lạ → "item".
pub fn safe_category(category: &str) -> String {
    if ASSET_CATEGORIES.contains(&category) {
        category.to_string()
    } else {
        "item".to_string()
    }
}

/// Parity `_safe_tag`: chỉ [a-z0-9_-], max 24.
pub fn safe_tag(tag: &str) -> String {
    let lowered = tag.to_lowercase();
    let clean: String = lowered
        .chars()
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-' || *c == '_')
        .collect();
    clean.chars().take(24).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories_and_sanitize_parity() {
        assert_eq!(ASSET_CATEGORIES.len(), 8);
        assert_eq!(safe_category("block"), "block");
        assert_eq!(safe_category("weird"), "item", "lạ → item");
        assert_eq!(safe_name("My Texture 01!"), "my-texture-01");
        assert_eq!(safe_name(""), "");
        assert_eq!(safe_tag("Red#Item"), "reditem");
        assert_eq!(safe_tag("a_b-c").len(), 5);
        let long = "x".repeat(70);
        assert_eq!(safe_name(&long).len(), 64);
    }

    #[test]
    fn error_codes_parity() {
        assert_eq!(
            ResourceError::TooLarge { size: 9, limit: 8 }.code(),
            "ASSET_TOO_LARGE"
        );
        assert_eq!(ResourceError::Invalid("hdr".into()).code(), "ASSET_INVALID");
        assert_eq!(ResourceError::Empty.code(), "VALIDATION_FAILED");
        assert_eq!(ResourceError::NotFound("a".into()).code(), "FILE_NOT_FOUND");
    }
}
