//! antares-launch — Batch 14, mục **Minecraft launch** (§111 Orchestrator 2.0, §112 Session
//! state machine).
//!
//! Phase 1 — tách orchestrator thành các mảnh như §111 yêu cầu:
//! - `LaunchSession` — state machine §112: `IDLE → … → COMPLETED/CRASHED`
//! - `LaunchPreflight` — preflight checks fail-fast kèm remediation
//! - `ArgumentBuilder` — dựng `--username/--version/…` + JVM args (mảnh của ArgumentBuilder §111)
//!
//! Phase 2 — thêm `resolver`: `JavaResolver` (bọc antares-java resolve order §110) +
//! `ArtifactResolver` cục bộ (shared store §104 → game_dir → Missing).
//!
//! Phase 3 — `planner`: `required_java_major` §108 + `plan_launch` một bước.
//!
//! Phase 4 — `exit`: `ExitAnalyzer` §116 (pipeline ingest→…→recommendation, không
//! kết luận khi evidence yếu) + `CompanionPairing` (ghi companion.json parity
//! `RuntimeService.write_pairing_for_instance`).

pub mod command;
pub mod exit;
pub mod jvm;
pub mod mcjson;
pub mod planner;
pub mod preflight;
pub mod resolver;
pub mod session;

pub use command::{
    get_minecraft_command, patch_java, CommandOptions, LaunchHints, LAUNCHER_NAME, LAUNCHER_VERSION,
};
pub use exit::{
    CompanionEndpoint, CompanionPairing, Evidence, EvidenceSeverity, ExitAnalysis, ExitAnalyzer,
    ExitVerdict, PairingError,
};
pub use jvm::JvmConfig;
pub use mcjson::{
    classpath, inherit_json, natives_dir, natives_to_extract, parse_version_json,
    resolve_version_json, Artifact, AssetIndex, Library, OsInfo, VersionJson, VersionJsonError,
};
pub use planner::{
    plan_launch, required_java_major, JavaSlots, PlanError, PlanInput, PlanOutput,
};
pub use preflight::{LaunchPreflight, PreflightCheck, PreflightReport, PreflightSeverity};
pub use resolver::{
    ArtifactResolver, ArtifactStatus, JavaResolver, ResolvedArtifact, ResolvedJava,
};
pub use session::{LaunchPhase, LaunchSession, SessionTransitionError};

use serde::Serialize;

/// §111 — LaunchPlanner output: cấu hình cho một lần launch (input của ArgumentBuilder).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchPlan {
    pub instance_id: String,
    pub mc_version: String,
    pub loader: Option<String>,
    pub java_path: String,
    pub jvm_heap_max_mb: u32,
    pub username: String,
    pub game_directory: String,
}

/// §111 — ArgumentBuilder: dựng argv Minecraft + JVM từ plan.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchArguments {
    pub jvm: Vec<String>,
    pub game: Vec<String>,
    pub main_class: String,
}

pub struct ArgumentBuilder;

impl ArgumentBuilder {
    /// Dựng argument list. Giữ contract với legacy `services/minecraft/launch/options.py`:
    /// game args theo thứ tự vanilla (`--username`, `--version`, `--gameDir`, `--assetsDir`,
    /// `--uuid`, `--accessToken`, `--userType`, `--versionType`).
    pub fn build(plan: &LaunchPlan) -> LaunchArguments {
        let jvm = vec![
            format!("-Xmx{}M", plan.jvm_heap_max_mb),
            "-Dfile.encoding=UTF-8".to_string(),
            format!(
                "-Djava.io.tmpdir={}",
                plan.game_directory.trim_end_matches('/')
            ),
        ];

        let game = vec![
            "--username".into(),
            plan.username.clone(),
            "--version".into(),
            plan.mc_version.clone(),
            "--gameDir".into(),
            plan.game_directory.clone(),
            "--assetsDir".into(),
            format!("{}/assets", plan.game_directory.trim_end_matches('/')),
            "--uuid".into(),
            uuid_from_username(&plan.username),
            "--accessToken".into(),
            "0".into(),
            "--userType".into(),
            "msa".into(),
            "--versionType".into(),
            "antares".into(),
        ];

        LaunchArguments {
            jvm,
            game,
            main_class: "net.minecraft.client.main.Main".into(),
        }
    }
}

/// Offline-mode uuid: đúng shape UUID v3, deterministic theo username như legacy
/// offline mode. Skeleton dùng FNV-1a 128 (2 hạt nhân x 64 bit); thay bằng md5
/// `OfflinePlayer:<name>` thật ở phase sau khi nối auth.
fn uuid_from_username(username: &str) -> String {
    let seed = format!("OfflinePlayer:{username}");
    let mut h1: u64 = 0xcbf29ce484222325;
    let mut h2: u64 = 0x9e3779b97f4a7c15;
    for byte in seed.bytes() {
        h1 ^= byte as u64;
        h1 = h1.wrapping_mul(0x100000001b3);
        h2 ^= (byte ^ 0xA5) as u64;
        h2 = h2.wrapping_mul(0x100000001b3);
    }
    // UUID v3 shape: version nibble = 3, variant = 10xx.
    let a = (h1 >> 32) as u32;
    let b = ((h1 >> 16) & 0xFFFF) as u16;
    let c = 0x3000 | (h1 & 0x0FFF) as u16;
    let d = 0x8000 | ((h2 >> 48) & 0x3FFF) as u16;
    let e = h2 & 0xFFFF_FFFF_FFFF;
    format!("{a:08x}-{b:04x}-{c:04x}-{d:04x}-{e:012x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan() -> LaunchPlan {
        LaunchPlan {
            instance_id: "inst-1".into(),
            mc_version: "1.21.4".into(),
            loader: Some("fabric".into()),
            java_path: "/usr/lib/jvm/java-21/bin/java".into(),
            jvm_heap_max_mb: 4096,
            username: "Steve".into(),
            game_directory: "/game/inst-1".into(),
        }
    }

    #[test]
    fn session_walks_full_state_machine() {
        let mut session = LaunchSession::new("inst-1");
        assert_eq!(session.phase(), LaunchPhase::Idle);

        for expected in [
            LaunchPhase::Preflight,
            LaunchPhase::Resolving,
            LaunchPhase::Downloading,
            LaunchPhase::Installing,
            LaunchPhase::Pairing,
            LaunchPhase::Spawning,
            LaunchPhase::Running,
            LaunchPhase::Exiting,
            LaunchPhase::Analyzing,
            LaunchPhase::Completed,
        ] {
            session.advance(expected).unwrap();
        }
        assert!(session.phase().is_terminal());
        assert_eq!(session.phase(), LaunchPhase::Completed);
    }

    #[test]
    fn session_crash_path() {
        let mut session = LaunchSession::new("inst-1");
        for expected in [
            LaunchPhase::Preflight,
            LaunchPhase::Resolving,
            LaunchPhase::Downloading,
            LaunchPhase::Installing,
            LaunchPhase::Pairing,
            LaunchPhase::Spawning,
            LaunchPhase::Running,
            LaunchPhase::Exiting,
        ] {
            session.advance(expected).unwrap();
        }
        session.advance(LaunchPhase::Crashed).unwrap();
        assert_eq!(session.phase(), LaunchPhase::Crashed);
    }

    #[test]
    fn session_rejects_invalid_transition() {
        let mut session = LaunchSession::new("inst-1");
        let err = session.advance(LaunchPhase::Running).unwrap_err();
        assert_eq!(err.code(), "LAUNCH_INVALID_TRANSITION");
        // Preflight fail → Cancelled là hợp lệ (early exit).
        session.advance(LaunchPhase::Preflight).unwrap();
        session.advance(LaunchPhase::Cancelled).unwrap();
        assert!(session.phase().is_terminal());
    }

    #[test]
    fn argument_builder_contract() {
        let args = ArgumentBuilder::build(&plan());
        assert_eq!(args.main_class, "net.minecraft.client.main.Main");
        assert_eq!(args.jvm[0], "-Xmx4096M");
        assert_eq!(args.jvm[1], "-Dfile.encoding=UTF-8");
        assert_eq!(args.jvm[2], "-Djava.io.tmpdir=/game/inst-1");

        // game args theo cặp flag/value như vanilla
        let pairs: Vec<(String, String)> = args
            .game
            .chunks(2)
            .map(|c| (c[0].clone(), c.get(1).cloned().unwrap_or_default()))
            .collect();
        let flag = |name: &str| -> String {
            pairs
                .iter()
                .find(|(f, _)| f == name)
                .map(|(_, v)| v.clone())
                .expect(name)
        };
        assert_eq!(flag("--username"), "Steve");
        assert_eq!(flag("--version"), "1.21.4");
        assert_eq!(flag("--gameDir"), "/game/inst-1");
        assert_eq!(flag("--assetsDir"), "/game/inst-1/assets");
        assert_eq!(flag("--versionType"), "antares");
        // uuid deterministic offline-mode
        assert_eq!(flag("--uuid"), uuid_from_username("Steve"));
        assert_eq!(uuid_from_username("Steve"), uuid_from_username("Steve"));
        assert_ne!(uuid_from_username("Steve"), uuid_from_username("Alex"));
    }

    #[test]
    fn uuid_shape_is_valid() {
        for name in ["Steve", "Alex", "player-01"] {
            let uuid = uuid_from_username(name);
            assert_eq!(uuid.len(), 36, "uuid {uuid} must be 36 chars");
            assert_eq!(uuid.split('-').map(str::len).collect::<Vec<_>>(), [8, 4, 4, 4, 12]);
            assert!(uuid.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
        }
    }
}
