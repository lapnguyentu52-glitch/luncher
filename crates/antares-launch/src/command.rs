//! B07b — dựng lệnh Minecraft parity `minecraft_launcher_lib.command` (8.0).
//!
//! Thứ tự byte-for-byte theo `get_minecraft_command`:
//!
//! ```text
//! [executable] + jvmArguments(options) + [version jvm args | default]
//!   + mainClass + [minecraftArguments.split | arguments.game] (+ --server/…)
//! ```
//!
//! Legacy options thật chỉ set `executablePath/jvmArguments/gameDirectory/
//! username/uuid/token` — Batch 07d thêm [`LaunchHints`] (profile launch hint,
//! mục 40): customResolution/quickPlay bật feature rules + `--server/--port`;
//! demo/`--disableMultiplayer`/`--disableChat` legacy không bao giờ set →
//! không có mặt (parity mặt cắt này).

use std::path::{Path, PathBuf};

use crate::mcjson::{
    classpath, natives_dir, rules_pass, ArgEntry, ArgValue, OsInfo, RuleOptions, VersionJson,
};

/// Launcher identity — parity MLL default (`launcherName`/`launcherVersion`).
pub const LAUNCHER_NAME: &str = "minecraft-launcher-lib";
pub const LAUNCHER_VERSION: &str = "8.0"; // minecraft_launcher_lib/version.txt

/// Launch options phụ (mục 40 — profile launch hint) parity MLL
/// `customResolution`/`server`/`port`/`quickPlay*`:
///
/// - `custom_resolution` → bật feature rule `has_custom_resolution`
///   (`--width/--height` trong arguments.game) + append `--width/--height`
///   ở đường `minecraftArguments` (parity `get_arguments_string`).
/// - `quick_play_*` (Some = set) → bật rule quick-play + placeholder.
/// - `server`/`port` → append `--server <s> [--port <p>]` sau game args
///   (port CHỈ khi server có — parity lồng nhau của MLL).
///
/// `demo`, `disableMultiplayer`, `disableChat`: legacy không set (ngoài
/// `_LAUNCH_KEYS`) → không đại diện ở đây.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LaunchHints<'a> {
    pub custom_resolution: bool,
    pub resolution_width: Option<&'a str>,
    pub resolution_height: Option<&'a str>,
    pub server: Option<&'a str>,
    pub port: Option<&'a str>,
    pub quick_play_path: Option<&'a str>,
    pub quick_play_singleplayer: Option<&'a str>,
    pub quick_play_multiplayer: Option<&'a str>,
    pub quick_play_realms: Option<&'a str>,
}

impl LaunchHints<'_> {
    /// Features rules (parity `parse_single_rule(rules, options)`).
    /// Library rules vẫn dùng `RuleOptions::default()` (MLL truyền `{}`).
    fn rule_options(&self) -> RuleOptions {
        RuleOptions {
            custom_resolution: self.custom_resolution,
            quick_play_path: self.quick_play_path.map(str::to_string),
            quick_play_singleplayer: self.quick_play_singleplayer.map(str::to_string),
            quick_play_multiplayer: self.quick_play_multiplayer.map(str::to_string),
            quick_play_realms: self.quick_play_realms.map(str::to_string),
        }
    }
}

/// Parity `MinecraftOptions` — đúng các key legacy `_build_options` truyền vào.
pub struct CommandOptions<'a> {
    pub username: &'a str,
    pub uuid: &'a str,
    pub token: &'a str,
    /// Parity `executablePath: "java"` — vị trí [0], `patch_java` thay sau.
    pub executable: &'a str,
    /// Parity `jvmArguments: build_args(JvmConfig)`.
    pub jvm_arguments: &'a [String],
    /// Parity `gameDirectory` — placeholder `${game_directory}`.
    pub game_directory: &'a Path,
    /// Launch options phụ (profile hint) — mặc định = MLL options bật đúng
    /// username/uuid/token/… (golden A/B dùng [`LaunchHints::default`]).
    pub hints: LaunchHints<'a>,
}

/// Parity `get_minecraft_command(version, game_dir, options)`.
pub fn get_minecraft_command(
    vj: &VersionJson,
    game_dir: &Path,
    opts: &CommandOptions<'_>,
    os: &OsInfo,
) -> Vec<String> {
    let natives = natives_dir(game_dir, &vj.id);
    let cp = classpath(vj, game_dir, os);
    let ctx = Placeholders {
        vj,
        game_dir,
        natives: &natives,
        classpath: &cp,
        opts,
        rule_options: opts.hints.rule_options(),
    };

    let mut cmd: Vec<String> = vec![opts.executable.to_string()];
    cmd.extend(opts.jvm_arguments.iter().cloned());

    // JVM args từ version json (key vắng mặt → default -Djava.library.path/-cp).
    match vj.arguments.as_ref().and_then(|a| a.jvm.as_ref()) {
        Some(entries) => extend_entries(&mut cmd, entries, &ctx, os),
        None => {
            cmd.push(format!("-Djava.library.path={}", natives.display()));
            cmd.push("-cp".into());
            cmd.push(cp.clone());
        }
    }

    cmd.push(vj.main_class.clone());

    // Game args — MLL ưu tiên minecraftArguments (kiểu cũ) trước.
    if let Some(ma) = &vj.minecraft_arguments {
        for token in ma.split(' ') {
            cmd.push(replace_placeholders(token, &ctx));
        }
        // parity `get_arguments_string`: custom resolution/demo append SAU token.
        if opts.hints.custom_resolution {
            cmd.push("--width".into());
            cmd.push(
                opts.hints
                    .resolution_width
                    .unwrap_or("854")
                    .to_string(),
            );
            cmd.push("--height".into());
            cmd.push(
                opts.hints
                    .resolution_height
                    .unwrap_or("480")
                    .to_string(),
            );
        }
    } else if let Some(game) = vj.arguments.as_ref().map(|a| &a.game) {
        extend_entries(&mut cmd, game, &ctx, os);
    }

    // parity `if "server" in options` — port LỒNG trong server (không có
    // server thì port bị bỏ). `disableMultiplayer`/`disableChat` legacy không
    // set → không nhánh (mục 07d ghi rõ ngoài scope).
    if let Some(server) = opts.hints.server {
        cmd.push("--server".into());
        cmd.push(server.to_string());
        if let Some(port) = opts.hints.port {
            cmd.push("--port".into());
            cmd.push(port.to_string());
        }
    }

    cmd
}

struct Placeholders<'a> {
    vj: &'a VersionJson,
    game_dir: &'a Path,
    natives: &'a Path,
    classpath: &'a String,
    opts: &'a CommandOptions<'a>,
    /// Features rules cho arguments (MLL truyền `options` — khác library
    /// rules luôn `{}`).
    rule_options: RuleOptions,
}

fn extend_entries(
    out: &mut Vec<String>,
    entries: &[ArgEntry],
    ctx: &Placeholders<'_>,
    os: &OsInfo,
) {
    for entry in entries {
        match entry {
            ArgEntry::Plain(value) => out.push(replace_placeholders(value, ctx)),
            ArgEntry::Ruled(ruled) => {
                // parity: "compatibilityRules" (nếu có) VÀ "rules" (nếu có)
                // cùng phải pass — mỗi list một nhóm, rỗng = pass.
                if !rules_pass(&ruled.rules, os, &ctx.rule_options)
                    || !rules_pass(&ruled.compatibility_rules, os, &ctx.rule_options)
                {
                    continue;
                }
                match &ruled.value {
                    ArgValue::One(value) => out.push(replace_placeholders(value, ctx)),
                    ArgValue::Many(values) => {
                        out.extend(values.iter().map(|v| replace_placeholders(v, ctx)))
                    }
                }
            }
        }
    }
}

/// Parity `replace_arguments` — đúng thứ tự replace của MLL, placeholder lạ
/// giữ nguyên (MLL không đụng tới `${clientid}`/`${auth_xuid}`…).
fn replace_placeholders(arg: &str, ctx: &Placeholders<'_>) -> String {
    let vj = ctx.vj;
    let path = ctx.game_dir;
    let opts = ctx.opts;
    let mut out = arg.to_string();

    let rep = |s: &mut String, key: &str, value: &str| {
        *s = s.replace(key, value);
    };

    rep(&mut out, "${natives_directory}", &ctx.natives.to_string_lossy());
    rep(&mut out, "${launcher_name}", LAUNCHER_NAME);
    rep(&mut out, "${launcher_version}", LAUNCHER_VERSION);
    rep(&mut out, "${classpath}", ctx.classpath);
    rep(&mut out, "${auth_player_name}", opts.username);
    rep(&mut out, "${version_name}", &vj.id);
    rep(&mut out, "${game_directory}", &opts.game_directory.to_string_lossy());
    rep(&mut out, "${assets_root}", &path.join("assets").to_string_lossy());
    rep(
        &mut out,
        "${assets_index_name}",
        vj.assets.as_deref().unwrap_or(&vj.id),
    );
    rep(&mut out, "${auth_uuid}", opts.uuid);
    rep(&mut out, "${auth_access_token}", opts.token);
    rep(&mut out, "${user_type}", "msa");
    rep(&mut out, "${version_type}", &vj.type_name);
    rep(&mut out, "${user_properties}", "{}");
    // parity `options.get("resolutionWidth", "854")` — hint không set vẫn 854.
    rep(
        &mut out,
        "${resolution_width}",
        opts.hints.resolution_width.unwrap_or("854"),
    );
    rep(
        &mut out,
        "${resolution_height}",
        opts.hints.resolution_height.unwrap_or("480"),
    );
    rep(
        &mut out,
        "${game_assets}",
        &path.join("assets").join("virtual").join("legacy").to_string_lossy(),
    );
    rep(&mut out, "${auth_session}", opts.token);
    rep(
        &mut out,
        "${library_directory}",
        &path.join("libraries").to_string_lossy(),
    );
    rep(&mut out, "${classpath_separator}", if cfg!(windows) { ";" } else { ":" });
    // parity `options.get(k) or "{k}"` — None/rỗng → giữ literal.
    rep(
        &mut out,
        "${quickPlayPath}",
        opts.hints
            .quick_play_path
            .filter(|s| !s.is_empty())
            .unwrap_or("{quickPlayPath}"),
    );
    rep(
        &mut out,
        "${quickPlaySingleplayer}",
        opts.hints
            .quick_play_singleplayer
            .filter(|s| !s.is_empty())
            .unwrap_or("{quickPlaySingleplayer}"),
    );
    rep(
        &mut out,
        "${quickPlayMultiplayer}",
        opts.hints
            .quick_play_multiplayer
            .filter(|s| !s.is_empty())
            .unwrap_or("{quickPlayMultiplayer}"),
    );
    rep(
        &mut out,
        "${quickPlayRealms}",
        opts.hints
            .quick_play_realms
            .filter(|s| !s.is_empty())
            .unwrap_or("{quickPlayRealms}"),
    );
    out
}

/// Parity `services/minecraft/launch/orchestrator._patch_java`:
/// thay executable bằng java đã resolve + lọc flag theo major
/// (`--sun-misc-unsafe-memory-access` ≥23, `--enable-native-access` ≥21).
pub fn patch_java(cmd: &mut Vec<String>, java_exe: &str, major: u16) {
    if let Some(first) = cmd.first_mut() {
        *first = java_exe.to_string();
    }
    let mut index = 0;
    cmd.retain(|arg| {
        let keep = index == 0
            || if arg.starts_with("--sun-misc-unsafe-memory-access") {
                major >= 23
            } else if arg.starts_with("--enable-native-access") {
                major >= 21
            } else {
                true
            };
        index += 1;
        keep
    });
}

/// Path thư mục game của instance (`<instances>/<id>/game`).
pub fn game_dir_of(instance_dir: &Path) -> PathBuf {
    instance_dir.join("game")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcjson::parse_version_json;

    /// Fixture hiện đại (arguments.game/jvm + rules os + natives maven) —
    /// shape theo client.json Mojang thật. Nguồn chung với A/B golden MLL.
    fn modern_json() -> String {
        include_str!("../fixtures/game/versions/1.21.11/1.21.11.json").to_string()
    }

    /// Fixture kiểu cũ — `minecraftArguments` (pre-1.13). Nguồn chung golden.
    fn legacy_json() -> String {
        include_str!("../fixtures/game/versions/1.8.9/1.8.9.json").to_string()
    }

    fn opts<'a>(game_dir: &'a Path, jvm: &'a [String]) -> CommandOptions<'a> {
        CommandOptions {
            username: "Steve",
            uuid: "00000000000000000000000000000001",
            token: "tok",
            executable: "java",
            jvm_arguments: jvm,
            game_directory: game_dir,
            hints: LaunchHints::default(),
        }
    }

    #[test]
    fn modern_command_order_and_rules() {
        let vj = parse_version_json(modern_json().as_bytes()).expect("parse");
        let game_dir = Path::new("/game/inst");
        let jvm = vec!["-Xms512M".to_string(), "-Xmx2048M".to_string()];
        let o = opts(game_dir, &jvm);
        let linux = OsInfo { name: "linux", bits32: false, version: "6.8.0".into() };

        let cmd = get_minecraft_command(&vj, game_dir, &o, &linux);

        // [0] executable + jvmArguments(options) trước version jvm args
        assert_eq!(cmd[0], "java");
        assert_eq!(cmd[1], "-Xms512M");
        assert_eq!(cmd[2], "-Xmx2048M");
        // version jvm: linux KHÔNG có -XstartOnFirstThread (windows/osx mới có)
        assert!(!cmd.contains(&"-XstartOnFirstThread".to_string()));
        let lib_path = cmd.iter().find(|a| a.starts_with("-Djava.library.path="));
        assert_eq!(
            lib_path.map(|s| s.trim_start_matches("-Djava.library.path=")),
            Some("/game/inst/versions/1.21.11/natives")
        );
        // -cp + classpath ngay trước mainClass
        let cp_index = cmd.iter().position(|a| a == "-cp").expect("-cp");
        assert!(cmd[cp_index + 1].contains("brigadier-1.3.13.jar"));
        assert!(cmd[cp_index + 1].ends_with("versions/1.21.11/1.21.11.jar"));
        assert_eq!(cmd[cp_index + 2], "net.minecraft.client.main.Main");

        // game args: placeholder đã thay
        let username = cmd.iter().position(|a| a == "--username").unwrap();
        assert_eq!(cmd[username + 1], "Steve");
        let game_dir_arg = cmd.iter().position(|a| a == "--gameDir").unwrap();
        assert_eq!(cmd[game_dir_arg + 1], "/game/inst");
        let assets = cmd.iter().position(|a| a == "--assetsDir").unwrap();
        assert_eq!(cmd[assets + 1], "/game/inst/assets");
        // demo/custom-resolution bị loại (feature live = false), osx-only loại trên linux
        assert!(!cmd.contains(&"--demo".to_string()));
        assert!(!cmd.contains(&"--screenshotDirname".to_string()));
        // disallow windows → linux được phép
        assert!(cmd.contains(&"--no-windows-flag".to_string()));
    }

    #[test]
    fn modern_command_windows_gets_xstart() {
        let vj = parse_version_json(modern_json().as_bytes()).expect("parse");
        let game_dir = Path::new("C:\\game\\inst");
        let o = opts(game_dir, &[]);
        let win = OsInfo { name: "windows", bits32: false, version: "10.0".into() };
        let cmd = get_minecraft_command(&vj, game_dir, &o, &win);
        let count = cmd.iter().filter(|a| *a == "-XstartOnFirstThread").count();
        assert_eq!(count, 1, "chỉ rule windows (osx loại) trên Windows");
        assert!(!cmd.contains(&"--no-windows-flag".to_string()), "disallow windows chặn");
    }

    #[test]
    fn legacy_minecraft_arguments_split_and_tokens() {
        let vj = parse_version_json(legacy_json().as_bytes()).expect("parse");
        let game_dir = Path::new("/mc");
        let jvm = vec!["-Xmx1024M".to_string()];
        let o = opts(game_dir, &jvm);
        let linux = OsInfo { name: "linux", bits32: false, version: "6.8.0".into() };
        let cmd = get_minecraft_command(&vj, game_dir, &o, &linux);

        let main = cmd.iter().position(|a| a == "net.minecraft.client.main.Main").unwrap();
        assert_eq!(cmd[main + 1], "--username");
        assert_eq!(cmd[main + 2], "Steve");
        let token = cmd.iter().position(|a| a == "--accessToken").unwrap();
        assert_eq!(cmd[token + 1], "tok");
        // json thiếu arguments.jvm → default -Djava.library.path + -cp
        assert!(cmd.iter().any(|a| a == "-cp"));
    }

    #[test]
    fn patch_java_swaps_exe_and_filters_flags_by_major() {
        let mut cmd: Vec<String> = [
            "java",
            "-Xmx1G",
            "--enable-native-access=ALL-UNNAMED",
            "--sun-misc-unsafe-memory-access=deny",
            "-cp",
            "x",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        patch_java(&mut cmd, "/opt/jdk21/bin/java", 21);
        assert_eq!(cmd[0], "/opt/jdk21/bin/java");
        assert!(cmd.contains(&"--enable-native-access=ALL-UNNAMED".to_string()), "21 ≥ 21 giữ");
        assert!(!cmd.iter().any(|a| a.starts_with("--sun-misc-unsafe-memory-access")), "21 < 23 loại");

        let mut cmd2: Vec<String> = ["java", "--enable-native-access=ALL-UNNAMED"]
            .iter().map(|s| s.to_string()).collect();
        patch_java(&mut cmd2, "j17", 17);
        assert!(!cmd2.iter().any(|a| a.starts_with("--enable-native-access")), "17 < 21 loại");
    }

    #[test]
    fn inherits_from_is_rejected() {
        let json = r#"{"id":"fabric","type":"release","inheritsFrom":"1.21.11",
            "mainClass":"net.fabricmc.loader.impl.launch.knot.KnotClient",
            "minecraftArguments":"","libraries":[]}"#;
        let err = parse_version_json(json.as_bytes()).expect_err("phải từ chối");
        assert_eq!(err.code(), "VALIDATION_FAILED");
    }

    // ---- Batch 07d — profile launch hints (parity orchestrator._profile_launch_overrides) ----

    #[test]
    fn hint_custom_resolution_modern_appends_width_via_rules() {
        let vj = parse_version_json(modern_json().as_bytes()).expect("parse");
        let game_dir = Path::new("/game/inst");
        let linux = OsInfo { name: "linux", bits32: false, version: "6.8.0".into() };
        let mut o = opts(game_dir, &[]);
        o.hints = LaunchHints {
            custom_resolution: true,
            resolution_width: Some("1920"),
            resolution_height: Some("1080"),
            ..LaunchHints::default()
        };
        let cmd = get_minecraft_command(&vj, game_dir, &o, &linux);
        // fixture hiện đại: rule `has_custom_resolution` → ["--width", "${resolution_width}"].
        let w = cmd.iter().position(|a| a == "--width").expect("--width có mặt");
        assert_eq!(cmd[w + 1], "1920", "placeholder thay theo hint");
        // demo vẫn tắt (legacy không set).
        assert!(!cmd.contains(&"--demo".to_string()));
        // mặc định (hint off) → không có --width (golden A/B).
        let plain = get_minecraft_command(&vj, game_dir, &opts(game_dir, &[]), &linux);
        assert!(!plain.contains(&"--width".to_string()));
    }

    #[test]
    fn hint_custom_resolution_legacy_appends_width_height() {
        let vj = parse_version_json(legacy_json().as_bytes()).expect("parse");
        let game_dir = Path::new("/mc");
        let linux = OsInfo { name: "linux", bits32: false, version: "6.8.0".into() };
        let mut o = opts(game_dir, &[]);
        o.hints = LaunchHints { custom_resolution: true, ..LaunchHints::default() };
        let cmd = get_minecraft_command(&vj, game_dir, &o, &linux);
        // parity get_arguments_string: default 854×480 khi hint không set size.
        let w = cmd.iter().position(|a| a == "--width").expect("--width");
        assert_eq!(cmd[w + 1], "854");
        let h = cmd.iter().position(|a| a == "--height").expect("--height");
        assert_eq!(cmd[h + 1], "480");
        assert!(h == w + 2, "thứ tự --width <v> --height <v>");

        o.hints.resolution_width = Some("1280");
        o.hints.resolution_height = Some("720");
        let cmd = get_minecraft_command(&vj, game_dir, &o, &linux);
        let w = cmd.iter().position(|a| a == "--width").unwrap();
        assert_eq!(cmd[w + 1], "1280");
        let h = cmd.iter().position(|a| a == "--height").unwrap();
        assert_eq!(cmd[h + 1], "720");
    }

    #[test]
    fn hint_server_appends_after_game_args_with_nested_port() {
        let vj = parse_version_json(modern_json().as_bytes()).expect("parse");
        let game_dir = Path::new("/game/inst");
        let linux = OsInfo { name: "linux", bits32: false, version: "6.8.0".into() };

        let mut o = opts(game_dir, &[]);
        o.hints = LaunchHints {
            server: Some("play.example.com"),
            port: Some("25565"),
            ..LaunchHints::default()
        };
        let cmd = get_minecraft_command(&vj, game_dir, &o, &linux);
        // parity: --server/--port append CUỐI lệnh (sau mọi game args).
        assert_eq!(
            &cmd[cmd.len() - 4..],
            ["--server", "play.example.com", "--port", "25565"]
        );
        // port CHỈ khi server có (parity lồng nhau MLL).
        let mut o2 = opts(game_dir, &[]);
        o2.hints.port = Some("25565");
        let cmd2 = get_minecraft_command(&vj, game_dir, &o2, &linux);
        assert!(!cmd2.contains(&"--port".to_string()));
        assert!(!cmd2.contains(&"--server".to_string()));
        // server có mà không port → chỉ --server.
        let mut o3 = opts(game_dir, &[]);
        o3.hints.server = Some("h");
        let cmd3 = get_minecraft_command(&vj, game_dir, &o3, &linux);
        assert_eq!(&cmd3[cmd3.len() - 2..], ["--server", "h"]);
    }

    #[test]
    fn hint_quick_play_rules_emit_flags_and_placeholders() {
        // Synthetic version json có 2 quick-play ruled args (shape thật 1.21+).
        let json = r#"{"id":"1.21","type":"release","mainClass":"M",
            "arguments":{"game":[
                "--username",
                {"rules":[{"action":"allow","features":{"is_quick_play_singleplayer":true}}],
                 "value":["--quickPlaySingleplayer","${quickPlaySingleplayer}"]},
                {"rules":[{"action":"allow","features":{"has_quick_plays_support":true}}],
                 "value":["--quickPlayPath","${quickPlayPath}"]}
            ],"jvm":[]},"libraries":[]}"#;
        let vj = parse_version_json(json.as_bytes()).expect("parse");
        let game_dir = Path::new("/g");
        let linux = OsInfo { name: "linux", bits32: false, version: "6.8.0".into() };

        // Hint tắt → ruled args bị loại (parity MLL options={}).
        let plain = get_minecraft_command(&vj, game_dir, &opts(game_dir, &[]), &linux);
        assert!(!plain.contains(&"--quickPlaySingleplayer".to_string()));
        assert!(!plain.contains(&"--quickPlayPath".to_string()));

        let mut o = opts(game_dir, &[]);
        o.hints = LaunchHints {
            quick_play_singleplayer: Some("world1"),
            quick_play_path: Some("quickplay.json"),
            ..LaunchHints::default()
        };
        let cmd = get_minecraft_command(&vj, game_dir, &o, &linux);
        let sp = cmd.iter().position(|a| a == "--quickPlaySingleplayer").expect("flag");
        assert_eq!(cmd[sp + 1], "world1", "placeholder thay giá trị hint");
        let p = cmd.iter().position(|a| a == "--quickPlayPath").expect("path flag");
        assert_eq!(cmd[p + 1], "quickplay.json", "placeholder thay giá trị hint");
        assert!(!cmd.iter().any(|a| a.contains("${quickPlay")), "không sót placeholder");
    }
}
