//! antares-optimization — B15.1c, parity `services/optimization/`
//! (advisor mục 72, profiles mục 3.2/3.3, service mục 3/70 plan-first + snapshot).
//!
//! ```text
//! services/optimization/            crates/antares-optimization
//! ├── advisor.py   recommend()   → advisor (thuần: nhận ram/threads đã đo)
//! ├── profiles.py  PROFILES      → profiles (6 preset + PROFILE_ORDER + memory_for)
//! └── service.py   GameOptimizationService → service (scan/plan/apply/rollback/snapshot_info)
//! ```
//!
//! ## Nguyên tắc an toàn
//!
//! - Flow mục 3.1: `scan → recommendation → preview diff → backup snapshot → apply
//!   → rollback (tuỳ chọn)` — **plan không ghi gì**, apply luôn snapshot trước (mục 70)
//! - Instance-local §74: chỉ đụng `instance.json` + `game/options.txt` — không
//!   system-wide
//! - Advisor "thiếu dữ liệu → không đoán" (mục 72): `ram_total_mb <= 0` → memory None,
//!   profile tính từ threads thôi
//! - Crate không link psutil — caller (Tauri command) đo hardware rồi inject

mod profiles;
mod service;

pub use profiles::{
    get_profile, memory_for_profile, profile_catalog, OptProfile, ProfileJvm, PROFILE_ORDER,
};
pub use service::{
    diff_jvm, diff_options, ApplyOutput, InstanceSnapshot, InstanceStore, JvmChange,
    OptionChange, PlanOutput, ScanOutput, Service, Snapshot, HARDWARE_DEFAULT, OPTIONS_KEYS,
    SERVICE_STATE_FILE,
};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Hardware {
    pub ram_total_mb: u32,
    pub cpu_threads: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecommendation {
    pub min_mb: u32,
    pub max_mb: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    pub hardware: Hardware,
    /// None = thiếu dữ liệu RAM — không đoán (mục 72).
    pub memory: Option<MemoryRecommendation>,
    pub profile: &'static str,
    pub warnings: Vec<&'static str>,
    pub bottlenecks: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum OptimizationError {
    #[error("instance not found: {0}")]
    InstanceNotFound(String),
    #[error("unknown profile: {0}")]
    UnknownProfile(String),
    #[error("no optimization snapshot to restore")]
    NoSnapshot,
    #[error("snapshot file missing: {0}")]
    SnapshotMissing(String),
    #[error("io: {0}")]
    Io(String),
}

impl OptimizationError {
    /// Code taxonomy §117 — parity codes.INSTANCE_NOT_FOUND / VALIDATION_FAILED /
    /// FILE_NOT_FOUND.
    pub fn code(&self) -> &'static str {
        match self {
            OptimizationError::InstanceNotFound(_) => "INSTANCE_NOT_FOUND",
            OptimizationError::UnknownProfile(_) | OptimizationError::NoSnapshot => {
                "VALIDATION_FAILED"
            }
            OptimizationError::SnapshotMissing(_) => "FILE_NOT_FOUND",
            OptimizationError::Io(_) => "APP_INTERNAL",
        }
    }
}

/// Parity `advisor._recommend_memory`: heap 35% RAM, trần 8GB, sàn 1GB;
/// min = max(max/2, 512). `ram_total_mb == 0` → None (không đoán).
pub fn recommend_memory(ram_total_mb: u32) -> Option<MemoryRecommendation> {
    if ram_total_mb == 0 {
        return None;
    }
    let max_mb = (8192u32).min((1024u32).max((ram_total_mb as f64 * 0.35) as u32));
    let min_mb = 512u32.max(max_mb / 2);
    Some(MemoryRecommendation { min_mb, max_mb })
}

/// Parity `advisor._recommend_profile`.
pub fn recommend_profile(ram_total_mb: u32, cpu_threads: u32) -> &'static str {
    if ram_total_mb > 0 && ram_total_mb <= 4096 {
        return "low_end";
    }
    if cpu_threads <= 4 {
        return "performance";
    }
    "balanced"
}

/// Parity `advisor._warnings`.
pub fn advisor_warnings(ram_total_mb: u32, cpu_threads: u32) -> Vec<&'static str> {
    let mut out = Vec::new();
    if ram_total_mb > 0 && ram_total_mb < 4096 {
        out.push("lowRam");
    }
    if cpu_threads <= 2 {
        out.push("fewCores");
    }
    out
}

/// Parity `advisor._bottlenecks`.
pub fn advisor_bottlenecks(ram_total_mb: u32, cpu_threads: u32) -> Vec<&'static str> {
    let mut out = Vec::new();
    if ram_total_mb > 0 && ram_total_mb < 8192 {
        out.push("ram");
    }
    if cpu_threads <= 4 {
        out.push("cpu");
    }
    out
}

/// Parity `advisor.recommend()` — thuần: caller inject hardware đã đo.
pub fn recommend(hardware: Hardware) -> Recommendation {
    Recommendation {
        memory: recommend_memory(hardware.ram_total_mb),
        profile: recommend_profile(hardware.ram_total_mb, hardware.cpu_threads),
        warnings: advisor_warnings(hardware.ram_total_mb, hardware.cpu_threads),
        bottlenecks: advisor_bottlenecks(hardware.ram_total_mb, hardware.cpu_threads),
        hardware,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recommend_memory_parity() {
        // Parity _recommend_memory: 35% RAM, trần 8192, sàn 1024, min = max/2 (≥512)
        assert_eq!(recommend_memory(0), None, "thiếu dữ liệu → không đoán");
        // 2048*0.35=716 < sàn 1024 → max=1024 (parity max(1024, int(ram*0.35)))
        assert_eq!(recommend_memory(2048), Some(MemoryRecommendation { min_mb: 512, max_mb: 1024 }));
        assert_eq!(
            recommend_memory(4096),
            Some(MemoryRecommendation { min_mb: 716, max_mb: 1433 })
        );
        assert_eq!(
            recommend_memory(16384),
            Some(MemoryRecommendation { min_mb: 2867, max_mb: 5734 })
        );
        // Trần 8GB
        assert_eq!(
            recommend_memory(65536),
            Some(MemoryRecommendation { min_mb: 4096, max_mb: 8192 })
        );
    }

    #[test]
    fn recommend_profile_parity() {
        assert_eq!(recommend_profile(4096, 16), "low_end");
        assert_eq!(recommend_profile(2048, 2), "low_end", "ram nhỏ ưu tiên trước");
        assert_eq!(recommend_profile(8192, 4), "performance");
        assert_eq!(recommend_profile(16384, 8), "balanced");
        assert_eq!(recommend_profile(0, 2), "performance", "ram=0 → chỉ xét threads");
    }

    #[test]
    fn warnings_and_bottlenecks_parity() {
        assert_eq!(advisor_warnings(2048, 2), vec!["lowRam", "fewCores"]);
        assert_eq!(advisor_warnings(8192, 8), Vec::<&str>::new());
        assert_eq!(advisor_bottlenecks(4096, 4), vec!["ram", "cpu"]);
        assert_eq!(advisor_bottlenecks(16384, 8), Vec::<&str>::new());
    }

    #[test]
    fn error_codes_parity() {
        assert_eq!(
            OptimizationError::InstanceNotFound("x".into()).code(),
            "INSTANCE_NOT_FOUND"
        );
        assert_eq!(
            OptimizationError::UnknownProfile("x".into()).code(),
            "VALIDATION_FAILED"
        );
        assert_eq!(OptimizationError::NoSnapshot.code(), "VALIDATION_FAILED");
        assert_eq!(
            OptimizationError::SnapshotMissing("f".into()).code(),
            "FILE_NOT_FOUND"
        );
    }
}
