//! A/B parity (B15.3 evidence) — lệnh Rust dựng ra phải GIỐNG
//! `minecraft-launcher_lib.get_minecraft_command` từng arg.
//!
//! Golden: `fixtures/mll_command_golden.json` sinh bởi
//! `tests/parity/gen_launch_command_golden.py` (MLL 8.0 thật, trên Linux).
//! Nên test chỉ chạy trên Linux — CI Windows lệch separator/path style.

#![cfg(target_os = "linux")]

use std::path::Path;

use antares_launch::{
    get_minecraft_command, parse_version_json, CommandOptions, LaunchHints, OsInfo,
};

const GAME_DIR: &str = "fixtures/game";

/// Đồng bộ 1-1 với OPTIONS trong gen script (legacy._build_options).
fn options<'a>(jvm: &'a [String]) -> CommandOptions<'a> {
    CommandOptions {
        username: "Steve",
        uuid: "00000000000000000000000000000001",
        token: "tok",
        executable: "java",
        jvm_arguments: jvm,
        game_directory: Path::new(GAME_DIR),
        // parity OPTIONS trong gen script — không launch hint → mọi feature tắt.
        hints: LaunchHints::default(),
    }
}

fn golden() -> serde_json::Value {
    serde_json::from_str(include_str!("../fixtures/mll_command_golden.json"))
        .expect("golden json")
}

fn assert_matches(key: &str, cmd: Vec<String>) {
    let expected: Vec<String> = golden()[key]
        .as_array()
        .unwrap_or_else(|| panic!("golden[{key}] phải là array"))
        .iter()
        .map(|v| v.as_str().expect("string arg").to_string())
        .collect();
    assert_eq!(
        cmd, expected,
        "Lệnh Rust phải khớp MLL từng arg (key={key})"
    );
}

#[test]
fn modern_version_matches_mll_golden() {
    let vj = parse_version_json(include_bytes!(
        "../fixtures/game/versions/1.21.11/1.21.11.json"
    ))
    .expect("parse modern fixture");
    let jvm = vec!["-Xms512M".to_string(), "-Xmx2048M".to_string()];
    let cmd = get_minecraft_command(&vj, Path::new(GAME_DIR), &options(&jvm), &OsInfo::current());
    assert_matches("modern", cmd);
}

#[test]
fn legacy_version_matches_mll_golden() {
    let vj =
        parse_version_json(include_bytes!("../fixtures/game/versions/1.8.9/1.8.9.json"))
            .expect("parse legacy fixture");
    let jvm = vec!["-Xms512M".to_string(), "-Xmx2048M".to_string()];
    let cmd = get_minecraft_command(&vj, Path::new(GAME_DIR), &options(&jvm), &OsInfo::current());
    assert_matches("legacy", cmd);
}
