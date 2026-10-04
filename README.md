# Antares Launcher 4.0

Minecraft Control Center — launcher + instance manager + mod manager + resource editor +
optimization center + runtime monitor + diagnostics.

Kiến trúc đích: **Tauri 2 + Vue 3 + TypeScript (UI) · Rust (core) · Python legacy (bridge, temporary) · Fabric companion mod (runtime telemetry)**.

Xem `remake.md` cho master plan và `docs/migration/` cho trạng thái migration.

## Cấu trúc

```text
apps/desktop/      Vue 3 + TypeScript + Vite (UI mới)
src-tauri/         Tauri 2 shell (Rust)
crates/            Rust core workspace (app, storage, core, bridge, downloads, process,
                   java, launch, net, profiles, system, diagnostics, optimization,
                   mods, resources, visuals — 16 crates)
                   ⚠ Chỉ app/core/storage/bridge được link vào src-tauri shell hiện tại;
                   12 crate còn lại là migration substrate (unit test xanh, chưa
                   gọi từ app) — xem mục \"Tích hợp crate\" dưới đây.
legacy/python/     Legacy sidecar bridge (JSON Lines stdio)
companion/         Fabric companion mod (Minecraft runtime data)
api/ core/ infrastructure/ services/        Python legacy (giữ nguyên trong migration)
frontend/          Legacy pywebview UI (deprecated dần)
docs/              Protocol + migration docs
```

## Development (frontend mới)

```bash
pnpm install --dir apps/desktop        # hoặc cd apps/desktop && pnpm install
pnpm --filter @antares/desktop dev
pnpm --filter @antares/desktop typecheck
pnpm --filter @antares/desktop lint
pnpm --filter @antares/desktop test
pnpm --filter @antares/desktop build
```

## Python legacy (baseline)

```bash
python -m pytest tests -q    # 495 passed (2026-10-02; baseline lịch sử 254 — không được phá)
```

## Tích hợp crate (trạng thái trung thực)

`src-tauri` hiện phụ thuộc `antares-core`, `antares-storage`, `antares-bridge` và
`antares-app` (composition root F-14 — chỉ 4/16 crate). 12 crate còn lại (`downloads`,
`process`, `java`, `launch`, `net`, `profiles`, `system`, `diagnostics`, `optimization`,
`mods`, `resources`, `visuals`)**chưa được nối vào Tauri command surface** — code +
test chạy nhưng chưa phải runtime của app; các flow UI domain vẫn đi qua `legacy_call`
→ sidecar Python (command đầu tiên qua composition root: `app_storage_info`).

Lộ trình: nối từng nhóm command qua `antares-app` theo kế hoạch Batch 05–13 trong
kế hoạch no-Python (docs/architecture), sau đó mới gỡ `legacy_*` (Batch 14) và xoá
Python (Batch 15).

## Milestone hiện tại

- [x] M0 Baseline freeze (254/254)
- [x] M1 Source hygiene
- [x] M2 Frontend foundation (Vue 3 + TS strict + Vite + Pinia)
- [x] M3 Tauri shell scaffold
- [x] Batch 3 Typed API (envelope §96, error taxonomy §117, QoS pipeline §93)
- [x] M4 Rust core skeleton (AppState typed, EventHub QoS, TaskRegistry, StorageService)
- [x] M5 Migration bridge (JSON Lines sidecar, handshake/timeout/restart/shutdown)
- [x] M6 Core user flows (Dashboard/Play thật qua bridge: preflight, launch, offline mode)
- [x] Batch 14 phase 1–4 — Rust migration: crates `antares-downloads` (state machine
  §103, checksum, dedup; phase 2: sha1/sha256 + verify_file + atomic commit; phase 3:
  HTTP/1.1 engine thuần — redirect 10 lần Range không kế thừa, chunked, body cap 2GB;
  PartInfo `.antares-part` + meta; pipeline `download()` parity DownloadManager: cache
  → Range bytes=N- → verify → finalize, retry 8 + backoff min(2n,8), checksum mismatch
  không backoff, 200 thay 206 → restart; phase 4: sha incremental + verify streaming
  256KB/block, `get_stream` chunk 64KB, TLS seam),
  `antares-process` (supervisor §113, cleanup policy; phase 2: spawn/wait/stop thật,
  `CREATE_NO_WINDOW` Windows; LogMux §114/§115 merge timestamp + log scoped),
  `antares-java` (model §110, resolve order, parse version; phase 3: discovery scan
  dirs theo OS + JAVA_HOME/PATH + detect_major `java -showversion` parity discovery.py),
  `antares-launch` (session §112, preflight, argument builder; phase 2: JavaResolver +
  ArtifactResolver §104; phase 3: planner `required_java_major` §108 + `plan_launch`
  một bước; phase 4: ExitAnalyzer §116 không-LLM + weak-evidence → Unknown,
  CompanionPairing companion.json atomic parity),
  `antares-net` (rtt parity, PacketRing §122; phase 2: MC Server List Ping §119; phase
  3: tcp_check/probe cap 30/dns dedupe/check_endpoints parity handle_net_* + favicon
  parity; phase 4: Mojang manifest + DiskCache TTL 30 phút + stale fallback offline +
  download_size parity),
  `antares-profiles` (GAME_KEYS/coerce parity, diff §107; phase 2: write_options_merged
  parity; phase 4: ProfileStore CRUD qua antares-storage + sanitize/validate spec,
  crate được thêm vào workspace members) — kèm unit tests + parity checklist
  `docs/migration/PARITY.md` + CI job `cargo test --workspace` (2 OS: Ubuntu +
  Windows, exclude Tauri shell)
- [x] Batch 6 Profiles · 7a Mods · 7b Modpack/Detail · 8a Asset Library · 8b Resource
  Studio · 9 Visual Studio (+ Three.js totem) · 10 Optimization · 11 Network Lab ·
  12 Runtime/Packet · 13 Diagnostics — qua legacy bridge (chi tiết trong BASELINE.md)
- [x] no-Python Batch 01–04 (audit F-01…F-14): rustls TLS thật, URL parse + body cap
  hard, TaskRegistry fail + history cap, process tests cross-platform Windows +
  `CREATE_NO_WINDOW`, LogMux §114/§115 (merge timestamp + scoped log file),
  `check_endpoints` song song, composition root `antares-app` (AppServices + typed
  command errors §117 mirror TS ErrorCodes + command `app_storage_info` qua root)
- [x] no-Python M5 flag `ANTA_RUST_ONLY` (runtime, opt-in): bật → sidecar không
  auto-start (không cấu hình program/env Python), lệnh `legacy_start/call/restart/
  shutdown` trả typed `LEGACY_DISABLED` trước khi chạm bridge,  `core_status.rustOnly` expose cho UI — cổng chặn đầy đủ UI-side + “mọi tab chạy native” hoàn tất sau
  Batch 05–13 nối xong native commands
- [x] no-Python Batch 05 — group **instances native** (F-14): `InstanceStore` trong
  `antares-app` parity `InstanceService` (list/get/create + validate `is_safe_name`,
  7 subdir game, `instance.json` atomic; `select` ghi `selectedInstance` vào
  `config/settings.json` — cùng file sidecar ConfigManager) + 4 Tauri command
  `instances_*` + UI `flowsCommands` bỏ `legacy_call` cho instances (các flow khác
  vẫn bridge đến Batch 06+); health/app đã có native từ trước (`app_ping`,
  `app_storage_info`, `legacy_status`, `legacy_shutdown`)
- [x] no-Python Batch 06 — groups **accounts/java/versions/dashboard native**
  (B15.2): `AccountStore` trong `antares-app` parity `accounts.*` (list strip
  secret `token`/`_`-prefix, select → `AUTH_FAILED` "account not found: {id}") +
  5 Tauri command `accounts_list/accounts_select/java_list/versions_list/
  dashboard_summary` — contract sidecar giữ nguyên (PARITY evidence #3):
  `{accounts}`, `{javas}`, `{versions}`, dashboard `{appVersion, instanceCount,
  selectedInstanceId, recentInstanceId, recentInstanceName, account:{id,
  displayName}, legacyAvailable}`; loader lạ → `VALIDATION_FAILED`, manifest
  lỗi → `NETWORK_UNAVAILABLE`, `legacyAvailable = !rustOnly` + UI
  `flowsCommands` bỏ `legacy_call` cho 5 flow này (preflight/launch còn bridge
  đến Batch 07)
- [x] no-Python Batch 07a — group **play.preflight native**: service
  `AppServices::play_preflight` parity `handle_play_preflight` (đúng 5 check
  java/version/account/disk/mods + thứ tự + detail; `required_java_major`
  parity sidecar riêng rule với §108 planner; disk free native statvfs/
  GetDiskFreeSpaceExW §14 — không psutil; thiếu instance → `INSTANCE_NOT_FOUND`
  thêm cả 2 phía catalog) + Tauri command `play_preflight` + UI bỏ
  `legacy_call` cho preflight — **play.launch vẫn bridge chờ Batch 07b**
  (orchestrator native: version JSON → classpath → install pipeline → spawn)
- [x] no-Python Batch 07b — **play.launch native** (orchestrator parity
  `LaunchOrchestrator`): version JSON parser + rules parity MLL 8.0
  (`mcjson.rs`) + command builder byte-for-byte (`command.rs` — **A/B golden**
  `tests/mll_parity.rs` khớp `minecraft_launcher_lib.get_minecraft_command`
  thật, script `tests/parity/gen_launch_command_golden.py`) + `JvmConfig`
  parity jvm.py (GC auto/g1/balanced + `_low_end` probe native §14) +
  ResourceLock parity locks.py (pid + stale steal) + natives extract
  (zip/deflate qua antares-mods, chặn traversal §50) + spawn qua
  `spawn_with_secrets` (token mask khỏi fingerprint §26) + LogMux scoped +
  task LAUNCH (complete sau spawn) + `required_java_major` fix snapshot → 21 —
  lệnh native chơi được instance vanilla ĐÃ cài (B07c đã nối install)
- [x] no-Python Batch 07c — **install pipeline + loader fabric/forge**:
  `antares-app::install` parity MLL (`install.py`/`fabric.py`/`forge.py`/
  `_helper.inherit_json`): version json (manifest cache 30' + stale offline,
  mã `MINECRAFT_VERSION_NOT_FOUND`) → libraries (rules/artifact/Maven parity
  + natives extract, nuốt lỗi Maven optional như `try/except: pass`) →
  assets content-addressed → logging → client jar (copy parent cho version
  kế thừa); `inheritsFrom` merge (`inherit_json` + `resolve_version_json`)
  trong `mcjson.rs`; **fabric** = fetch profile JSON thẳng từ meta
  (`/v2/versions/loader/{mc}/{lv}/profile/json` — bỏ `java -jar` installer);
  **forge** = installer jar → `install_profile.json` → profile libs → extract
  `version.json`/universal/client.lzma → 6 processors client (parity var
  `{VAR}` + unwrap `[maven]`, Main-Class từ MANIFEST, exit≠0 →
  `LOADER_INSTALL_FAILED` — mạnh hơn MLL bỏ qua exit code; <1.13 → báo rõ
  không tự động); `play_launch` **auto-install khi version chưa cài** (task
  LAUNCH progress, parity orchestrator step 3) + command mới `play_install`
  (task `INSTALL` chạy nền, contract `{taskId}`) + `versions_list` fabric
  (stable MC) / forge (maven metadata); codes mới
  `MINECRAFT_VERSION_NOT_FOUND`/`LOADER_INSTALL_FAILED` (cả 2 phía catalog)
- [x] no-Python Batch 07d — **Mojang JRE runtime + profile launch hints**:
  `antares-app::mojang_runtime` parity MLL `runtime.py` (manifest java-runtime
  `2ec0cc96…all.json`, platform string theo OS/arch, tải **lzma ~2.9MB →
  decompress + verify sha1 raw** parity `download_java_runtime` — lzma-rs;
  file/dir/link manifest, `.version` + `{component}.sha1` sidecar, chmod +x,
  symlink, path escape → `VALIDATION_FAILED`, component thiếu/platform trống →
  `Ok(None)` nuốt như legacy) + `resolve_java_for` parity `_resolve_java` 3
  bước (1 system → 2 probe runtime đã cài + `detect_major` → 3 cài Mojang
  component theo `javaVersion.major`, source `JavaSource::Mojang`) +
  **profile launch hints** parity `_profile_launch_overrides` (đọc
  `config/profiles-state.json` key `launch` version 1 + instanceId guard →
  server/port, quickPlay, customResolution 854×480 falsy-default) →
  `LaunchHints` trong `antares-launch::command` (`--server/--port` sau game
  args, `--width/--height` path `minecraftArguments` pre-1.13, feature rules
  theo launch options parity `parse_single_rule` — library rules luôn
  `options={}`) + fix step 6 `resolve_version_json` (fabric/forge
  `inheritsFrom` không còn bị reject); codes dùng lại (không mới):
  `JAVA_NOT_FOUND`/`OPEN_JAVA_SETTINGS`/`NETWORK_UNAVAILABLE`;
  **deferred**: MINECRAFT_* events
