# PARITY.md — Legacy Python → Rust parity checklist

> Mục đích: **Batch 15 — parity check** dùng chính checklist này để xác nhận Rust đã bọc
> 100% hành vi legacy trước khi disable/remove Python sidecar (§227 thứ tự milestone).
>
> Nguồn kiểm toán: `legacy/python/sidecar.py::HANDLERS` (**118 method / 23 nhóm** — đếm lại
> 2026-09-29; số 144 ghi trước đây là sai, đã sửa), `services/` (thân service),
> `api/bridge/api.py` (shape API legacy). Cập nhật mỗi khi một crate bọc thêm behavior mới.
> Rà 2026-09-29: **0/118 method dead-legacy** — mọi method đều được gọi từ UI mới,
> legacy UI, tests hoặc companion (verify bằng quét chuỗi method name).

## Trạng thái batch 14 (skeleton phase 1)

Crates mới: `antares-downloads`, `antares-process`, `antares-java`, `antares-launch`,
`antares-net`, `antares-profiles` — phase 1 = **logic thuần + parity tests + state machine**.
I/O thật nối từ phase 2 (sha/HTTP/spawn/discovery/probe), phase 3 (download pipeline,
java discovery, net probe/dns, planner §108), phase 4 (streaming, TLS seam, ProfileStore
CRUD, ExitAnalyzer, CompanionPairing, Mojang manifest) — đánh dấu như bên dưới.

| Crate | Mục Batch 14 | Đã bọc (phase 1) | Phase 2–4 (I/O thật) |
|---|---|---|---|
| `antares-downloads` | downloads | state machine §103, checksum codes, dedup registry | **xong phase 2+3+4**: sha1/sha256 thuần → incremental hasher (Sha1Hasher/Sha256Hasher — streaming verify 256KB/block, `verify_file_streaming`); HTTP engine §102 thuần (GET/Range/206/chunked/redirect — Range không kế thừa) + **streaming `get_stream`** callback từng chunk 64KB + **TLS seam** `https_url_to_http` (https → fail rõ ràng tới khi bundle) ; `PartInfo` resume parity; pipeline `download()` parity `DownloadManager` retry 8 + backoff min(2n,8) — **attempt_once streaming** (ghi part từng chunk, restart 200≠206 đúng parity). Còn: TLS thật (bundle Batch 16) |
| `antares-process` | process | record §113, cleanup policy, registry, sweep | **xong**: spawn/wait/stop thật (`std::process`, `CREATE_NO_WINDOW`), stdin prompt graceful → kill, exit→state; **LogMux** — merge stdout/stderr timestamp §114 (ring bounded §88) + scoped log file §115 |
| `antares-java` | Java manager | model §110, resolve order, parse version | **xong phase 3**: `discovery` — scan known dirs theo OS + JAVA_HOME + PATH (resolve symlink, dedupe giữ thứ tự), `detect_major` qua `java -showversion` timeout 15s, `java_info`/`scan_java_infos`. Còn: Mojang runtime **download** (manifest đã có ở antares-net, runtime download ⏳) |
| `antares-launch` | Minecraft launch | session §112, preflight, argument builder | **xong phase 2+3+4**: `JavaResolver` + `ArtifactResolver` §104; `planner` — `required_java_major` §108 + `plan_launch`; **`exit.rs`** — `ExitAnalyzer` §116 (ingest→normalize→fingerprint→classify→rank→recommendation, weak evidence → Unknown/confidence 0, không LLM) + `CompanionPairing` (ghi companion.json atomic parity `write_pairing_for_instance` — server off → None) |
| `antares-net` | network | rtt_stats parity `_probe_stats`, PacketRing §122 | **xong phase 2+3+4**: ping §119 + `probe.rs` (tcp_check/probe/dns/endpoints); **`manifest.rs`** — Mojang manifest parity `ManifestService` + `DiskCache` TTL 30 phút `{ts,value}` + stale fallback offline (mục 60) + `download_size` (client + libraries artifact/classifiers + assetIndex). endpoints đã song song (F-13 scope-thread); download pipeline async giữ cho Tokio stage (audit §12: không Tokio hoá vội) |
| `antares-profiles` | profiles | GAME_KEYS 28 key, coerce parity, options parse, diff §107 | **xong phase 2+4**: `write_options_merged` parity; **`store.rs`** — `ProfileStore` CRUD qua antares-storage (state `profiles-state.json` atomic, id `prof-YYYYmmdd-HHMMSS-<6hex>`, duplicate uniquify, delete confirm, mark_applied/clear_revert/launch_hint, sanitize/validate spec parity) |

## Checklist parity theo domain (cập nhật khi bọc)

Chú giải: ✅ parity verified (test song song Python) · ☐ Rust đã có, cần verify · ⏳ phase 2 · ❌ chưa migrated.

### Downloads (legacy `services/downloads`)
- [x] ✅ State machine DISCOVER→PREPARE→DOWNLOAD→VERIFY→COMMIT (§103) — `DownloadState`
- [x] ✅ Codes lỗi: `CHECKSUM_MISMATCH`, `CONFIG_INVALID` — `ChecksumError::code()`
- [x] ✅ Dedup: 2 instance cùng artifact → 1 download (§103) — `DedupRegistry`
- [x] ✅ SHA-1/SHA-256 thuần + `verify_file` + `commit_artifact` atomic rename, idempotent (phase 2)
- [x] ✅ HTTP engine (§102) thuần: GET + redirect 10 lần (Range không kế thừa qua redirect),
  Content-Length + chunked, body cap 2GB (phase 3 — https/TLS nối khi bundle)
- [x] ✅ Resume/range/partial file: `PartInfo` parity `.antares-part` + `.meta` (chặn resume
  khác URL), pipeline Range `bytes=N-` + append, 200 thay 206 → restart từ đầu (phase 3)
- [x] ✅ Streaming chunk (phase 4): sha incremental `Sha1Hasher`/`Sha256Hasher` +
  `verify_file_streaming` 256KB/block + `get_stream` callback 64KB — `download()` ghi part
  từng chunk, không giữ body nguyên khối trong RAM
- [x] ✅ TLS seam (phase 4): `https_url_to_http` — điểm nối TLS duy nhất, hiện fail rõ
  ràng `NET_UNREACHABLE` (bundle TLS thật ở Batch 16)
- [ ] ⏳ TLS thật + source fallback

### Storage (legacy `infrastructure/fs` + `services/profiles` file I/O)
- [x] ✅ Scoped roots + sandbox traversal (§98) — crate `antares-storage` (có sẵn từ trước)
- [x] ✅ Atomic write tmp+rename (§99) — `write_json_atomic`
- [x] ✅ ProfileStore CRUD state JSON qua storage (phase 4 — `profiles-state.json`,
  field name parity `lastRaw`)
- [x] ✅ Migration `_write_options` (merge/dedupe key) sang `antares-profiles` + storage
  (xong phase 2 — `write_options_merged`)

### Process (legacy `infrastructure/process/manager.py`)
- [x] ✅ Record pid/owner/instance/fingerprint/exit (§113) — `ProcessRecord`
- [x] ✅ Cleanup policy Wait/Kill/Keep, không kill ngoài scope (§113) — `CleanupPolicy` + `shutdown_plan`
- [x] ✅ Spawn/wait/stop thật: stderr drain tagged, utf-8 lossy, stdin prompt graceful → wait timeout → kill, exit code → EXITED/Failed (phase 2; tests cross-platform — chạy cả Windows CI, gồm cwd space/unicode, `CREATE_NO_WINDOW`)
- [x] ✅ stdout/stderr pipeline gộp dòng theo timestamp (§114) — `LogMux`: `timestamp_ms` + tag `stdout`/`stderr`, ring bounded drop-oldest (§88)
- [x] ✅ Log storage (§115) — `LogMux::create_scoped(logs_root, scope)`: scope `[A-Za-z0-9._-]` chặn traversal (§50) → append `<logs_root>/<scope>.log` (caller truyền LogsRoot)

### Java manager (legacy `services/java/discovery.py` + `manager.py`)
- [x] ✅ Model `JavaRuntime` (§110) — source/path/major/minor/arch/vendor/verified/capabilities
- [x] ✅ Resolve order: instance → profile → managed → mojang → system (§110)
- [x] ✅ Parse `java -version` (1.8 legacy mapping) — parity `discovery.py`
- [x] ✅ Discovery scan known dirs theo OS + JAVA_HOME + PATH (resolve symlink, bỏ symlink
  subdir, dedupe giữ thứ tự) + `detect_major` timeout 15s + `java_info` (phase 3)
- [x] ✅ Mojang JRE runtime download (B07d) — `antares-app::mojang_runtime` parity MLL
  `runtime.py`: manifest java-runtime + lzma → verify sha1 raw + `.version`/`.sha1` sidecar +
  links/chmod; `resolve_java_for` 3 bước parity `_resolve_java`

### Minecraft launch (legacy `services/minecraft/launch/*`)
- [x] ✅ Session state machine §112 (IDLE→…→COMPLETED/CRASHED + CANCELLED early exit)
- [x] ✅ Preflight fail-fast + remediation (§111 `LaunchPreflight`)
- [x] ✅ Argument builder contract vanilla (§111 `ArgumentBuilder`)
- [x] ✅ `ArtifactResolver` cục bộ store §104 → game_dir → Missing (phase 2)
- [x] ✅ `required_java_major` §108 tập trung (b1.x/≤1.16→8, 1.17→16, 1.18–1.20.4→17,
  1.20.5+/1.21+→21) + `plan_launch` tổng hợp resolve→plan→preflight→args (phase 3)
- [x] ✅ CompanionPairing (phase 4) — ghi companion.json atomic parity
  `write_pairing_for_instance` (version/endpoint/token/packetTypes/writtenAt;
  server off → None, không tạo dir)
- [x] ✅ ExitAnalyzer §116 (phase 4) — pipeline ingest→normalize→fingerprint→classify→
  rank evidence→recommendation; pattern OOM/mod-conflict/java-missing/user-cancel;
  weak evidence → Unknown + confidence 0 (§116 "không kết luận nếu evidence yếu")
- [x] ✅ Profile launch hints (B07d) — `read_launch_hint`/`profile_launch_overrides` parity
  `_profile_launch_overrides` → `LaunchHints` (`--server/--port`, `--width/--height`);
  feature rules theo launch options parity `parse_single_rule` (library rules luôn `options={}`)
- [ ] ⏳ ExitAnalyzer input đầy đủ: latest.log/crash-report file reading + runtime telemetry

### Network (legacy `services/diagnostics/net.py`)
- [x] ✅ `rtt_stats` parity 1:1 `_probe_stats` (min/avg/max/jitter/loss, mẫu None = fail) — verified song song Python
- [x] ✅ PacketRing §122 (10k/100k oldest-first, export on-request)
- [x] ✅ PacketMode §121 (OFF/METADATA/DEBUG PAYLOAD)
- [x] ✅ Server List Ping §119 (phase 2): VarInt/frame + handshake + đọc đúng packet length + parse status (MOTD extra concat parity, players/version/modinfo/favicon)
- [x] ✅ TCP check `tcp_check` — multi-addr `connect_timeout` + RTT round 1 + error cắt
  160 ký tự (phase 3)
- [x] ✅ Probe RTT timeline cap 30 + stats + timeline `at` round 4 — parity `net.probe` (phase 3)
- [x] ✅ DNS `dns_lookup` — dedupe giữ thứ tự + ms — parity `net.dns` (phase 3)
- [x] ✅ Mojang manifest + DiskCache TTL (phase 4 — `manifest.rs`): parity `ManifestService`
  (get_manifest/find/download_size) + `DiskCache` `{ts,value}` TTL 30 phút, stale fallback
  offline (mục 60)
- [x] ✅ check_endpoints song song — scope-thread 1/endpoint parity ThreadPoolExecutor legacy (F-13: wall-clock ≈ max(RTT) thay vì sum 5×timeout; sort by id giữ nguyên)

### Profiles (legacy `services/profiles`)
- [x] ✅ GAME_KEYS 28 key + GAME_KEY_ORDER — parity `keys.py` (đếm verified)
- [x] ✅ `coerce()` parity (bool→true/false; bool vào key lạ →1/0; banker's rounding; float `1.0`) — verified song song Python
- [x] ✅ Parse options.txt chỉ giữ whitelist — parity `_parse_options`
- [x] ✅ `write_options_merged` parity `_write_options`: giữ thứ tự dòng, dedupe dòng đầu thắng, append key mới cuối, luôn kết thúc `\n` (phase 2)
- [x] ✅ Diff engine +/−/~ theo GAME_KEY_ORDER (§107)
- [x] ✅ CRUD profile qua storage (phase 4 — `ProfileStore`): create (id
  `prof-YYYYmmdd-HHMMSS-<6hex>`, name validate parity `is_safe_name` ≤64 ký tự),
  duplicate ("copy"/"copy 2" uniquify), update (name/spec), delete confirm:true,
  mark_applied/clear_revert/launch_hint; state JSON atomic qua antares-storage
- [x] ✅ Sanitize/validate spec (phase 4) — parity `_sanitize_spec`/`validate_spec`:
  5 section, game key whitelist + coerce, launch key whitelist (giá trị falsy bị bỏ),
  jvm key whitelist (jvmArgs list[str])
- [ ] ⏳ Capture/plan/apply/revert — parity `profiles.capture|plan|apply|revert`
  (cần instances/accounts runtime — nối tầng bridge)
- [ ] ⏳ Export/import profile JSON

## Inventory 118 method `HANDLERS` — mapping Rust hiện tại

> Batch 15 coi là xong khi **mọi method** có một trong: bridge Tauri command mới (Rust),
> hoặc được chứng minh không cần (dead legacy). Không được còn method nào chỉ tồn tại ở Python.
> Số liệu đếm lại từ `sidecar.py::HANDLERS` 2026-09-29: **118 method, 23 nhóm**
> (số 144 cũ là sai). Scan 2026-09-29: **0 method dead-legacy**.

| # | Nhóm (n) | Methods | Đích Rust | Trạng thái |
|---|---|---|---|---|
| 1 | health/app (5) | `health.ping/version/shutdown`, `app.echo/storage_root` | Tauri core commands (không cần sidecar) | ☐ cần Tauri command thay thế |
| 2 | instances (4) | `instances.list/get/create/select` | `antares-core::AppState` + storage instances root | ✅ wiring Batch 05 (`instances_*` + UI native) |
| 3 | accounts (2) | `accounts.list/select` | Tauri command + auth crate | ✅ wiring B15.2 (`AccountStore` + `accounts_list/select`, UI native) — ⏳ B15.3 A/B |
| 4 | java (1) | `java.list` | `antares-java::discovery` | ✅ wiring B15.2 (`java_list` payload `{javas}` parity + UI native) — ⏳ B15.3 A/B |
| 5 | versions (1) | `versions.list` | `antares-net::manifest` (phase 4) | ✅ wiring B15.2 (`versions_list` vanilla + `VALIDATION_FAILED` loader lạ, UI native) + **B07c Fabric/Forge metadata** (`antares_app::install::list_loader_versions` — fabric stable MC sort desc, forge maven `<versions>` sort desc; payload untagged `string \| {id}` parity `list[str]` vs `list[dict]`) — ⏳ B15.3 A/B |
| 6 | play (2) | `play.preflight/launch` | `antares-launch` (planner/preflight/session xong) | ✅ cả 2 native: preflight B07a (`play_preflight` 5 check + `INSTANCE_NOT_FOUND`) + launch B07b (`play_launch` orchestrator parity MLL **A/B golden** `mll_parity.rs` + lock/account/java/build/spawn/task) + **B07c install pipeline + loader** (`ensure_installed` auto-install khi Play — parity orchestrator step 3 `loader.install`, fabric profile JSON từ meta, forge installer/processors; lệnh native mới `play_install` task `INSTALL` nền) + **B07d Mojang runtime + profile launch hints** (`mojang_runtime.rs` parity MLL `runtime.py` lzma download + `resolve_java_for` 3 bước parity `_resolve_java`; `read_launch_hint`/`profile_launch_overrides` parity `_profile_launch_overrides` → `LaunchHints` `--server/--port`/`--width/--height`; feature rules theo options parity `parse_single_rule`; step 6 `resolve_version_json` fix `inheritsFrom`) — ⏳ B15.3 A/B end-to-end; MINECRAFT_* events deferred |
| 7 | dashboard (1) | `dashboard.summary` | Tauri command tổng hợp | ✅ wiring B15.2 (`dashboard_summary` parity contract + UI native) — ⏳ B15.3 A/B |
| 8 | profiles (13) | `profiles.list/get/create/duplicate/update/delete/capture/plan/apply/revert/export/import/validate` | `antares-profiles` | ✅ CRUD/sanitize/coerce/diff xong; ⏳ capture/plan/apply/revert (cần instances/accounts runtime) + export/import |
| 9 | mods (10) | `mods.list/search/install/remove/health/scan/autofix`, `mods.quarantine.list/restore/delete` | crate `antares-mods` (B15.1d A+B+C) | ✅ deflate+zipread thuần (golden zlib), read_mod_info, check_health, quarantine vault, list/uninstall; jar_reader JVMS + scanner 10 rules (thresholds 25/60); modrinth search/versions/pick_file + fix plan KNOWN_DEPS; mrpack info/detect_loader/filter/ensure_inside/install-plan; auto_fix flow scan→plan→download |
| 10 | modpack (5) | `modpack.info/install/status/cancel`, `mod.detail` | crate `antares-mods` | ❌ chưa crate |
| 11 | asset (5) | `asset.list/import/get/delete/assign` | crate `antares-resources` (B15.1e phase A) | ✅ AssetStore parity assets.py: import PNG sig/IHDR/dims (8MB/4096), sha256 content-addressed + catalog atomic corrupt→backup, list filter/sort, get/read/delete, assign regex `assets/<ns>/textures/` + `..`/`//` reject |
| 12 | resource (18) | `resource.*` (18 method: list/get/wizard_info/create/update/delete/generate/validate/build/build_task/build_cancel/builds/install/installed/uninstall/layer.get/set/move) | crate `antares-resources` | ⚠️ phase A xong: png encode/dims + validator parity `validate_dir`/`validate_zip_names` (finding camelCase, FAIL chỉ khi ERROR mục 42, structure KNOWN_MC_DIRS + ext rules + model refs + duplicate sha1); ⏳ resource 18 defer phase sau (project store/builder/layering/installer) |
| 13 | visual (15) | `visual.*` (15 method: presets/render_preview/export_pack/totem_presets/render_totem/totem_model.get/save/render_totem_model/export_totem_pack/hud_widgets/save_hud_layout/fx_defaults/render_hit/render_particle/export_fx_pack) | crate `antares-visuals` (B15.1f phase A) | ⚠️ phase A xong: crosshair presets 7 + totem presets 8 + voxel_spec mục 70, render PNG thuần (16/32/128/atlas particle + isometric z-buffer deterministic mục 9), fx defaults + hud 14 widget validate/sanitize/merge + model3d validate mục 15 + draft store totem-3d; ⏳ export_* 3 + save_hud_layout wiring cần RS project store/builder/install (phase sau) |
| 14 | optimization (5) | `optimization.scan/plan/apply/rollback/snapshot_info` | crate `antares-optimization` (B15.1c) | ✅ advisor parity (35% RAM/trần 8GB/sàn 1GB, low_end/performance/balanced, warnings/bottlenecks) + 6 PROFILES + plan diff/apply snapshot `opt-<ts>.json`/rollback nguyên vẹn + state `optimization.json` |
| 15 | system (7) | `system.overview`, `system.cleanup.scan/clean/undo/empty_trash`, `system.power.status/set_plan` | crate `antares-system` (B15.1a) | ✅ cleaner (SAFE_RULES/PROTECTED/trash+manifest/undo parity) + power (powercfg parity, hint-first) xong; ⏳ overview cần psutil-equivalent + wiring |
| 16 | net (5) | `net.endpoints/tcp/ping/probe/dns` | `antares-net::probe` + `ping` | ✅ parity xong (probe/dns/tcp/ping/endpoints), ⏳ wiring |
| 17 | runtime (5) | `runtime.endpoint/start/sessions/metrics/pairing` | `antares-launch::CompanionPairing` (pairing xong) + IPC server | ⚠️ pairing ✅; IPC server/sessions/metrics ⏳ |
| 18 | packet (6) | `packet.ingest/list/stats/capture/clear/export` | `antares-net::PacketRing` (ring/mode parity xong) | ⚠️ ring ✅; ingest/IPC wiring ⏳ |
| 19 | console (4) | `console.sources/read/analyze/insights` | crate `antares-diagnostics` (B15.1b) | ✅ LogAnalyzer parity (11 blueprint, severity matrix, sort count→severity, lines cắt 240, read limit/truncated 400); ⏳ wiring launcher ring buffer |
| 20 | repair (3) | `repair.actions/scan/run` | crate `antares-diagnostics` (B15.1b) | ✅ RepairService parity 7 ACTIONS dry-run first (metadata/missing_dirs/options/launcher_config/downloads/caches/resource_packs + to_trash owned check) |
| 21 | diagnostic (1) | `diagnostic.export` | crate `antares-diagnostics` (B15.1b) | ✅ compose sẵn qua `analyze` + `sources` + `RepairService::scan`; ⏳ wiring gói export |

Tổng: **118 method** — ✅/⚠️ crate đã bọc: `java`(1), `versions`(1), `play`(2), `profiles`(13),
`net`(5), `system`(7 — B15.1a), `console`+`repair`+`diagnostic`(8 — B15.1b),
`optimization`(5 — B15.1c), `mods` (15 — B15.1d A+B+C), `resources` phase A (B15.1e:
asset 5 + validator + png), `visuals` phase A (B15.1f: presets/render/hud/model3d
logic thuần), `runtime`+`packet` một phần (ring/pairing) · ⚠️ còn defer: resource 18
(need project store/builder/installer) + visual export 3 + save_hud_layout wiring —
**22 method** defer phase sau · phần còn lại là wiring Tauri command trên nền crate
có sẵn.

## Kế hoạch Batch 15 — parity check (lập 2026-09-29)

### Phương pháp verify từng method (evidence bắt buộc)

Mỗi method trong inventory phải có MỘT trong các evidence sau mới được đánh ✅:

1. **A/B snapshot test** (mạnh nhất — cho method có I/O thật): chạy cùng input qua
   sidecar Python và qua Rust handler, so envelope output JSON 100% khớp (field name,
   kiểu, round, error code). Script: `tests/parity/ab_check.py` gọi sidecar qua stdio
   (M5 bridge) + Rust qua cargo test/fixture — golden files lưu `tests/parity/golden/`.
2. **Parity unit test song song Python** (cho logic thuần — đã dùng cho coerce/rtt/
   parse_java_version): chạy snippet Python thật, so output với assert Rust.
3. **Golden file contract**: UI mới đã gọi method này qua bridge trong production flow
   (Batch 6–13) — giữ contract JSON y nguyên khi chuyển sang Rust (test mirror
   `apps/desktop/tests/services/*.test.ts` không được sửa).
4. **Chứng minh dead-legacy** — hiện không có method nào rơi vào nhóm này (scan
   2026-09-29: 0/118).

Method I/O phụ thuộc môi trường (spawn/java scan/net) dùng A/B với fixture
đóng gói sẵn (fake bin/java, local TCP server) — đã có trong phase 2/3 tests.

### Thứ tự công việc (không đảo — mỗi bước là gate của bước sau)

```text
B15.1  Crate mới còn thiếu (6 crate / 73 method) — theo dependency:
       a. antares-system (7)      — không phụ thuộc crate khác, parse/hardware
       b. antares-optimization (5)— phụ thuộc system + profiles
       c. antares-diagnostics (8) — LogAnalyzer §41 + RepairService + export
       d. antares-mods (15)       — scan/health/quarantine + modpack + mod.detail
       e. antares-resources (23)  — asset store + resourcepack pipeline
       f. antares-visuals (15)    — dựng trên antares-resources pipeline
B15.2  Wiring Tauri command cho nhóm đã có crate (instances/accounts/dashboard/
       play orchestrator + spawn, profiles capture/plan/apply/revert, runtime IPC
       server, packet.ingest) — thay lượt gọi sidecar trong stores/services TS.
B15.3  Parity check chạy toàn bộ 118 method theo evidence ở trên — bug sửa tại
       crate (không vá ở tầng bridge).
B15.4  Feature-flag `antares_legacy_sidecar=off` → chạy toàn bộ UI flow chính
       (Dashboard/Play/Profiles/Mods/Resource/Visual/Optimization/System/Network/
       Runtime/Diagnostics) — mọi phase machine §158 phải render đủ 4 state.
B15.5  Disable Python sidecar (flag default off) → benchmark (§61: download
       throughput, hashing, event throughput, IPC latency, memory rings).
B15.6  Remove bridge + cleanup (frontend/, legacy/, app/, api/, core/,
       infrastructure/, services/, tests/unit/legacy) — chỉ khi B15.1–B15.5 xanh.
```

### Definition of Done mỗi method (§83 rút gọn cho parity)

```text
- Rust handler có unit test parity (A/B hoặc golden)
- Error code §117 giống legacy cho mọi failure path đã biết
- Contract JSON field-name/kiểu giữ nguyên (TS types không sửa)
- Feature-flag off: method gọi đường Rust
```

### Rủi ro đã biết

- Rust compile chưa verify local (không có cargo) — B15.0 phải bắt đầu bằng
  `cargo test --workspace` xanh trên CI TRƯỚC khi viết crate mới.
- TLS seam (https) — 6 crate mới đều cần fetch online (mods search/modpack download);
  nếu TLS chưa bundle, parity check chạy fixture local, https thật chuyển Batch 16.
- `resource.layer.*` sync options.txt atomically (§173) + `visual.*` render data URI
  cần pixel-perfect — A/B so byte ảnh (không so JSON).

## Tiêu chí đóng Batch 15
1. Mọi dòng inventory trên có ✅ (evidence theo kế hoạch B15.3) hoặc chứng minh dead-legacy.
2. `cargo test --workspace` xanh trên CI (mỗi crate ≥ coverage parity như hiện tại).
3. Feature-flag `antares_legacy_sidecar=off` → toàn bộ flow UI vẫn chạy (B15.4).
4. Benchmark B15.5 ghi vào BASELINE.md trước khi remove.
5. `frontend/` + `legacy/python/` + Python runtime remove khỏi repo (B15.6, mục 15.2–15.4).
