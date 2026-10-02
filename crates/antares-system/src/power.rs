//! PowerService — parity 1:1 `services/system/power.py` (mục 4.3 Power, 35, 74).
//!
//! Hint-first, không tự sửa:
//! - Đọc plan hiện tại qua `powercfg /getactivescheme` (Windows) — read-only
//! - OS khác → `{supported: false, plan: None, recommendation: None}` (không fail app)
//! - Đề xuất High performance khi đang Power Saver — chỉ HINT; đổi plan là action
//!   user bấm (requiresAdmin=True, reversible), KHÔNG tự áp (mục 74)
//! - set_plan: plan lạ → Ok(false); handler trên đổi thành CONFIG_INVALID (parity
//!   BridgeMethodError "unknown or failed plan")

use serde::Serialize;

/// Parity `_PLANS` — 3 plan GUID chuẩn Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PowerPlanId {
    PowerSaver,
    Balanced,
    HighPerformance,
}

impl PowerPlanId {
    pub fn guid(&self) -> &'static str {
        match self {
            PowerPlanId::PowerSaver => "a1841308-3541-4fab-bc81-f71556f20b4a",
            PowerPlanId::Balanced => "381b4222-f694-41f0-9685-ff5bb260df2e",
            PowerPlanId::HighPerformance => "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c",
        }
    }

    pub fn label_key(&self) -> &'static str {
        match self {
            PowerPlanId::PowerSaver => "powerSaver",
            PowerPlanId::Balanced => "balanced",
            PowerPlanId::HighPerformance => "highPerformance",
        }
    }

    pub fn from_guid(guid: &str) -> Option<Self> {
        let lower = guid.to_lowercase();
        if lower == PowerPlanId::PowerSaver.guid() {
            Some(PowerPlanId::PowerSaver)
        } else if lower == PowerPlanId::Balanced.guid() {
            Some(PowerPlanId::Balanced)
        } else if lower == PowerPlanId::HighPerformance.guid() {
            Some(PowerPlanId::HighPerformance)
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerPlan {
    pub id: String,
    pub name: String,
    pub label_key: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerRecommendation {
    pub to: &'static str,
    pub to_label_key: &'static str,
    pub reason_key: &'static str,
    pub risk: &'static str,
    pub reversible: bool,
    pub requires_admin: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PowerStatus {
    pub supported: bool,
    pub plan: Option<PowerPlan>,
    /// None = không cần đổi (parity recommendation None).
    pub recommendation: Option<PowerRecommendation>,
}

pub struct PowerService {
    windows: bool,
}

impl PowerService {
    pub fn new() -> Self {
        Self {
            windows: cfg!(windows),
        }
    }

    /// Cho test: ép platform flag.
    pub fn with_platform(windows: bool) -> Self {
        Self { windows }
    }

    /// Đọc plan hiện tại + recommendation. Không bao giờ panic/fail — lỗi powercfg
    /// nuốt thành plan None (parity try/except).
    pub fn status(&self) -> PowerStatus {
        if !self.windows {
            return PowerStatus {
                supported: false,
                plan: None,
                recommendation: None,
            };
        }
        let (plan_id, plan_name) = self.current_plan();
        let recommendation = Self::recommendation(plan_id.as_deref());
        let plan = plan_id.map(|id| PowerPlan {
            label_key: PowerPlanId::from_guid(&id)
                .map(|p| p.label_key().to_string())
                // GUID lạ có tên nhưng không nằm trong 3 plan chuẩn → label rỗng
                // (parity `_PLANS.get(plan_id.lower(), "")`).
                .unwrap_or_default(),
            id,
            name: plan_name.unwrap_or_default(),
        });
        PowerStatus {
            supported: true,
            plan,
            recommendation,
        }
    }

    /// Parity `_current_plan`: chạy `powercfg /getactivescheme`, regex
    /// `([0-9a-fA-F-]{36})\s*\(([^)]+)\)` trên stdout+stderr, trả (guid-lower, name).
    fn current_plan(&self) -> (Option<String>, Option<String>) {
        let Ok(output) = std::process::Command::new("powercfg")
            .arg("/getactivescheme")
            .output()
        else {
            return (None, None);
        };
        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));
        if let Some(plan) = parse_active_scheme(&text) {
            return (Some(plan.0), Some(plan.1));
        }
        (None, None)
    }

    /// Parity `_recommendation`: chỉ hint khi đang Power Saver (an toàn, mục 35).
    pub fn recommendation(plan_id: Option<&str>) -> Option<PowerRecommendation> {
        let plan_id = plan_id?;
        if plan_id.eq_ignore_ascii_case(PowerPlanId::PowerSaver.guid()) {
            return Some(PowerRecommendation {
                to: PowerPlanId::HighPerformance.guid(),
                to_label_key: "highPerformance",
                reason_key: "powerSaverHint",
                risk: "LOW",
                reversible: true,
                requires_admin: true,
            });
        }
        None
    }

    /// Action — chỉ chạy khi user bấm (UI confirm); requiresAdmin (mục 35).
    /// Plan lạ hoặc powercfg fail → `false` (handler đổi thành CONFIG_INVALID).
    pub fn set_plan(&self, plan_id: &str) -> bool {
        if !self.windows {
            return false;
        }
        // Parity: chỉ nhận 3 GUID chuẩn (lower check) — tên rút gọn bị chặn.
        let Some(plan) = PowerPlanId::from_guid(plan_id) else {
            return false;
        };
        let Ok(output) = std::process::Command::new("powercfg")
            .arg("/setactive")
            .arg(plan.guid())
            .output()
        else {
            return false;
        };
        output.status.success()
    }
}

impl Default for PowerService {
    fn default() -> Self {
        Self::new()
    }
}

/// Regex-free parity của `([0-9a-fA-F-]{36})\s*\(([^)]+)\)`: tìm GUID 36 ký tự
/// theo sau ` (name)`. Dùng cho cả test golden output powercfg.
pub fn parse_active_scheme(text: &str) -> Option<(String, String)> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i + 36 <= bytes.len() {
        // `get` trả None khi i không nằm ở char boundary — tiếng Việt có dấu
        // (vd "Ư...") không panic khi cắt chuỗi.
        let Some(candidate) = text.get(i..i + 36) else {
            i += 1;
            continue;
        };
        if candidate
            .chars()
            .all(|c| c.is_ascii_hexdigit() || c == '-')
            && candidate.matches('-').count() == 4
        {
            // Phải đứng độc lập (không nằm giữa hex khác)
            let before_ok = i == 0 || !bytes[i - 1].is_ascii_hexdigit() && bytes[i - 1] != b'-';
            // Tìm " (name)" sau candidate — cho phép whitespace
            let rest = &text[i + 36..];
            let trimmed = rest.trim_start_matches([' ', '\t']);
            if before_ok && trimmed.starts_with('(') {
                if let Some(end) = trimmed.find(')') {
                    let name = trimmed[1..end].trim().to_string();
                    return Some((candidate.to_lowercase(), name));
                }
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plan_ids_parity() {
        assert_eq!(
            PowerPlanId::PowerSaver.guid(),
            "a1841308-3541-4fab-bc81-f71556f20b4a"
        );
        assert_eq!(
            PowerPlanId::Balanced.guid(),
            "381b4222-f694-41f0-9685-ff5bb260df2e"
        );
        assert_eq!(
            PowerPlanId::HighPerformance.guid(),
            "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c"
        );
        assert_eq!(
            PowerPlanId::from_guid("8C5E7FDA-E8BF-4A96-9A85-A6E23A8C635C"),
            Some(PowerPlanId::HighPerformance)
        );
        assert_eq!(PowerPlanId::from_guid("not-a-guid"), None);
    }

    #[test]
    fn parse_active_scheme_golden_windows_output() {
        // Golden output thật của `powercfg /getactivescheme` (tiếng Anh + tiếng Việt có dấu).
        let english = "Power Scheme GUID: 381b4222-f694-41f0-9685-ff5bb260df2e  (Balanced)";
        assert_eq!(
            parse_active_scheme(english),
            Some((
                "381b4222-f694-41f0-9685-ff5bb260df2e".into(),
                "Balanced".into()
            ))
        );
        let vietnamese =
            "GUID lược đồ nguồn: a1841308-3541-4fab-bc81-f71556f20b4a  (Tiết kiệm điện)";
        assert_eq!(
            parse_active_scheme(vietnamese),
            Some((
                "a1841308-3541-4fab-bc81-f71556f20b4a".into(),
                "Tiết kiệm điện".into()
            ))
        );
        // Không GUID → None
        assert_eq!(parse_active_scheme("no plan here"), None);
        // GUID không có tên trong ngoặc → None
        assert_eq!(
            parse_active_scheme("GUID: 381b4222-f694-41f0-9685-ff5bb260df2e no parens"),
            None
        );
    }

    #[test]
    fn status_non_windows_unsupported() {
        let service = PowerService::with_platform(false);
        let status = service.status();
        assert!(!status.supported);
        assert!(status.plan.is_none());
        assert!(status.recommendation.is_none());
        // set_plan trên OS khác → false (không bao giờ gọi powercfg)
        assert!(!service.set_plan(PowerPlanId::Balanced.guid()));
    }

    #[test]
    fn recommendation_only_for_power_saver() {
        // Parity _recommendation: None khi không có plan / không phải Power Saver
        assert!(PowerService::recommendation(None).is_none());
        assert!(PowerService::recommendation(Some(PowerPlanId::Balanced.guid())).is_none());
        assert!(PowerService::recommendation(Some(PowerPlanId::HighPerformance.guid())).is_none());
        let rec = PowerService::recommendation(Some(PowerPlanId::PowerSaver.guid())).unwrap();
        assert_eq!(rec.to, PowerPlanId::HighPerformance.guid());
        assert_eq!(rec.to_label_key, "highPerformance");
        assert_eq!(rec.reason_key, "powerSaverHint");
        assert!(rec.reversible && rec.requires_admin);
        assert_eq!(rec.risk, "LOW");
    }

    #[test]
    fn set_plan_rejects_unknown_plan() {
        let service = PowerService::with_platform(true);
        // GUID lạ → false (không gọi powercfg — hành vi như legacy plan check)
        assert!(!service.set_plan("00000000-0000-0000-0000-000000000000"));
        // Tên rút gọn bị chặn (parity chỉ nhận GUID trong _PLANS)
        assert!(!service.set_plan("highPerformance"));
        assert!(!service.set_plan(""));
    }
}
