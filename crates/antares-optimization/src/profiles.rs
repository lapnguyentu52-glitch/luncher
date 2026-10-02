//! Optimization profiles — parity 1:1 `services/optimization/profiles.py`
//! (mục 3.2, 3.3). Mỗi profile gồm:
//! - `jvm`      : patch lên instance.json (memory.minMb/maxMb, jvmPreset, jvmArgs)
//! - `minecraft`: key ghi vào game/options.txt (Render Distance, Particles…)
//!
//! Nguyên tắc mục 74: KHÔNG profile nào đụng system-wide; mọi thay đổi
//! instance-local + có snapshot rollback (mục 70).

use serde::Serialize;

use crate::MemoryRecommendation;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileJvm {
    /// None = dùng memory recommended từ advisor (mục 72).
    pub memory: Option<ProfileMemory>,
    pub jvm_preset: &'static str,
    pub jvm_args: &'static [&'static str],
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileMemory {
    pub min_mb: u32,
    pub max_mb: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OptProfile {
    pub id: &'static str,
    pub label_key: &'static str,
    pub desc_key: &'static str,
    pub jvm: ProfileJvm,
    pub minecraft: &'static [(&'static str, McValue)],
}

/// Giá trị options.txt của profile — bool giữ kiểu để coerce khi ghi.
/// `Owned` = chuỗi đã render sẵn (apply merge từ plan.after — ghi nguyên không
/// coerce lại lần 2).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum McValue {
    Bool(bool),
    Num(f64),
    Str(&'static str),
    Owned(String),
}

impl McValue {
    /// So sánh với value parse từ options.txt (luôn là string) — parity
    /// `current.get(key) != mc_patch[key]` (Python so lỏng kiểu raw string).
    pub fn equals_str(&self, raw: &str) -> bool {
        match self {
            McValue::Bool(b) => raw.eq_ignore_ascii_case(if *b { "true" } else { "false" }),
            McValue::Num(n) => {
                let raw_trim = raw.trim();
                raw_trim.parse::<f64>().map(|v| v == *n).unwrap_or(false)
                    || raw_trim == format!("{}", *n)
            }
            McValue::Str(s) => raw.eq_ignore_ascii_case(s),
            McValue::Owned(s) => raw == s,
        }
    }

    /// Coerce khi ghi (parity `_coerce`: bool → true/false, còn lại str(v)).
    pub fn to_option_str(&self) -> String {
        match self {
            McValue::Bool(true) => "true".into(),
            McValue::Bool(false) => "false".into(),
            McValue::Num(n) => {
                // str(v) Python: 10.0 → "10.0", 10 → "10"
                if n.fract() == 0.0 && n.abs() < 1e15 {
                    format!("{}", *n as i64)
                } else {
                    format!("{n}")
                }
            }
            McValue::Str(s) => (*s).to_string(),
            McValue::Owned(s) => s.clone(),
        }
    }
}

pub const PROFILES: &[OptProfile] = &[
    OptProfile {
        id: "balanced",
        label_key: "balanced",
        desc_key: "balancedDesc",
        jvm: ProfileJvm {
            memory: None,
            jvm_preset: "auto",
            jvm_args: &[],
        },
        minecraft: &[
            ("renderDistance", McValue::Num(10.0)),
            ("simulationDistance", McValue::Num(10.0)),
            ("particles", McValue::Num(0.0)),
            ("clouds", McValue::Str("true")),
            ("entityShadows", McValue::Bool(true)),
            ("mipmapLevels", McValue::Num(4.0)),
            ("vsync", McValue::Bool(false)),
            ("maxFps", McValue::Num(120.0)),
        ],
    },
    OptProfile {
        id: "low_end",
        label_key: "lowEnd",
        desc_key: "lowEndDesc",
        jvm: ProfileJvm {
            memory: Some(ProfileMemory { min_mb: 512, max_mb: 1536 }),
            jvm_preset: "g1",
            jvm_args: &[],
        },
        minecraft: &[
            ("renderDistance", McValue::Num(6.0)),
            ("simulationDistance", McValue::Num(6.0)),
            ("particles", McValue::Num(2.0)),
            ("clouds", McValue::Str("false")),
            ("entityShadows", McValue::Bool(false)),
            ("mipmapLevels", McValue::Num(0.0)),
            ("vsync", McValue::Bool(false)),
            ("maxFps", McValue::Num(60.0)),
            ("biomeBlendRadius", McValue::Num(0.0)),
            ("entityDistanceScaling", McValue::Num(0.75)),
        ],
    },
    OptProfile {
        id: "performance",
        label_key: "performance",
        desc_key: "performanceDesc",
        jvm: ProfileJvm {
            memory: None,
            jvm_preset: "balanced",
            jvm_args: &[],
        },
        minecraft: &[
            ("renderDistance", McValue::Num(8.0)),
            ("simulationDistance", McValue::Num(8.0)),
            ("particles", McValue::Num(1.0)),
            ("clouds", McValue::Str("false")),
            ("entityShadows", McValue::Bool(false)),
            ("mipmapLevels", McValue::Num(2.0)),
            ("vsync", McValue::Bool(false)),
            ("maxFps", McValue::Num(260.0)),
        ],
    },
    OptProfile {
        id: "competitive",
        label_key: "competitive",
        desc_key: "competitiveDesc",
        jvm: ProfileJvm {
            memory: None,
            jvm_preset: "balanced",
            jvm_args: &[],
        },
        minecraft: &[
            ("renderDistance", McValue::Num(7.0)),
            ("simulationDistance", McValue::Num(6.0)),
            ("particles", McValue::Num(2.0)),
            ("clouds", McValue::Str("false")),
            ("entityShadows", McValue::Bool(false)),
            ("mipmapLevels", McValue::Num(0.0)),
            ("vsync", McValue::Bool(false)),
            ("maxFps", McValue::Num(260.0)),
            ("biomeBlendRadius", McValue::Num(0.0)),
            ("entityDistanceScaling", McValue::Num(0.75)),
            ("guiScale", McValue::Num(2.0)),
        ],
    },
    OptProfile {
        id: "visual",
        label_key: "visual",
        desc_key: "visualDesc",
        jvm: ProfileJvm {
            memory: None,
            jvm_preset: "auto",
            jvm_args: &[],
        },
        minecraft: &[
            ("renderDistance", McValue::Num(14.0)),
            ("simulationDistance", McValue::Num(12.0)),
            ("particles", McValue::Num(0.0)),
            ("clouds", McValue::Str("fast")),
            ("entityShadows", McValue::Bool(true)),
            ("mipmapLevels", McValue::Num(4.0)),
            ("vsync", McValue::Bool(true)),
            ("maxFps", McValue::Num(120.0)),
            ("entityDistanceScaling", McValue::Num(1.5)),
        ],
    },
    OptProfile {
        id: "battery",
        label_key: "battery",
        desc_key: "batteryDesc",
        jvm: ProfileJvm {
            memory: None,
            jvm_preset: "g1",
            jvm_args: &[],
        },
        minecraft: &[
            ("renderDistance", McValue::Num(6.0)),
            ("simulationDistance", McValue::Num(6.0)),
            ("particles", McValue::Num(2.0)),
            ("clouds", McValue::Str("false")),
            ("entityShadows", McValue::Bool(false)),
            ("mipmapLevels", McValue::Num(1.0)),
            ("vsync", McValue::Bool(true)),
            ("maxFps", McValue::Num(60.0)),
        ],
    },
];

/// Thứ tự hiển thị trong UI — parity `PROFILE_ORDER`.
pub const PROFILE_ORDER: &[&str] = &[
    "balanced",
    "performance",
    "competitive",
    "low_end",
    "visual",
    "battery",
];

pub fn get_profile(profile_id: &str) -> Option<&'static OptProfile> {
    PROFILES.iter().find(|p| p.id == profile_id)
}

/// Catalog cho `scan()` — `[{id, labelKey, descKey}]` theo PROFILE_ORDER.
pub fn profile_catalog() -> Vec<serde_json::Value> {
    PROFILE_ORDER
        .iter()
        .filter_map(|id| get_profile(id))
        .map(|p| {
            serde_json::json!({
                "id": p.id,
                "labelKey": p.label_key,
                "descKey": p.desc_key,
            })
        })
        .collect()
}

/// Parity `memory_for_profile`: profile có `memory: None` = dùng memory recommended
/// từ advisor; recommended=None → giữ nguyên memory (không tự đoán — mục 72).
pub fn memory_for_profile(
    profile_id: &str,
    recommended: Option<&MemoryRecommendation>,
) -> Option<MemoryRecommendation> {
    let prof = get_profile(profile_id)?;
    match &prof.jvm.memory {
        Some(m) => Some(MemoryRecommendation {
            min_mb: m.min_mb,
            max_mb: m.max_mb,
        }),
        None => recommended.cloned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_count_and_order_parity() {
        assert_eq!(PROFILES.len(), 6);
        assert_eq!(
            PROFILE_ORDER,
            &["balanced", "performance", "competitive", "low_end", "visual", "battery"]
        );
        // mọi id trong order đều tồn tại
        for id in PROFILE_ORDER {
            assert!(get_profile(id).is_some(), "{id} phải tồn tại");
        }
        assert!(get_profile("nope").is_none());
    }

    #[test]
    fn profile_values_parity() {
        let low = get_profile("low_end").unwrap();
        assert_eq!(low.jvm.memory, Some(ProfileMemory { min_mb: 512, max_mb: 1536 }));
        assert_eq!(low.jvm.jvm_preset, "g1");
        // key value renderDistance=6, entityDistanceScaling=0.75
        let rd = low.minecraft.iter().find(|(k, _)| *k == "renderDistance").unwrap();
        assert_eq!(rd.1, McValue::Num(6.0));
        let visual = get_profile("visual").unwrap();
        let vsync = visual.minecraft.iter().find(|(k, _)| *k == "vsync").unwrap();
        assert_eq!(vsync.1, McValue::Bool(true));
        let balanced = get_profile("balanced").unwrap();
        let clouds = balanced.minecraft.iter().find(|(k, _)| *k == "clouds").unwrap();
        assert_eq!(clouds.1, McValue::Str("true"));
    }

    #[test]
    fn memory_for_profile_parity() {
        // low_end có memory riêng → thắng recommended
        assert_eq!(
            memory_for_profile(
                "low_end",
                Some(&MemoryRecommendation { min_mb: 999, max_mb: 9999 })
            ),
            Some(MemoryRecommendation { min_mb: 512, max_mb: 1536 })
        );
        // balanced memory=None → dùng recommended
        assert_eq!(
            memory_for_profile(
                "balanced",
                Some(&MemoryRecommendation { min_mb: 1000, max_mb: 4000 })
            ),
            Some(MemoryRecommendation { min_mb: 1000, max_mb: 4000 })
        );
        // recommended=None → giữ nguyên (không đoán)
        assert_eq!(memory_for_profile("balanced", None), None);
        // profile lạ → None
        assert_eq!(memory_for_profile("nope", None), None);
    }

    #[test]
    fn mc_value_coerce_parity() {
        assert_eq!(McValue::Bool(true).to_option_str(), "true");
        assert_eq!(McValue::Bool(false).to_option_str(), "false");
        assert_eq!(McValue::Num(120.0).to_option_str(), "120");
        assert_eq!(McValue::Num(0.75).to_option_str(), "0.75");
        assert_eq!(McValue::Str("fast").to_option_str(), "fast");
        // equals_str so với raw options.txt
        assert!(McValue::Bool(true).equals_str("true"));
        assert!(McValue::Num(120.0).equals_str("120"));
        assert!(McValue::Num(120.0).equals_str("120.0"));
        assert!(McValue::Str("fast").equals_str("fast"));
        assert!(!McValue::Bool(false).equals_str("true"));
    }
}
