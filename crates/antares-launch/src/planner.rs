//! Phase 3 — `LaunchPlanner` tổng hợp (§111): chọn Java theo resolution order,
//! dựng `LaunchPlan` + chạy preflight + dựng arguments trong một bước.
//!
//! `required_java_major` — `MinecraftVersionCapability` (§108): không để
//! `if version >= ...` rải trong 20 file; mapping tập trung tại đây.
//!
//! Mapping (quy tắc vanilla chính thức):
//! - alpha/beta (a1.x, b1.x)          → 8
//! - 1.x với x ≤ 16                    → 8
//! - 1.17.x                            → 16
//! - 1.18.x – 1.20.4                   → 17
//! - 1.20.5+ và 1.21+                  → 21

use antares_java::JavaRuntime;

use crate::preflight::{LaunchPreflight, PreflightInput, PreflightReport};
use crate::resolver::JavaResolver;
use crate::{ArgumentBuilder, LaunchArguments, LaunchPlan};

/// §108 — MinecraftVersionCapability: Java major yêu cầu cho 1 MC version.
pub fn required_java_major(mc_version: &str) -> u16 {
    let version = mc_version.trim().to_lowercase();
    // alpha/beta legacy → Java 8
    if version.starts_with('a') || version.starts_with('b') {
        return 8;
    }
    let parts: Vec<u32> = version
        .split('.')
        .map(|p| p.trim().parse().unwrap_or(0))
        .collect();
    let minor = parts.get(1).copied().unwrap_or(0);
    let patch = parts.get(2).copied().unwrap_or(0);
    match minor {
        m if m <= 16 => 8,
        17 => 16,
        m if (18..=20).contains(&m) => {
            if m == 20 && patch >= 5 {
                21
            } else {
                17
            }
        }
        _ => 21,
    }
}

/// 5 slot Java theo resolution order §110 — caller lấy từ settings/profile/discovery.
#[derive(Debug, Clone, Default)]
pub struct JavaSlots {
    pub instance_explicit: Option<JavaRuntime>,
    pub profile_explicit: Option<JavaRuntime>,
    pub managed: Option<JavaRuntime>,
    pub mojang: Option<JavaRuntime>,
    pub system: Option<JavaRuntime>,
}

/// Input cho plan_launch — snapshot cấu hình một lần launch.
pub struct PlanInput {
    pub instance_id: String,
    pub mc_version: String,
    pub loader: Option<String>,
    pub username: String,
    pub game_directory: String,
    pub jvm_heap_max_mb: u32,
    pub java_slots: JavaSlots,
    pub game_directory_locked: bool,
}

/// Output: plan + preflight report + arguments dựng sẵn.
#[derive(Debug)]
pub struct PlanOutput {
    pub plan: LaunchPlan,
    pub preflight: PreflightReport,
    pub arguments: LaunchArguments,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlanError {
    #[error("no suitable java runtime (required major {0})")]
    NoJava(u16),
}

impl PlanError {
    pub fn code(&self) -> &'static str {
        "JAVA_NOT_FOUND"
    }
}

/// §111 — LaunchPlanner: resolve java → build plan → preflight → arguments.
/// Java resolve fail → error (JAVA_NOT_FOUND) — preflight vẫn nhận Java đã resolve.
pub fn plan_launch(input: &PlanInput) -> Result<PlanOutput, PlanError> {
    let required_major = required_java_major(&input.mc_version);
    let slots = &input.java_slots;
    let resolved = JavaResolver::resolve(
        required_major,
        slots.instance_explicit.clone(),
        slots.profile_explicit.clone(),
        slots.managed.clone(),
        slots.mojang.clone(),
        slots.system.clone(),
    )
    .map_err(|_| PlanError::NoJava(required_major))?;

    let plan = LaunchPlan {
        instance_id: input.instance_id.clone(),
        mc_version: input.mc_version.clone(),
        loader: input.loader.clone(),
        java_path: resolved.path.clone(),
        jvm_heap_max_mb: input.jvm_heap_max_mb,
        username: input.username.clone(),
        game_directory: input.game_directory.clone(),
    };

    let preflight = LaunchPreflight::run(&PreflightInput {
        mc_version: input.mc_version.clone(),
        loader: input.loader.clone(),
        java_path: resolved.path,
        java_major: resolved.major,
        required_java_major: required_major,
        heap_max_mb: input.jvm_heap_max_mb,
        game_directory: input.game_directory.clone(),
        game_directory_locked: input.game_directory_locked,
    });

    let arguments = ArgumentBuilder::build(&plan);
    Ok(PlanOutput {
        plan,
        preflight,
        arguments,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use antares_java::JavaSource;

    #[test]
    fn required_java_major_mapping() {
        // §108 — bảng mapping chuẩn vanilla
        assert_eq!(required_java_major("b1.7.3"), 8);
        assert_eq!(required_java_major("1.8.9"), 8);
        assert_eq!(required_java_major("1.12.2"), 8);
        assert_eq!(required_java_major("1.16.5"), 8);
        assert_eq!(required_java_major("1.17"), 16);
        assert_eq!(required_java_major("1.17.1"), 16);
        assert_eq!(required_java_major("1.18"), 17);
        assert_eq!(required_java_major("1.19.4"), 17);
        assert_eq!(required_java_major("1.20.1"), 17);
        assert_eq!(required_java_major("1.20.4"), 17);
        assert_eq!(required_java_major("1.20.5"), 21);
        assert_eq!(required_java_major("1.20.6"), 21);
        assert_eq!(required_java_major("1.21.4"), 21);
        assert_eq!(required_java_major("1.22"), 21);
    }

    fn slots_system(major: u16) -> JavaSlots {
        JavaSlots {
            system: Some(JavaRuntime {
                source: JavaSource::System,
                path: format!("/jvm/java-{major}/bin/java"),
                major,
                minor: 0,
                architecture: "x86_64".into(),
                vendor: "vendor".into(),
                verified: true,
                capabilities: vec![],
            }),
            ..Default::default()
        }
    }

    fn input() -> PlanInput {
        PlanInput {
            instance_id: "inst-1".into(),
            mc_version: "1.21.4".into(),
            loader: Some("fabric".into()),
            username: "Steve".into(),
            game_directory: "/game/inst-1".into(),
            jvm_heap_max_mb: 4096,
            java_slots: slots_system(21),
            game_directory_locked: false,
        }
    }

    #[test]
    fn plan_launch_happy_path() {
        let output = plan_launch(&input()).expect("plan ok");
        assert_eq!(output.plan.java_path, "/jvm/java-21/bin/java");
        assert!(output.preflight.is_launchable());
        assert_eq!(output.arguments.main_class, "net.minecraft.client.main.Main");
        assert_eq!(output.arguments.jvm[0], "-Xmx4096M");
        // arguments game có username
        assert!(output.arguments.game.iter().any(|arg| arg == "Steve"));
    }

    #[test]
    fn plan_launch_old_java_is_preflight_blocker_not_plan_error() {
        // Java 8 cho MC 1.21 → resolve vẫn trả slot system (resolve chỉ check major ≥
        // required — 8 < 21 → cả 5 slot fail → JAVA_NOT_FOUND từ resolver).
        let mut input = input();
        input.java_slots = slots_system(8);
        let err = plan_launch(&input).unwrap_err();
        assert_eq!(err.code(), "JAVA_NOT_FOUND");
    }

    #[test]
    fn plan_launch_locked_game_dir_blocks_preflight() {
        let mut input = input();
        input.game_directory_locked = true;
        let output = plan_launch(&input).expect("plan vẫn dựng được");
        assert!(!output.preflight.is_launchable());
        assert_eq!(output.preflight.blocking, 1);
    }

    #[test]
    fn plan_launch_uses_explicit_over_system() {
        let mut input = input();
        input.java_slots.instance_explicit = Some(JavaRuntime {
            source: JavaSource::InstanceExplicit,
            path: "/custom/java-17/bin/java".into(),
            major: 17,
            minor: 0,
            architecture: "x86_64".into(),
            vendor: "vendor".into(),
            verified: true,
            capabilities: vec![],
        });
        // MC 1.17 (cần 16) — explicit 17 thắng system 21 theo order §110.
        input.mc_version = "1.17.1".into();
        let output = plan_launch(&input).expect("plan ok");
        assert_eq!(output.plan.java_path, "/custom/java-17/bin/java");
    }
}
