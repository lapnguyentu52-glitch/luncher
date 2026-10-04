//! B07b — `play_launch` parity `handle_play_launch` → `AntaresApi.minecraft_launch`
//! → `LaunchOrchestrator._pipeline` (phần native, vanilla + instance đã cài).
//!
//! Khác có chủ đích so với legacy: validation (instance/lock/account) chạy
//! **đồng bộ** trước → lỗi trả typed §117 ngay cho UI (legacy fail trong
//! thread → task failed âm thầm, UI chỉ thấy "Launch started"). Task LAUNCH
//! tạo sau bước account (lỗi resolve/install/java → task failed + typed error).
//!
//! Batch 07c: install pipeline tự chạy khi version chưa cài (parity
//! orchestrator step 3 `loader.install`) + loader fabric/forge resolve.
//! Batch 07d: Mojang runtime download (`_resolve_java` bước 2–3) + profile
//! launch hint (`_profile_launch_overrides`, mục 40).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use antares_core::tasks::TaskPriority;
use antares_core::TaskRegistry;
use antares_java::{JavaRuntime, JavaSource};
use antares_launch::{
    get_minecraft_command, natives_dir, natives_to_extract, patch_java, resolve_version_json,
    CommandOptions, CompanionPairing, JavaResolver, JvmConfig, LaunchHints, OsInfo, VersionJson,
    VersionJsonError,
};
use antares_mods::{zip_entries, zip_read_entry};
use antares_process::{spawn_with_secrets, CleanupPolicy, LogMux, SpawnError};
use antares_storage::ScopedRoot;

use crate::error::{codes, AppError, AppResult};
use crate::install;
use crate::lock::ResourceLock;
use crate::system;
use crate::AppServices;

/// Kết quả một lần launch (task result — legacy `{sessionId, pid, version}`).
#[derive(Debug, Clone)]
pub struct LaunchOutcome {
    pub task_id: String,
    pub session_id: String,
    pub pid: u32,
    pub version: String,
}

/// Scope log game trong logs root — parity `on_line → MINECRAFT_OUTPUT`
/// (F-12: `<logs>/<scope>.log`).
const LOG_SCOPE_PREFIX: &str = "game-";
const LOG_RING_CAPACITY: usize = 10_000;

/// Parity `LaunchOrchestrator.launch(instance_id, task)` — xem doc module.
pub fn launch(
    tasks: &TaskRegistry,
    services: &Arc<AppServices>,
    instance_id: &str,
) -> AppResult<LaunchOutcome> {
    launch_with(
        tasks,
        services,
        instance_id,
        &install::Endpoints::default(),
    )
}

/// Biến thể inject [`install::Endpoints`] — test offline-deterministic
/// (endpoint chết → lỗi typed ngay, không đụng mạng thật).
pub(crate) fn launch_with(
    tasks: &TaskRegistry,
    services: &Arc<AppServices>,
    instance_id: &str,
    endpoints: &install::Endpoints,
) -> AppResult<LaunchOutcome> {
    // 1. instance — parity `_load_instance` → INSTANCE_NOT_FOUND (đúng message legacy).
    let instance = services.instances().get(instance_id).ok_or_else(|| {
        AppError::new(
            codes::INSTANCE_NOT_FOUND,
            format!("Instance '{instance_id}' không tồn tại"),
        )
    })?;
    let instance_root = PathBuf::from(&instance.directory);

    // 2. lock — parity ResourceLock `.lock` trong instance root.
    let mut lock = ResourceLock::new(instance_root.join(".lock"));
    if !lock.acquire() {
        return Err(AppError::new(
            codes::INSTANCE_LOCKED,
            "Instance is already running or installing.",
        ));
    }

    // 3. account — parity: thiếu → AUTH_FAILED + action (mở tab Accounts).
    let account = services
        .accounts()
        .selected_account_for_launch()
        .ok_or_else(|| {
            AppError::new(codes::AUTH_FAILED, "No account selected")
                .with_action("OPEN_ACCOUNTS")
        })?;
    let username = account
        .get("displayName")
        .and_then(|v| v.as_str())
        .ok_or_else(|| {
            AppError::new(codes::AUTH_FAILED, "account missing displayName")
                .with_action("OPEN_ACCOUNTS")
        })?
        .to_string();
    // parity `_build_options`: uuid = minecraftUuid hoặc uuid4().hex (32 hex).
    let uuid = account
        .get("minecraftUuid")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(new_hex32);
    let token = account
        .get("token")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // 4. task LAUNCH — tạo SAU validations sync (07b) và TRƯỚC install
    //    (07c) để report tiến độ tải; mọi lỗi từ đây → task failed + typed.
    let task = tasks.spawn("LAUNCH", TaskPriority::P1UserAction, None)?;
    let task_id = task.id.clone();
    if let Err(err) = tasks.start(&task_id) {
        return Err(AppError::internal(err.to_string()));
    }
    let fail = |err: AppError| -> AppError {
        let _ = tasks.fail(&task_id, err.to_string());
        err
    };

    // 5. resolve version — parity providers `resolve_launch_version`
    //    (vanilla identity / fabric-loader-latest / {mc}-forge-{build}).
    let game_dir = instance_root.join("game");
    let cache_dir = services.storage().root().join("cache");
    let installer = install::Installer::new(&game_dir, &cache_dir, endpoints.clone());
    let resolved = installer.resolve(&instance).map_err(&fail)?;

    // 6. version đã cài? parity `_is_installed` → `loader.install` (07c —
    //    auto-install khi Play; thiếu manifest entry → MINECRAFT_VERSION_NOT_FOUND).
    let progress = |status: &str, fraction: f64| {
        let _ = tasks.set_progress(&task_id, fraction, Some(status.to_string()));
    };
    if !installer.is_installed(&resolved) {
        installer
            .ensure_installed(
                &instance.loader,
                &instance.minecraft_version,
                &resolved,
                &progress,
            )
            .map_err(&fail)?;
    }
    // 6b. Đọc + merge version json (parity mlc.get_minecraft_command đọc
    //     merged — JSON fabric/forge trên đĩa GIỮ `inheritsFrom`, parse
    //     trực tiếp sẽ từ chối; javaVersion kế thừa từ parent vanilla).
    let vj = resolve_version_json(&game_dir, &resolved).map_err(|err: VersionJsonError| {
        fail(AppError::new(err.code(), format!("{err} (versions/{resolved})")))
    })?;

    // 7. java — parity `_resolve_java`: find_compatible (system) →
    //    mojang_runtime_executable (đã cài) → install_mojang_runtime (07d)
    //    → JAVA_NOT_FOUND + action (mở Settings → Java).
    let resolved_java = resolve_java_for(
        &instance.minecraft_version,
        &resolved,
        &vj,
        &game_dir,
        endpoints,
        (install::DOWNLOAD_MAX_ATTEMPTS, install::DOWNLOAD_BACKOFF),
        &progress,
    )
    .map_err(&fail)?;

    // 8. JVM args — parity JvmConfig + build_args (warnings chỉ log, không chặn).
    let jvm = JvmConfig::new(
        instance.memory.min_mb,
        instance.memory.max_mb,
        instance.jvm_preset.clone(),
        instance.jvm_args.clone(),
    );
    for warning in jvm.validate() {
        log::warn!("JVM warning: {warning}");
    }
    let jvm_args = jvm.build_args(system::cpu_count(), system::is_low_end());

    // 9. natives — MLL extract lúc install; nếu thư mục thiếu (dọn tay / file
    //    legacy) thì extract lại tại đây; lỗi chỉ warn (parity try/except nhẹ).
    let os = OsInfo::current();
    ensure_natives(&vj, &game_dir, &os);

    // 10. build lệnh — parity mlc.get_minecraft_command + _patch_java.
    //     Profile launch hint (mục 40) — parity orchestrator đọc
    //     `profiles_svc.launch_hint()` NGAY TRƯỚC get_minecraft_command,
    //     mọi lỗi → None (try/except legacy), rồi `_profile_launch_overrides`.
    let hint = read_launch_hint(services);
    let overrides = match &hint {
        Some(hint) => profile_launch_overrides(hint, instance_id),
        None => ProfileOverrides::default(), // parity `if hint:` — không có hint → không override
    };
    let mut cmd = get_minecraft_command(
        &vj,
        &game_dir,
        &CommandOptions {
            username: &username,
            uuid: &uuid,
            token: &token,
            executable: "java",
            jvm_arguments: &jvm_args,
            game_directory: &game_dir,
            hints: LaunchHints {
                custom_resolution: overrides.custom_resolution,
                resolution_width: overrides.resolution_width.as_deref(),
                resolution_height: overrides.resolution_height.as_deref(),
                server: overrides.server.as_deref(),
                port: overrides.port.as_deref(),
                // legacy `_LAUNCH_KEYS` không có quickPlayPath → luôn None
                // (parity: `has_quick_plays_support` không bao giờ bật).
                quick_play_path: None,
                quick_play_singleplayer: overrides.quick_play_singleplayer.as_deref(),
                quick_play_multiplayer: overrides.quick_play_multiplayer.as_deref(),
                quick_play_realms: overrides.quick_play_realms.as_deref(),
            },
        },
        &os,
    );
    patch_java(&mut cmd, &resolved_java.path, resolved_java.major);
    let (program, args) = cmd
        .split_first()
        .ok_or_else(|| fail(AppError::internal("launch command rỗng")))?;
    let program = program.clone();
    let args = args.to_vec();

    // (task LAUNCH đã tạo ở bước 4 — ở đây chỉ complete sau khi spawn.)

    // 11. LogMux scoped (F-12) — `<logs>/game-<instance>.log`.
    let scope = format!("{LOG_SCOPE_PREFIX}{instance_id}");
    let mux = LogMux::create_scoped(services.logs_root(), &scope, LOG_RING_CAPACITY)
        .map(Arc::new)
        .map_err(|err| {
            let _ = tasks.fail(&task_id, err.to_string());
            AppError::new(codes::STORAGE_WRITE_FAILED, err.to_string())
        })?;

    // 12. spawn — parity process_manager.spawn(owner minecraft, cwd game_dir,
    //     CleanupPolicy::Keep); token mask khỏi fingerprint (§26).
    let now_ms = antares_process::logmux::now_ms();
    let secrets = [token.clone()];
    let spawn_result = services.with_processes(|registry| {
        spawn_with_secrets(
            registry,
            "minecraft",
            Some(instance_id.to_string()),
            &program,
            &args,
            Some(game_dir.to_string_lossy().as_ref()),
            CleanupPolicy::Keep,
            Some(mux),
            now_ms,
            &secrets,
        )
    });
    let (proc_id, handle, pid) = match spawn_result {
        Ok(spawned) => spawned,
        Err(err) => {
            let _ = tasks.fail(&task_id, err.to_string());
            return Err(spawn_error_to_app(err));
        }
    };

    // 13. companion pairing — parity runtime.write_pairing_for_instance; IPC
    //     server (Batch 12) chưa chạy → endpoint None → không ghi (legacy nuốt lỗi).
    let now_secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    match CompanionPairing::write_pairing(None, &game_dir, now_secs) {
        Ok(_) => {}
        Err(err) => log::warn!("companion pairing skip: {err}"),
    }

    // 14. thread nền pump log + mark exit khi game thoát (không giữ mutex
    //     registry suốt vòng đời — wait_detached callback).
    let services_bg = Arc::clone(services);
    let proc_id_bg = proc_id.clone();
    let instance_id_bg = instance_id.to_string();
    std::thread::spawn(move || {
        let _ = antares_process::wait_detached(handle, None, move |state, code| {
            log::info!("minecraft exited (instance {instance_id_bg}): {state:?} code={code}");
            services_bg.with_processes(|registry| {
                let _ = registry.mark_exit(&proc_id_bg, state);
            });
        });
    });

    // 15. complete task (legacy complete(task, {sessionId, pid, version})).
    let session_id = new_hex32();
    let _ = tasks.set_progress(&task_id, 1.0, Some(format!("Minecraft started (pid {pid})")));
    let _ = tasks.complete(&task_id);

    // lock giữ tới cuối hàm — parity finally: release ngay khi pipeline xong
    // (SAU khi game đã spawn, không giữ suốt vòng đời game).
    drop(lock);

    Ok(LaunchOutcome {
        task_id,
        session_id,
        pid,
        version: resolved,
    })
}

/// Parity `JavaError JAVA_NOT_FOUND` — message legacy
/// `Java not found for Minecraft {mc}.` + action mở Settings → Java.
fn java_not_found(mc_version: &str) -> AppError {
    AppError::new(
        codes::JAVA_NOT_FOUND,
        format!("Java not found for Minecraft {mc_version}."),
    )
    .with_action("OPEN_JAVA_SETTINGS")
}

/// Slot system — parity `JavaManager.find_compatible` (discovery, major
/// ≥ required; chọn major cao nhất để không fail khi máy có nhiều JRE).
fn system_java_slot(required: u16) -> Option<JavaRuntime> {
    antares_java::scan_java_infos()
        .into_iter()
        .filter(|info| info.major >= required)
        .max_by_key(|info| info.major)
        // `info.path` là JAVA HOME (thư mục) — spawn phải dùng `info.exe`
        // (binary). Dùng home → Command::new(dir) = EACCES (bug 07b bắt
        // được qua test run_processor với java thật).
        .map(|info| JavaRuntime::system(info.exe, info.major))
}

/// Chọn JRE hệ thống ≥ required (§108) — mặt cắt system của
/// parity `_resolve_java`. `pub(crate)` — Batch 07c: Forge processors chạy
/// cùng java này (legacy processors không cài Mojang runtime → giữ system).
pub(crate) fn resolve_java(mc_version: &str) -> AppResult<antares_launch::ResolvedJava> {
    let required = antares_launch::required_java_major(mc_version);
    JavaResolver::resolve(required, None, None, None, None, system_java_slot(required))
        .map_err(|_| java_not_found(mc_version))
}

/// Parity `_resolve_java` FULL 3 bước (orchestrator step 4) — Batch 07d:
///
/// 1. `find_compatible(mc)` — java hệ thống ≥ required (chuỗi 07b giữ nguyên).
/// 2. `mojang_runtime_executable(mc, game_dir)` — probe JRE Mojang đã cài
///    trong game dir (component lấy từ version json `javaVersion`).
/// 3. `install_mojang_runtime(mc, game_dir, cb)` — tải JRE Mojang; legacy
///    nuốt mọi lỗi → None (đã warn).
///
/// Cả 3 trống → `JAVA_NOT_FOUND` + `OPEN_JAVA_SETTINGS` đúng message legacy
/// (`Java not found for Minecraft {mc}.`).
///
/// `mc_version` = instance.minecraftVersion (message parity), `resolved` = id
/// đã resolve (tính required — parity 07b), `vj` = json ĐÃ merge (fabric/forge
/// kế thừa `javaVersion` từ parent vanilla — parity MLL `get_client_json`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn resolve_java_for(
    mc_version: &str,
    resolved: &str,
    vj: &VersionJson,
    game_dir: &Path,
    endpoints: &install::Endpoints,
    tuning: (u32, bool),
    progress: install::Progress<'_>,
) -> AppResult<antares_launch::ResolvedJava> {
    let required = antares_launch::required_java_major(resolved);

    // 1. system — parity find_compatible (fail → không raise, đi tiếp).
    if let Ok(resolved_java) =
        JavaResolver::resolve(required, None, None, None, None, system_java_slot(required))
    {
        return Ok(resolved_java);
    }

    // Component Mojang runtime — parity `get_client_json()["javaVersion"]`.
    let jv = vj.java_version.as_ref();
    let component = jv
        .and_then(|j| j.component.as_deref())
        .filter(|c| !c.is_empty());
    let Some(component) = component else {
        return Err(java_not_found(mc_version));
    };
    let major_hint = jv.and_then(|j| j.major_version).unwrap_or(required);

    let to_resolved = |exe: PathBuf| -> antares_launch::ResolvedJava {
        // Detect thật (parity `_patch_java` tự detect_major) — fallback
        // majorVersion từ json rồi required (detect fail với binary hỏng).
        let major = antares_java::detect_major(&exe)
            .or_else(|| jv.and_then(|j| j.major_version))
            .unwrap_or(required);
        antares_launch::ResolvedJava {
            path: exe.to_string_lossy().into_owned(),
            major,
            source: JavaSource::Mojang,
        }
    };

    // 2. Đã cài — parity mojang_runtime_executable (probe im lặng).
    if let Some(exe) = crate::mojang_runtime::get_executable_path(component, game_dir) {
        return Ok(to_resolved(exe));
    }

    // 3. Tải — parity install_mojang_runtime: message TRƯỚC khi tải
    //    (`Installing Java runtime {component} (Java {major})...`), lỗi →
    //    None (wrapper đã warn) → JAVA_NOT_FOUND.
    progress(
        &format!("Installing Java runtime {component} (Java {major_hint})..."),
        0.9,
    );
    let exe = crate::mojang_runtime::install_mojang_runtime(
        component,
        game_dir,
        endpoints,
        tuning,
        progress,
    )
    .ok_or_else(|| java_not_found(mc_version))?;
    Ok(to_resolved(exe))
}

// ---------------------------------------------------------------------------
// Profile launch hint — Batch 07d, mục 40
// (parity orchestrator `_profile_launch_overrides` + `ProfileService.launch_hint`)
// ---------------------------------------------------------------------------

/// Overrides đã chuẩn hoá từ launch hint — owned (sinh từ JSON state file),
/// mượn lại khi dựng [`LaunchHints`] ở bước build lệnh.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ProfileOverrides {
    pub custom_resolution: bool,
    pub resolution_width: Option<String>,
    pub resolution_height: Option<String>,
    pub server: Option<String>,
    pub port: Option<String>,
    pub quick_play_singleplayer: Option<String>,
    pub quick_play_multiplayer: Option<String>,
    pub quick_play_realms: Option<String>,
}

/// Parity Python truthiness cho JSON value (nhất quán `is_truthy` của
/// antares-profiles — không dep crate đó để tránh vòng lặp composition).
fn truthy(value: Option<&serde_json::Value>) -> bool {
    match value {
        None | Some(serde_json::Value::Null) => false,
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
        Some(serde_json::Value::String(s)) => !s.is_empty(),
        Some(serde_json::Value::Array(a)) => !a.is_empty(),
        Some(serde_json::Value::Object(o)) => !o.is_empty(),
    }
}

/// Parity `str(value)` cho giá trị launch hint. Dict/list → None (legacy
/// `str()` sinh chuỗi rác kiểu `"{'a': 1}"` — không có giá trị dùng làm
/// `--server`, bỏ sạch an toàn hơn).
fn coerce_str(value: Option<&serde_json::Value>) -> Option<String> {
    match value? {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Number(n) => Some(n.to_string()),
        // parity `str(True)` — không ai set bool vào launch, giữ đúng parity.
        serde_json::Value::Bool(true) => Some("True".into()),
        serde_json::Value::Bool(false) => Some("False".into()),
        _ => None,
    }
}

/// Parity `LaunchOrchestrator._profile_launch_overrides(hint, instance_id)`:
///
/// - không phải dict → {};
/// - **vé an toàn**: `hint.instanceId` khác instance đang launch → {}
///   (chỉ ăn khi launch đúng instance profile đang áp — không nhảy chàm);
/// - `server` (truthy) → server + `port` (truthy, LỒNG trong server);
/// - `quickPlaySingleplayer/Multiplayer/Realms` truthy → str;
/// - `customResolution` truthy → true + size (falsy → default 854/480);
/// - server + quickPlayMultiplayer xung đột → **server thắng**.
pub(crate) fn profile_launch_overrides(
    hint: &serde_json::Value,
    launching_instance_id: &str,
) -> ProfileOverrides {
    let mut out = ProfileOverrides::default();
    // parity `isinstance(hint, dict)`.
    let Some(hint) = hint.as_object() else {
        return out;
    };
    // Vé an toàn — parity `if (instance_id and launching and !=)`. Writer
    // legacy luôn ghi str hoặc vắng (None → falsy → bỏ qua guard).
    if let Some(id) = hint
        .get("instanceId")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        if id != launching_instance_id {
            return out;
        }
    }
    if truthy(hint.get("server")) {
        out.server = coerce_str(hint.get("server"));
        if out.server.is_some() && truthy(hint.get("port")) {
            out.port = coerce_str(hint.get("port"));
        }
    }
    if truthy(hint.get("quickPlaySingleplayer")) {
        out.quick_play_singleplayer = coerce_str(hint.get("quickPlaySingleplayer"));
    }
    if truthy(hint.get("quickPlayMultiplayer")) {
        out.quick_play_multiplayer = coerce_str(hint.get("quickPlayMultiplayer"));
    }
    if truthy(hint.get("quickPlayRealms")) {
        out.quick_play_realms = coerce_str(hint.get("quickPlayRealms"));
    }
    if truthy(hint.get("customResolution")) {
        out.custom_resolution = true;
        // parity `str(hint.get(k) or "854")` — falsy → default.
        out.resolution_width = hint
            .get("resolutionWidth")
            .filter(|v| truthy(Some(v)))
            .and_then(|v| coerce_str(Some(v)))
            .or_else(|| Some("854".into()));
        out.resolution_height = hint
            .get("resolutionHeight")
            .filter(|v| truthy(Some(v)))
            .and_then(|v| coerce_str(Some(v)))
            .or_else(|| Some("480".into()));
    }
    // Xung đột — server thắng (parity `out.pop("quickPlayMultiplayer")`).
    if out.server.is_some() {
        out.quick_play_multiplayer = None;
    }
    out
}

/// Parity `ProfileService.launch_hint()` — đọc `config/profiles-state.json`
/// (cùng file Python sidecar ghi khi apply profile) rồi lấy key `launch`.
/// Mọi lỗi (thiếu/corrupt/version lệch/launch falsy) → None — parity
/// `_load_state` + `launch or None` + try/except trong orchestrator.
fn read_launch_hint(services: &AppServices) -> Option<serde_json::Value> {
    let handle = services.storage().scoped(ScopedRoot::Config);
    let state: serde_json::Value = handle.read_json("profiles-state.json").ok()?;
    // parity `_load_state`: version != 1 → cả state coi như rỗng.
    if state.get("version").and_then(|v| v.as_i64()) != Some(1) {
        return None;
    }
    let launch = state.get("launch")?;
    // parity `launch or None` — {} / null / kiểu không phải dict → None.
    launch.as_object().filter(|o| !o.is_empty())?;
    Some(launch.clone())
}

/// Extract natives jar vào `versions/<id>/natives` nếu thiếu (parity
/// `natives.extract_natives` — MLL thực thi lúc install). Lỗi → log, không
/// chặn launch (game thiếu natives sẽ crash và log sẽ chỉ ra).
fn ensure_natives(vj: &VersionJson, game_dir: &Path, os: &OsInfo) {
    let dest = natives_dir(game_dir, &vj.id);
    if dest.is_dir() {
        return;
    }
    for (jar, excludes) in natives_to_extract(vj, game_dir, os) {
        if !jar.is_file() {
            continue;
        }
        if let Err(err) = extract_natives_jar(&jar, &dest, &excludes) {
            log::warn!("extract natives {} thất bại: {err}", jar.display());
        }
    }
}

/// Parity `extract_natives_file`: ghi mọi entry trừ prefix exclude
/// (mặc định META-INF handled qua exclude của JSON). Path traversal bị chặn §50.
/// `pub(crate)` — Batch 07c: install pipeline extract natives lúc cài.
pub(crate) fn extract_natives_jar(
    jar: &Path,
    dest: &Path,
    excludes: &[String],
) -> Result<(), String> {
    let data = std::fs::read(jar).map_err(|err| err.to_string())?;
    std::fs::create_dir_all(dest).map_err(|err| err.to_string())?;
    for entry in zip_entries(&data).map_err(|err| err.to_string())? {
        if entry.name.ends_with('/') {
            continue;
        }
        if excludes.iter().any(|prefix| entry.name.starts_with(prefix)) {
            continue;
        }
        // §50 — từ chối thoát dest (absolute / .. / đường dẫn Windows).
        let relative = Path::new(&entry.name);
        if relative.is_absolute()
            || relative
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err(format!("entry thoát natives dir: {}", entry.name));
        }
        let out = dest.join(relative);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent).map_err(|err| err.to_string())?;
        }
        let bytes = zip_read_entry(&data, &entry.name).map_err(|err| err.to_string())?;
        std::fs::write(&out, bytes).map_err(|err| err.to_string())?;
    }
    Ok(())
}

fn spawn_error_to_app(err: SpawnError) -> AppError {
    AppError::new(err.code(), err.to_string())
}

/// Session id parity `uuid.uuid4().hex` — 32 hex (không cần crypto; chỉ để
/// correlate trong task result/log).
fn new_hex32() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    let a = nanos ^ (u64::from(std::process::id()) << 32);
    let b = COUNTER.fetch_add(0x9e37_79b9_7f4a_7c15, Ordering::Relaxed)
        ^ nanos.rotate_left(17);
    format!("{a:016x}{b:016x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex32_is_32_hex_chars() {
        let id = new_hex32();
        assert_eq!(id.len(), 32);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
        assert_ne!(new_hex32(), new_hex32(), "liên tục phải khác nhau");
    }

    /// Regression 07c: resolve_java phải trả ĐƯỜNG DẪN BINARY — `JavaInfo.path`
    /// là java HOME (thư mục) → spawn EACCES. Máy không có java → skip.
    #[test]
    fn resolve_java_returns_executable_file_not_home() {
        let Ok(resolved) = resolve_java("1.21.11") else {
            log::warn!("skip — không có java ≥ 21 trên máy chạy test");
            return;
        };
        assert!(
            std::path::Path::new(&resolved.path).is_file(),
            "resolved.path phải là binary java, thấy: {}",
            resolved.path
        );
    }

    #[test]
    fn missing_instance_is_typed_not_found() {
        let root = std::env::temp_dir().join(format!("antares-launch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let services = Arc::new(AppServices::new(&root));
        let tasks = TaskRegistry::new();

        let err = launch(&tasks, &services, "ghost").expect_err("phải fail");
        assert_eq!(err.code, codes::INSTANCE_NOT_FOUND);
        assert!(!err.retryable);
        assert_eq!(err.message, "Instance 'ghost' không tồn tại"); // parity message legacy

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn instance_without_account_is_auth_failed_with_action() {
        let root = std::env::temp_dir().join(format!("antares-launch-na-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let services = Arc::new(AppServices::new(&root));
        let tasks = TaskRegistry::new();
        let instance = services
            .instances()
            .create("Demo", "1.21.11", None, None, None)
            .expect("create");

        let err = launch(&tasks, &services, &instance.id).expect_err("không có account");
        assert_eq!(err.code, codes::AUTH_FAILED);
        assert_eq!(err.action.as_deref(), Some("OPEN_ACCOUNTS"));
        assert_eq!(err.message, "No account selected");
        // lock phải đã nhả khi fail (finally parity) → lần sau retry được
        assert!(!root.join("instances").join(&instance.id).join(".lock").exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn non_vanilla_loader_resolves_via_loader_registry_not_rejected() {
        let root = std::env::temp_dir().join(format!("antares-launch-ld-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let services = Arc::new(AppServices::new(&root));
        let tasks = TaskRegistry::new();
        let instance = services
            .instances()
            .create("Fabric", "1.21.11", Some("fabric"), None, None)
            .expect("create");
        // có account để bước account pass
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::write(
            root.join("config/settings.json"),
            r#"{"accounts":[{"id":"a1","displayName":"Steve"}],"selectedAccount":"a1"}"#,
        )
        .unwrap();

        // Endpoint chết (offline-deterministic): fabric resolve không tới meta
        // + chưa cài → NETWORK_UNAVAILABLE — KHÔNG CÒN VALIDATION_FAILED "Batch 07c".
        let endpoints = install::Endpoints {
            fabric_meta: "http://127.0.0.1:1".into(),
            ..install::Endpoints::default()
        };
        let err = launch_with(&tasks, &services, &instance.id, &endpoints)
            .expect_err("fabric offline");
        assert_eq!(err.code, codes::NETWORK_UNAVAILABLE, "{}", err.message);
        assert!(!err.message.contains("Batch 07c"), "07c đã làm xong: {}", err.message);
        // task đã tạo ở bước 4 → failed khi resolve lỗi (không tồn tại active)
        assert!(tasks.active().is_empty(), "task phải failed khi resolve lỗi");
        // lock nhả khi fail (finally parity)
        assert!(!root.join("instances").join(&instance.id).join(".lock").exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_version_runs_install_pipeline_version_not_found() {
        let root = std::env::temp_dir().join(format!("antares-launch-nv-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let services = Arc::new(AppServices::new(&root));
        let tasks = TaskRegistry::new();
        let instance = services
            .instances()
            .create("Fresh", "1.21.11", None, None, None)
            .expect("create");
        std::fs::create_dir_all(root.join("config")).unwrap();
        std::fs::write(
            root.join("config/settings.json"),
            r#"{"accounts":[{"id":"a1","displayName":"Steve"}],"selectedAccount":"a1"}"#,
        )
        .unwrap();
        // Seed manifest cache FRESH (không network) nhưng vắng 1.21.11 →
        // install pipeline chạy rồi fail typed — parity MLL VersionNotFound →
        // legacy codes.MINECRAFT_VERSION_NOT_FOUND (không còn MC_VERSION_UNKNOWN).
        let cache = antares_net::ManifestCache::new(&root.join("cache"));
        cache.put(
            "mojang_manifest",
            &[antares_net::manifest::VersionEntry {
                id: "1.20.4".into(),
                r#type: "release".into(),
                release_time: "2023".into(),
                url: "".into(),
                sha1: "".into(),
            }],
        );

        let err = launch(&tasks, &services, &instance.id).expect_err("chưa cài version");
        assert_eq!(err.code, codes::MINECRAFT_VERSION_NOT_FOUND, "{}", err.message);
        assert!(err.message.contains("1.21.11"), "{}", err.message);
        // task đã tạo → failed (không active)
        assert!(tasks.active().is_empty());

        let _ = std::fs::remove_dir_all(&root);
    }

    // ---- Batch 07d — profile launch hint (parity _profile_launch_overrides) ----

    #[test]
    fn profile_launch_overrides_maps_hint_parity() {
        let hint = serde_json::json!({
            "instanceId": "i1",
            "server": "play.example.com",
            "port": 25565,
            "quickPlaySingleplayer": "world1",
            "quickPlayMultiplayer": "mp.example.com",
            "quickPlayRealms": "realm1",
            "customResolution": true,
            "resolutionWidth": "",
            "resolutionHeight": "720"
        });
        let out = profile_launch_overrides(&hint, "i1");
        assert_eq!(out.server.as_deref(), Some("play.example.com"));
        assert_eq!(out.port.as_deref(), Some("25565"), "number → str parity");
        assert_eq!(out.quick_play_singleplayer.as_deref(), Some("world1"));
        assert_eq!(out.quick_play_realms.as_deref(), Some("realm1"));
        assert!(out.custom_resolution);
        assert_eq!(out.resolution_width.as_deref(), Some("854"), "falsy → default");
        assert_eq!(out.resolution_height.as_deref(), Some("720"));
        // server + quickPlayMultiplayer xung đột → server thắng.
        assert_eq!(out.quick_play_multiplayer, None);
    }

    #[test]
    fn profile_launch_overrides_guard_falsy_and_shape_parity() {
        // Vé an toàn: khác instance đang launch → rỗng.
        let hint = serde_json::json!({"instanceId": "other", "server": "x"});
        assert_eq!(
            profile_launch_overrides(&hint, "i1"),
            ProfileOverrides::default()
        );
        // instanceId vắng/falsy → không guard (parity Python `if instance_id`).
        let hint = serde_json::json!({"server": "x"});
        assert_eq!(
            profile_launch_overrides(&hint, "i1").server.as_deref(),
            Some("x")
        );
        // falsy tất cả → rỗng (parity `if hint.get(k)`).
        let hint = serde_json::json!({
            "server": "",
            "port": 0,
            "customResolution": false,
            "quickPlaySingleplayer": [],
            "quickPlayRealms": null
        });
        assert_eq!(
            profile_launch_overrides(&hint, "i1"),
            ProfileOverrides::default()
        );
        // không phải dict → {} (parity isinstance check).
        assert_eq!(
            profile_launch_overrides(&serde_json::json!("x"), "i1"),
            ProfileOverrides::default()
        );
        assert_eq!(
            profile_launch_overrides(&serde_json::Value::Null, "i1"),
            ProfileOverrides::default()
        );
        // port LỒNG trong server — không server thì port bị bỏ.
        let hint = serde_json::json!({"port": 25565});
        assert_eq!(profile_launch_overrides(&hint, "i1").port, None);
    }

    #[test]
    fn read_launch_hint_reads_sidecar_state_and_tolerates_garbage() {
        let root = std::env::temp_dir().join(format!(
            "antares-launch-hint-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let services = Arc::new(AppServices::new(&root));
        let state = root.join("config/profiles-state.json");
        std::fs::create_dir_all(root.join("config")).unwrap();

        // vắng file → None (parity try/except).
        assert_eq!(read_launch_hint(&services), None);
        // corrupt → None.
        std::fs::write(&state, "{oops").unwrap();
        assert_eq!(read_launch_hint(&services), None);
        // version lệch → cả state coi rỗng (parity _load_state).
        std::fs::write(&state, r#"{"version":2,"launch":{"server":"x"}}"#).unwrap();
        assert_eq!(read_launch_hint(&services), None);
        // launch null / {} → None (parity `launch or None`).
        std::fs::write(&state, r#"{"version":1,"launch":null}"#).unwrap();
        assert_eq!(read_launch_hint(&services), None);
        std::fs::write(&state, r#"{"version":1,"launch":{}}"#).unwrap();
        assert_eq!(read_launch_hint(&services), None);
        // Hợp lệ — đúng shape sidecar legacy ghi khi apply profile.
        std::fs::write(
            &state,
            r#"{"version":1,"profiles":[],"applied":{},"lastRaw":null,
                "launch":{"instanceId":"i1","server":"play.example.com","port":25565}}"#,
        )
        .unwrap();
        let hint = read_launch_hint(&services).expect("có hint");
        assert_eq!(hint["server"], "play.example.com");
        // Tầng overrides đọc tiếp — instanceId khớp → server + port.
        let out = profile_launch_overrides(&hint, "i1");
        assert_eq!(out.server.as_deref(), Some("play.example.com"));
        assert_eq!(out.port.as_deref(), Some("25565"));

        let _ = std::fs::remove_dir_all(&root);
    }
}
