# Migration Baseline — Antares 4.0

Ghi lại theo Milestone 0 (Baseline Freeze) của remake.md.

## Test baseline (Python legacy)

```text
Command:    python -m pytest tests -q
Kết quả:    254 passed in 5.64s
Môi trường: Python 3.14.2, Codespaces Linux
Ngày:       2026-09-27
```

Không được phá baseline này trong toàn bộ quá trình migration (§87.1, §190).

## Audit số liệu (từ remake.md §87.1)

```text
Python files                178
Frontend JS files            51
Java companion files          3
```

## Milestone 1 — Source hygiene (đã hoàn thành)

- [x] Xoá toàn bộ `__pycache__/` (175 file .pyc)
- [x] Xoá `dist/Antares-1.0.0-source.zip` khỏi source tree
- [x] Xoá `.pytest_cache/`
- [x] Thêm `.gitignore` theo §87.2
- [x] Baseline test vẫn 254/254 sau khi dọn

## Milestone 2 — Frontend foundation (đã hoàn thành)

```text
Stack:  Vue 3.5 + TypeScript 5.9 strict + Vite 7 + Pinia 3 + Vue Router 4
Path:   apps/desktop/
```

Gates đạt được:

```text
vue-tsc (strict):        PASS
eslint:                  0 errors
vitest:                  5/5 passed
vite build:              PASS (chunk tách: ui-shell + feature chunks lazy)
```

Đã có:

- Design tokens (palette, motion, glass budget) — `src/styles/tokens.css`
- App shell: topbar / sidebar (expanded-compact) / statusbar / splash
- Router với lazy loading feature routes + catch-all 404
- Stores: session, notifications (spam guard §171), settings
- Command palette Ctrl+K (skeleton)
- Toast stack với auto-dismiss
- Typed contract placeholder (§96)

## Milestone 3 — Tauri 2 shell (đã hoàn thành scaffold)

```text
Path:   src-tauri/
```

- `tauri.conf.json`: frontendDist trỏ `apps/desktop/dist`, devUrl Vite 5173
- Capability `main-window.json` scoped, không wildcard (§148)
- Commands skeleton: `app_ping`, `app_version`, `app_storage_mode` (§88.2 style — mỗi domain một file)
- `StorageMode` detect theo §218

Lưu ý: Rust toolchain chưa có trong Codespace hiện tại — build Tauri sẽ chạy trên
Windows runner (Milestone 3 CI) hoặc sau khi cài rustup.

## Batch 3 — Typed API (đã hoàn thành)

```text
Rust:  src-tauri/src/protocol/{error,response,event}.rs
TS:    apps/desktop/src/types/{protocol,commands}.ts
Svc:   services/{ipc,eventIngestion,appCommands}.ts
Docs:  docs/protocol/README.md
```

- Response envelope §96: mọi command trả `{ ok, data, error, warnings }` — không raw stack trace
- Error taxonomy §117: catalog codes tập trung 2 phía (APP_INTERNAL, INSTANCE_LOCKED, JAVA_NOT_FOUND, ...)
- Event envelope §88.3: topic + QoS (latest/coalesce/batched/lossless) + schema version, reject an toàn khi lệch schema (§31)
- Event ingestion pipeline §93: QoS buffers → requestAnimationFrame flush — raw event rate ≠ UI update rate
- Typed ipc layer: `invokeCommand` (throw IpcError) / `requestCommand` (envelope) + browser fallback cho dev ngoài Tauri
- Event bridge: `antares://event` → ingestion singleton (App.vue)
- Gates: vue-tsc ✓ · eslint 0 errors ✓ · vitest 24/24 ✓ · build ✓ · pytest baseline 254/254 ✓

Lưu ý: Rust side chưa compile được trong Codespace này (thiếu cargo) —
`cargo test` cho protocol tests chạy trên CI Windows/linux khi cài rustup.

## Milestone 4 — Rust Core (đã hoàn thành skeleton)

```text
Cargo workspace:
├── crates/antares-storage   §98 Storage Layer 2.0
│   ├── ScopedRoot (AppData/Cache/Logs/Profiles/Instances/Backups)
│   ├── StorageService/StorageHandle: read_json, write_json_atomic,
│   │   remove_safe, list_scoped
│   └── Path sandbox §50: reject traversal/absolute/parent-dir
└── crates/antares-core      §88.1 typed AppState
    ├── events.rs  EventHub §32: bounded queues (latest 256, lossless 1024,
    │   coalesce cap 4096 keys), drop-oldest, stats()
    ├── tasks.rs   TaskRegistry §90-§91: state machine (QUEUED→RUNNING→
    │   COMPLETED/FAILED/CANCELLED qua CANCEL_REQUESTED), priority P0-P3,
    │   dedupe §169, prune_finished
    ├── app_state.rs  AppState: events + tasks + storage, read_config/
    │   write_config atomic qua scoped root
    └── error.rs   CoreError → taxonomy codes

src-tauri:
├── state/core_state.rs  Managed state + storage root resolve (§218 portable/installed)
├── events/bridge.rs     Hub drain (50ms) → emit batch lên 'antares://event'
└── commands/core_state.rs  core_status, core_spawn_task, core_task_progress,
    core_complete_task, core_cancel_task — envelope §96

TS:
├── types/commands.ts    CoreTask/CoreHubStats/CoreStatusPayload mirrors
├── services/coreCommands.ts  typed helpers
└── AppStatusbar         core chip + hub dropped warning
```

Gates: vue-tsc ✓ · eslint ✓ · vitest 28/28 ✓ · build ✓ · pytest baseline 254/254 ✓
Rust unit tests (storage 3, events 5, tasks 8, app_state 2) — chạy `cargo test`
khi có rustup/CI.

## Milestone 5 — Migration Bridge (đã hoàn thành)

```text
crates/antares-bridge     LegacyBridge: spawn/handshake/timeout/restart/shutdown/crash detection
legacy/python/sidecar.py  JSON Lines stdio server (stdlib-only) — health.* + app.* adapters
src-tauri/commands/legacy.rs  legacy_status/start/call/restart/shutdown
TS: types/legacy.ts + services/legacyCommands.ts
```

- Handshake §234: protocol version check, mismatch → kill + Failed
- Timeout 10s default → IPC_TIMEOUT retryable
- Crash detection: try_wait + pending calls drain → IPC_SESSION_STALE
- Graceful shutdown: health.shutdown → đợi 3s → kill
- Env config: ANTARES_LEGACY_SIDECAR (dev); sau bundle dùng Tauri externalBin
- Gates: pytest 266/266 (254 baseline + 12 sidecar) ✓ · vue-tsc ✓ · vitest 33/33 ✓ · build ✓

## Milestone 6 — Core User Flows (đã hoàn thành)

```text
legacy/python/sidecar.py   M6 adapters: instances.list/get/create/select,
                           accounts.list/select, java.list, versions.list,
                           play.preflight (§8/§30 PASS/WARNING/ERROR),
                           play.launch (LaunchOrchestrator async → taskId),
                           dashboard.summary (§7)
TS:
├── types/instances.ts    AntaresInstance/JavaInfo/Preflight*/DashboardSummary
├── services/flowsCommands.ts  typed flow helpers
├── stores/instances.store.ts  phase machine idle/loading/ready/error/offline
├── features/play/PlayPage.vue    instance thật + preflight thật + launch thật
│                                 + loading/empty/error/offline states (§158/§68)
└── features/dashboard/DashboardPage.vue  summary thật (instances/recent/account)
```

- Preflight: java (theo MC version → major heuristic), account (warning, không chặn
  — Play Anyway §8), disk (≥512MB), mods count; blockers chặn Play, warnings không
- Launch: play.launch → AntaresApi.minecraft_launch → taskId (async orchestrator)
- Offline mode §68: bridge chưa chạy → phase offline, UI vẫn render + Retry
- Gates: pytest 276/276 ✓ · vue-tsc ✓ · eslint 0 errors ✓ · vitest 41/41 ✓ · build ✓

## Batch 6 — Profiles (đã hoàn thành)

```text
legacy/python/sidecar.py    profile.* handlers: list/get/create/duplicate/update/
                            delete(confirm)/capture/plan/apply/revert/export/import/validate
                            + dispatch map AntaresError.to_dict() → error code §117
TS:
├── types/profiles.ts       ProfileSummary/ProfileDetail/ProfilePlan §107/ProfileExport
├── services/profilesCommands.ts  typed helpers (13 methods)
├── stores/profiles.store.ts      phase machine §158 + plan/apply/revert
└── features/profiles/ProfilesPage.vue  list + diff preview (§107) + Apply/Revert
                                 + loading/empty/error/offline states
Router + sidebar: /profiles lazy chunk (§152/§153)
Tests: tests/unit/legacy/test_sidecar_profiles.py (24 tests)
       apps/desktop/tests/services/profilesCommands.test.ts (6 tests)
```

- Plan-first §107/§140: UI bắt buộc xem diff trước khi Apply — không apply mù
- Revert §167: rollback raw options.txt của lần apply gần nhất (từng chữ cái)
- Delete yêu cầu confirm:true — nhất quán với InstanceService
- Notify `profiles.changed` → event bridge cho notification center
- Gates: pytest 300/300 ✓ · vue-tsc ✓ · eslint 0 errors ✓ · vitest 47/47 ✓ · build ✓

## Batch 7a — Mods (đã hoàn thành)

```text
legacy/python/sidecar.py    mods.* handlers: list(search-enriched)/search/install/
                            remove(path traversal reject)/health/scan/autofix/
                            quarantine.list/restore/delete(confirm)
                            + _NoopTask cho service sync (progress mất — chấp nhận B7a)
TS:
├── types/mods.ts           InstalledMod/ModSearchHit/ModsHealth/ScanReport §164/
│                           QuarantineEntry
├── services/modsCommands.ts  typed helpers (10 methods)
├── stores/mods.store.ts    phase machine §158 + tabs Installed/Discover/Security §11
└── features/mods/ModsPage.vue  3 tabs + scan verdicts (SAFE/SUSPICIOUS/DANGEROUS)
                                 + quarantine vault (restore/delete) + health issues
Router + sidebar: /mods lazy chunk
Tests: tests/unit/legacy/test_sidecar_mods.py (19 tests)
       apps/desktop/tests/services/modsCommands.test.ts (7 tests)
```

- Security §11/§164: không execute JAR — heuristic scan, UI ghi rõ "không tuyên bố
  100% safe"; DANGEROUS được quarantine (move, không delete), restore được
- mods.list không scan lại (dùng health_check metadata, best-effort) — §208 cached index
- mods.search online-only §68 — lỗi network trả envelope, UI không chết trắng
- Path traversal rejected ở mods.remove (filename chỉ là tên, không path)
- Notify `mods.changed` → event bridge
- Gates: pytest 319/319 ✓ · vue-tsc ✓ · eslint 0 errors ✓ · vitest 54/54 ✓ · build ✓

## Batch 7b — Modpack + Mod Detail (đã hoàn thành)

```text
legacy/python/sidecar.py    modpack.info (preview .mrpack)/install (async Task,
                            validate trước khi trả taskId)/status (poll)/cancel
                            + mod.detail (metadata + depends/recommends/breaks)
                            + _ensure_task_listener: TaskManager → notify('task.updated')
TS:
├── types/mods.ts           +ModDetail/MrpackInfo/ModpackTask (§90 mirror)
├── services/modsCommands.ts +5 helpers (detail, mrpack info/install/status/cancel)
├── stores/mods.store.ts    +selectDetail/installModpack/pollTask (1s cadence §171)
│                           /cancelModpackTask
└── features/mods/ModsPage.vue  mod detail panel (depends/breaks graph) + mrpack
                                 install form + task progress bar (cancel/dismiss)
Tests: test_sidecar_mods.py +9 (28 tổng) · modsCommands.test.ts +4 (11 tổng)
```

- Install .mrpack chạy background thread với Task thật (§90) — cancel được qua
  task.cancelled flag; validate index trước khi trả taskId (fail fast)
- UI poll `modpack.status` 1s/lần (§171 — không spam command, không poll 100Hz)
- Task listener sidecar: notify('task.updated') cho event bridge — UI vẫn dùng poll
  làm nguồn chính thức (B7b đơn giản, event flow để Batch 12 mở rộng)
- mod.detail đọc metadata jar read-only (fabric.mod.json/mods.toml) — không execute
- Gates: pytest 328/328 ✓ · vue-tsc ✓ · eslint 0 errors ✓ · vitest 58/58 ✓ · build ✓

## Batch 8a — Asset Library + Editor Shell (đã hoàn thành)

```text
legacy/python/sidecar.py    asset.list/import/get/delete/assign + resource.list/get
                            (AssetStore thật: validate signature/dims/size §12.2,
                            content-addressed sha256 §125, reject traversal ở assign)
TS:
├── types/assets.ts         AssetEntry/ResourceProject/ASSET_CATEGORIES
├── services/assetsCommands.ts  +7 helpers + fileToBase64 (FileReader)
├── stores/assets.store.ts  phase machine §158 + filters + preview cache (§205)
└── features/resources/ResourceStudioPage.vue  editor shell 3 cột §12/§123:
    Assets (grid + search/category/sort + import PNG) | Preview (pixelated,
    checkerboard) | Inspector (metadata + assign to project + delete)
Router + sidebar: /resources lazy chunk
Tests: tests/unit/legacy/test_sidecar_assets.py (16)
       apps/desktop/tests/services/assetsCommands.test.ts (6)
```

- Import không pass path — UI đọc file qua FileReader → base64 (giới hạn attack surface)
- Preview cache theo assetId trong session — data URI không refetch (§205)
- Preview render `image-rendering: pixelated` + checkerboard — chuẩn texture editor
- Assign validate target path `assets/<ns>/textures/*.png` phía service — traversal blocked
- Gates: pytest 344/344 ✓ · vue-tsc ✓ · eslint 0 errors ✓ · vitest 64/64 ✓ · build ✓

## Batch 8b — Resource Studio pack lifecycle (đã hoàn thành)

```text
legacy/python/sidecar.py    resource.* handlers: wizard_info (§10.2 templates + versions
                            đã xác minh)/create/update/delete(confirm)/generate/
                            validate (§127 findings)/build (§126 ZIP + sha256 manifest)/
                            build_task (TaskManager thật + task.updated)/build_cancel/
                            builds/install (§75 build-nếu-cần rồi copy vào resourcepacks,
                            overwrite yêu cầu flag — backup tự động)/installed/uninstall
                            (traversal reject)/layer.get/set/move (§173 deterministic
                            order + sync options.txt atomically)
TS:
├── types/resources.ts      WizardInfo/ValidationFinding/BuildManifest/PackInstallResult/
│                           LayerState (§126–§128/§173 mirrors)
├── types/assets.ts         +buildCount vào ResourceProject (mirror project.json thật)
├── services/resourcesCommands.ts  15 typed helpers
├── stores/resources.store.ts      phase machine §158 + wizard + validate/build/install
│                                  + layer state
└── features/resources/ResourceStudioPage.vue  tab Assets|Packs: project list + wizard
    modal (name/version/template/description) + lifecycle actions (Regenerate/Validate/
    Build ZIP) + findings panel (PASS/FAIL + per-finding severity) + builds list +
    install panel (instance select + overwrite) + layer panel (↑↓ reorder + conflicts)
Tests: tests/unit/legacy/test_sidecar_resources.py (32)
       apps/desktop/tests/services/resourcesCommands.test.ts (13)
```

- Validate-first §126: build tự validate, ERROR chặn build, WARNING không (§42-logic)
- resource.build_task dùng TaskManager thật của core + listener `task.updated` (B7b)
- Install không overwrite silent — file trùng tên cần overwrite=True; service backup
  vào data/backups/resourcepacks trước khi ghi (§75)
- Layer order thấp→cao priority, deterministic; set/move sync options.txt atomically
  (§173); conflicts preview hiển thị path do >1 pack cung cấp
- AntaresError từ service giữ code taxonomy §117 qua dispatch map (VALIDATION_FAILED/
  FILE_NOT_FOUND/INSTANCE_NOT_FOUND…)
- Notify `resources.changed` → event bridge
- Gates: pytest 376/376 ✓ · vue-tsc ✓ · eslint 0 errors ✓ · vitest 77/77 ✓ · build ✓
  (ResourceStudioPage chunk 26.35 kB)

## Batch 9 — Visual Studio: Crosshair/Totem/HUD/FX (đã hoàn thành)

```text
legacy/python/sidecar.py    visual.* handlers: presets/render_preview/export_pack
                            (§134 crosshair), totem_presets/render_totem/
                            totem_model.get+save (§129 draft voxel)/render_totem_model
                            (§130 static fallback)/export_totem_pack, hud_widgets/
                            save_hud_layout (§135), fx_defaults/render_hit/
                            render_particle/export_fx_pack (§52)
TS:
├── types/visuals.ts        CrosshairSpec/TotemSpec/VisualPresets/HudWidgets/
│                           FxDefaults/VisualExportResult
├── services/visualsCommands.ts  15 typed helpers
├── stores/visuals.store.ts phase machine §158 + 4 editor states + preview
│                           theo tab + export pipeline
└── features/visuals/VisualStudioPage.vue  4 tab (Crosshair/Totem/HUD/FX):
    preset chips + spec controls (color/range/select/checkbox) + preview data URI
    (pixelated, checkerboard) + HUD widget toggles + export form (name/MC version/
    instance select/overwrite) → pipeline Resource Studio
Router + sidebar: /visuals lazy chunk (Visual icon ✦)
Tests: tests/unit/legacy/test_sidecar_visuals.py (28)
       apps/desktop/tests/services/visualsCommands.test.ts (10)
```

- Mọi preview là data URI b64 render phía service (§132: UI chỉ throttle RAF,
  không rebuild ZIP mỗi keypress — ZIP chỉ build khi Export)
- Export crosshair/totem/fx đi qua đúng pipeline Resource Studio: RS project →
  validate → build ZIP → install (overwrite có backup §75); totem thêm model JSON
  version-aware (definition 1.21.4+ / legacy)
- HUD layout validate widget id/dup/limits 0..819/0.5..4 phía service; lưu vào
  project visuals (JSON, không binary §34)
- totem_model.save validate chặn ERROR + atomic write (mục 62); corrupt draft →
  trả {spec: None} để UI recovery
- AntaresError giữ code §117; params sai → CONFIG_INVALID; installInstanceId
  không tồn tại → INSTANCE_NOT_FOUND trước khi gọi service
- Notify `visuals.changed` → event bridge
- Gates: pytest 404/404 ✓ · vue-tsc ✓ · eslint 0 errors ✓ · vitest 87/87 ✓ · build ✓

## Batch 9b — Three.js Totem 3D viewport (đã hoàn thành)

```text
apps/desktop:
├── src/features/visuals/voxelEngine.ts  Engine domain-independent (§129):
│   │                                    VoxelDocument → Scene graph → Renderer;
│   │                                    Vue không tự hiểu Minecraft JSON — mọi
│   │                                    spec đi qua specToDocument() (resolve
│   │                                    texture hex/key, lọc cube lỗi)
│   ├── VoxelEngine  async factory create() — three.js dynamic import,
│   │               chunk 3D load-on-demand (§152/§153);
│   │               quality LOW/MEDIUM/HIGH (§130) = antialias + pixelRatio;
│   │               debug modes: wireframe / bounds / axis (§130);
│   │               dispose() §131: pause loop + remove ResizeObserver +
│   │               dispose toàn bộ materials/geometries + renderer.dispose()
│   └── specToDocument()  pure function (test được không cần WebGL)
├── src/features/visuals/VisualStudioPage.vue  tab Totem 3D: WebGL viewport
│   (canvas + ResizeObserver), spec JSON editor, quality select, debug toggles,
│   Save draft qua visual.totem_model.save; WebGL fail → static render fallback
│   (render_totem_model §130); viewport chỉ mount khi tab mở, dispose khi rời
└── tests/features/voxelEngine.test.ts  specToDocument conversion (5 tests)
Dependency: three ^0.186.1 + @types/three (dev)
```

- Pipeline §129 đúng chữ: Document → Scene graph → Renderer (exporter dùng
  pipeline MC có sẵn qua export_totem_pack — model JSON version-aware)
- Memory §131 bắt buộc: rời tab → cancelAnimationFrame + observer.disconnect +
  dispose mọi GPU resources; canvas dùng v-show nhưng engine destroy thật
- §132: không rebuild scene mỗi keypress — applyDocument chỉ gọi khi change
  (JSON change / debug toggle); render loop tự xoay chậm (auto-rotate)
- three được dynamic import → chunk riêng 746 kB chỉ tải khi mở tab 3D;
  VisualStudioPage chunk chỉ 22 kB
- Gates: pytest 404/404 ✓ · vue-tsc ✓ · eslint 0 errors ✓ · vitest 92/92 ✓ · build ✓

## Batch 10 — Optimization: Game + System (đã hoàn thành)

```text
legacy/python/sidecar.py    optimization.* handlers: scan (hardware + recommendation
                            + profile catalogue)/plan (preview diff — KHÔNG ghi,
                            §3.1 plan-first)/apply (snapshot trước khi ghi — §70)/
                            rollback (phục hồi snapshot gần nhất — 1 click)/
                            snapshot_info; system.* handlers: overview (§4.2
                            hardware + power + cleanup preview)/cleanup.scan/
                            clean (paths list, undo-able)/undo/empty_trash/
                            power.status/set_plan (§35–§37 risk/reversible)
TS:
├── types/optimization.ts   OptimizationScan/Plan/Change/SnapshotInfo/Cleanup*/
│                           PowerStatus/SystemOverview
├── services/optimizationCommands.ts  12 typed helpers
├── stores/optimization.store.ts      phase machine §158 + plan-first flow +
│                                     snapshot/rollback + cleanup selection + power
└── features/optimization/OptimizationPage.vue  tab Game|System:
    Game — instance select, hardware summary, profile chips, plan preview
    (field: before → after), Apply qua confirm modal (snapshot trước §70),
    Rollback card khi có snapshot
    System — hardware + battery, power plan switcher, disk cleanup checklist
    (select paths → clean → undo), empty trash
Router + sidebar: /optimization lazy chunk (⚡ Optimize)
Tests: tests/unit/legacy/test_sidecar_optimization.py (23)
       apps/desktop/tests/services/optimizationCommands.test.ts (9)
```

- Plan-first §3.1: UI luôn preview diff (jvm + minecraft field: before → after)
  trước khi Apply — không apply mù (khớp pattern profiles.plan/apply B6)
- Snapshot §70: apply tự backup instance.json fields + options.txt raw vào
  optimization-snapshots/ trước khi ghi; rollback restore nguyên vẹn + 1 click
- Instance-local §74: mọi thay đổi game opt chỉ đụng instance.json +
  game/options.txt của instance — không system-wide
- Cleanup undo-able §37: clean trả cleanId; UI giữ lastCleanId để Undo;
  empty trash là action riêng (không undo)
- Power plan §36: set_plan fail (unknown plan) → CONFIG_INVALID envelope
- AntaresError giữ code §117 (INSTANCE_NOT_FOUND trước khi gọi service);
  notify `instances.changed` (game) + `system.changed` (cleanup/power)
- Gates: pytest 427/427 ✓ · vue-tsc ✓ · eslint 0 errors ✓ · vitest 101/101 ✓ · build ✓

## Batch 11 — Network Lab (đã hoàn thành)

```text
legacy/python/sidecar.py    net.* handlers: endpoints (§42 TCP song song các
                            endpoint launcher — mojang/modrinth/ely/curseforge)/
                            tcp (1 connect + RTT, timeout cap 10s)/ping (§42
                            Minecraft Server List Ping: MOTD/players/version/
                            latency/favicon)/probe (composition: N lần TCP →
                            min/avg/max/jitter/loss + timeline, count cap 30)/
                            dns (socket.getaddrinfo — IPv4/IPv6 + ms)
                            Nguồn: services/diagnostics/net.py (NetDiagnostics);
                            probe/dns là composition sidecar (như preflight M6)
TS:
├── types/network.ts        TcpCheck/McPing/ProbeStats/ProbeSample/NetProbe/DnsResult
├── services/networkCommands.ts  5 typed helpers
├── stores/network.store.ts phase machine §158 + target form + probe/ping state
└── features/network/NetworkPage.vue  endpoints list (ok/ms per endpoint), target
    form (host/port), MC ping card (MOTD/players/version/latency), probe stats
    (min/avg/max/jitter/loss) + timeline bar chart per-sample, DNS addresses
Router + sidebar: /network lazy chunk (⇄ Network)
Tests: tests/unit/legacy/test_sidecar_network.py (17)
       apps/desktop/tests/services/networkCommands.test.ts (5)
```

- Jitter = trung bình delta tuyệt đối giữa các mẫu RTT liên tiếp; loss tính trên
  TỔNG số mẫu (mẫu fail = None trong samples) — có test riêng cho trường hợp loss
- Probe sync nhưng bounded (count cap 30, timeout cap 5s) — UI gọi khi bấm nút,
  không poll liên tục (§171)
- DNS NXDOMAIN trả data {ok: false, error} — không phải bridge error (envelope
  ok: true); host rỗng/port sai → CONFIG_INVALID
- MC ping lỗi (offline/timeout) là data (`online: false, error`) — UI hiện card
  offline thay vì toast lỗi
- Gates: pytest 444/444 ✓ · vue-tsc ✓ · eslint 0 errors ✓ · vitest 106/106 ✓ · build ✓

## Batch 12 — Runtime/Packet: companion + inspector (đã hoàn thành)

```text
legacy/python/sidecar.py    runtime.* handlers: endpoint/start/sessions/metrics
                            (§12 — RuntimeService thật: IPC loopback, token xoay
                            mỗi start, FPS ring RAM ~600 điểm/session mục 62)/
                            pairing (ghi companion.json vào game dir — auto-pair);
                            _PacketRing (§122 in-memory ring: cap 10k default /
                            100k dev, oldest-first eviction, KHÔNG ghi disk);
                            packet.* handlers: ingest (schema v1 validate →
                            metadata ring + đẩy RuntimeService)/list (§121
                            metadata: timestamp/direction/packetId/name/size)/
                            stats/capture (§121 DEBUG PAYLOAD manual toggle —
                            payload chỉ lưu khi bật)/clear/export (§122 chỉ khi
                            user yêu cầu)
TS:
├── types/runtime.ts        RuntimeSession/RuntimeMetricPoint/RuntimeEndpoint/
│                           PacketEntry/PacketStats
├── services/runtimeCommands.ts  11 typed helpers
├── stores/runtime.store.ts phase machine §158 + poll 2s (§171) + inspector state
└── features/runtime/RuntimePage.vue  IPC endpoint card (url/token/packetTypes,
    start, write companion.json per-instance), sessions list, FPS timeline bar
    chart (60 điểm cuối), packet inspector table (click row → payload JSON khi
    capture bật), capture toggle + clear + export JSON (download blob)
Router + sidebar: /runtime lazy chunk (◉ Runtime)
Tests: tests/unit/legacy/test_sidecar_runtime.py (20)
       apps/desktop/tests/services/runtimeCommands.test.ts (10)
```

- Ring buffer §122 đúng chữ: memory-only, oldest-first eviction, cap 10k/100k
  (dev), export chỉ khi user bấm — không ghi disk liên tục
- Inspector §121 2 mode: METADATA (mặc định — không payload) và DEBUG PAYLOAD
  (manual toggle, memory bounded)
- packet.ingest validate schema v1 (version/type/payload) → CONFIG_INVALID khi
  sai; vẫn ghi metadata type lạ (inspector là công cụ debug)
- Metrics poll 2s cadence §171 (companion đẩy 2–4 packet/s — không poll 60Hz);
  poll dừng khi rời trang (clearInterval onBeforeUnmount)
- RuntimeService thật dùng cho endpoint/sessions/metrics/pairing — telemetry
  events (RUNTIME_PERFORMANCE…) publish qua ctx.events như legacy
- Gates: pytest 464/464 ✓ · vue-tsc ✓ · eslint 0 errors ✓ · vitest 116/116 ✓ · build ✓

## Batch 13 — Diagnostics: log center + repair + evidence (đã hoàn thành)

```text
legacy/python/sidecar.py    console.* handlers: sources (launcher/minecraft/crash +
                            bytes/lines)/read (§16 virtualized — limit cap 20000)/
                            analyze (§41 blueprint insights — KHÔNG LLM, mục 24)/
                            insights (chip gọn); repair.* handlers: actions/
                            scan (§39 dry-run "What will change?" — findings +
                            planned, KHÔNG ghi)/run (thực thi planned steps);
                            diagnostic.export — evidence gói JSON on-request
                            (appVersion + sources + insights + repairScans ×3,
                            không token/secret)
                            Nguồn: LogAnalyzer + RepairService thật
TS:
├── types/diagnostics.ts    LogSourceInfo/LogLine/LogInsight/RepairScan/
│                           RepairPlannedStep/DiagnosticExport
├── services/diagnosticsCommands.ts  8 typed helpers
├── stores/diagnostics.store.ts      phase machine §158 + scan-first repair flow
└── features/diagnostics/DiagnosticsPage.vue  insights cards (severity/error count
    + recommend route), log center (source chips + monospace list + truncated
    notice), repair panel (action chips → scan planned steps → Run), export
    evidence (JSON download)
Router + sidebar: /diagnostics lazy chunk (⚑ Diagnostics)
Tests: tests/unit/legacy/test_sidecar_diagnostics.py (17)
       apps/desktop/tests/services/diagnosticsCommands.test.ts (8)
```

- Blueprint matching §41 đúng legacy: 11 pattern (OOM/mod conflict/conn refused…)
  severity error/warning — không LLM, không tự bịa insight
- Repair scan-first §39: UI luôn dry-run hiện planned steps trước khi Run
  (khớp pattern plan-first profiles/optimization)
- Console read limit cap 20000 phía sidecar; truncated notice phía UI
- Evidence export là data thuần — repair scan fail vẫn trả gói (tolerated)
- instance_id được verify INSTANCE_NOT_FOUND trước repair scan/run
- Gates: pytest 481/481 ✓ · vue-tsc ✓ · eslint 0 errors ✓ · vitest 124/124 ✓ · build ✓

## Batch 14 phase 1 — Rust migration skeleton (7 mục → 6 crates)

Batch 14 (remake.md §227) gồm 7 mục: downloads/storage/process/launch/java/network/profiles.
Trong Codespace không có Rust toolchain → chiến lược **phase 1**: skeleton crates logic thuần
+ unit tests + parity checklist để CI chạy `cargo test --workspace` ở lần push tới.

```text
Cargo.toml (workspace)      +5 members (tổng 8 crates)
crates/antares-downloads    §103 state machine DISCOVER→COMMIT, retry trong phase,
│                           Verify→Download (checksum sai), checksum SHA-1/256 hex
│                           (code CHECKSUM_MISMATCH parity legacy), DownloadJob,
│                           DedupRegistry (2 instance cùng artifact → 1 download)
crates/antares-process      §113 ProcessRecord (pid/owner/instance/fingerprint/exit),
│                           CleanupPolicy Wait/Kill/Keep (game mặc định Keep),
│                           ProcessRegistry + shutdown_plan (chỉ Kill policy) + sweep
crates/antares-java         §110 JavaRuntime model, resolve order instance→profile→
│                           managed→mojang→system (slot quá cũ → fallback, không fail sớm),
│                           parse_java_version (1.8 legacy mapping) — parity discovery.py
crates/antares-launch       §112 LaunchSession (IDLE→…→COMPLETED/CRASHED, CANCELLED
│                           early exit trước SPAWNING), LaunchPreflight fail-fast 5 check
│                           (fail kèm remediation, warn không chặn), ArgumentBuilder
│                           contract vanilla (--username/--version/--gameDir/…) +
│                           offline uuid deterministic v3-shape
├─ crates/antares-launch/src/session.rs    state machine §112
└─ crates/antares-launch/src/preflight.rs  preflight + tests
crates/antares-net          rtt_stats parity 1:1 _probe_stats sidecar (verified song
│                           song Python: loss tính cả mẫu None, jitter delta liên tiếp,
│                           round 1 decimal, float path 0.5), PacketMode §121,
│                           PacketRing §122 (10k/100k, oldest-first, export on-request)
crates/antares-profiles     GAME_KEYS 28 key parity keys.py, coerce parity (bool→
│                           true/false; bool vào key lạ →1/0; banker's rounding —
│                           round(0.5)=0, round(1.5)=2; float "1.0" format),
│                           parse_game_options whitelist parity _parse_options,
│                           diff_options +/−/~ theo GAME_KEY_ORDER (§107)
docs/migration/PARITY.md    parity checklist Batch 15: bảng trạng thái per-crate +
                            inventory 118 method HANDLERS → đích Rust (đếm lại
                            2026-09-29 — số 144 ghi ban đầu là sai; 0 method dead-legacy)
```

Parity verify song song Python (chạy thật sidecar logic):
- `coerce()` — 100% khớp keys.py trên toàn bộ case test (28 key, bool variants,
  banker's rounding, float format)
- `rtt_stats()` — 5/5 case khớp `_probe_stats` (gồm float path 11.1−10.5=0.5999…)
- `parse_java_version` — 4 format output `java -version` (gồm legacy 1.8→8)

Phase 2 (đã làm — logic + I/O sync, tests unix-gated cho phần spawn/ping):
- `antares-downloads` +sha.rs: SHA-1/SHA-256 thuần Rust (test vector chuẩn "abc"/
  ""/multi-block), `verify_file` (code FILE_UNREADABLE mới cho file mất),
  `commit_artifact` atomic rename idempotent (target tồn tại → false + dọn tmp,
  không ghi đè shared immutable §104)
- `antares-process` +spawn.rs: spawn thật qua std::process — stderr drain nền,
  utf-8 lossy, `wait()` pump dòng + mark exit (0→Exited, khác→Failed), `stop()`
  stdin prompt graceful → poll try_wait timeout → kill (parity ProcessManager);
  tests spawn/exit-3/stop-graceful/stop-kill unix-gated
- `antares-net` +ping.rs: VarInt pack/read (số âm 5-byte, lỗi quá dài), frame/
  handshake builder, `parse_status` parity mc_ping (MOTD text + extra concat,
  players/version/modinfo/favicon, round 1 decimal), `ping()` TCP đọc đúng packet
  length (read_exact, không read_to_end — server không đóng connection);
  tests protocol thuần chạy mọi OS
- `antares-launch` +resolver.rs: `JavaResolver` bọc antares-java resolve order;
  `ArtifactResolver` cục bộ shared store §104 → game_dir → Missing (path đích
  commit vào store), `resolve_many` chia present/missing
- `antares-profiles` +`write_options_merged` parity `_write_options`: giữ thứ tự
  dòng, dedupe (dòng đầu thắng), append key mới cuối, luôn kết thúc \n
- CI: thêm job `rust` (3 OS) chạy `cargo clippy` (non-blocking) +
  `cargo test --workspace --exclude antares-desktop` (Tauri shell cần webkit2gtk
  system deps — kiểm ở Batch 16)

Phase 3 (đã làm — parity logic + I/O sync, tests với local TCP server):
- `antares-downloads` +http.rs/resume.rs/download.rs: HTTP/1.1 GET thuần Rust
  (redirect 10 lần — Range KHÔNG kế thừa qua redirect, Content-Length + chunked,
  body cap 2GB; https cần TLS — nối khi bundle), `PartInfo` parity `.antares-part`
  + meta (chặn resume khác URL), pipeline `download()` parity DownloadManager:
  cache check → HTTP (Range bytes=N- khi resume, attempt>1 như legacy) → .part →
  verify sha1/sha256 → atomic finalize; retry 8 + backoff min(2n,8), checksum
  mismatch không backoff, 200 thay 206 → restart từ đầu; tests dùng local TCP
  server thật (206/200/chunked/redirect/refused)
- `antares-java` +discovery.rs: scan known dirs theo OS (Program Files\Java,
  /usr/lib/jvm, /Library/Java/JavaVirtualMachines…) bỏ symlink + JAVA_HOME +
  PATH (resolve symlink về java home, dedupe giữ thứ tự), `detect_major` qua
  `java -showversion` timeout 15s (parse parity 1.8 mapping), `java_info`/
  `scan_java_infos` (path/exe/javaw/major/name)
- `antares-net` +probe.rs: `tcp_check` (multi-addr connect_timeout, RTT round 1,
  error cắt 160), `probe` cap 30 + stats rtt_stats + timeline `at` round 4 (parity
  handle_net_probe), `dns_lookup` dedupe giữ thứ tự (parity handle_net_dns),
  `check_endpoints` 5 endpoint sort by id; fix favicon parity trong ping.rs
  (bool(favicon) — chuỗi rỗng = false như Python)
- `antares-launch` +planner.rs: `required_java_major` §108 — MinecraftVersion-
  Capability tập trung (b1.x/≤1.16.5→8, 1.17→16, 1.18–1.20.4→17, 1.20.5+/1.21+→21),
  `plan_launch` tổng hợp JavaResolver→LaunchPlan→Preflight→ArgumentBuilder một bước

Phase 4 (đã làm — streaming + seams + service parity còn lại):
- `antares-downloads`: sha.rs refactor incremental `Sha1Hasher`/`Sha256Hasher`
  (update/finish, test equal-buffer-API qua chunk sizes 1..1000) +
  `verify_file_streaming` 256KB/block (file lớn không load vào RAM); http.rs refactor
  `get_following_redirects` dùng chung + **`get_stream`** streaming callback từng chunk
  64KB (content-length/chunked/till-close, callback Err → dừng + bubble user error) +
  **TLS seam** `https_url_to_http` (https → cổng chết, fail rõ `NET_UNREACHABLE` tới
  khi bundle Batch 16); download.rs `attempt_once` streaming — ghi .part từng chunk
  qua BufWriter, restart 200≠206 đệ quy resume=false (File::create truncate),
  verify_digests dùng streaming
- `antares-profiles` +store.rs: `ProfileStore` CRUD qua antares-storage — state
  `profiles-state.json` (field JSON parity `lastRaw`), create id
  `prof-YYYYmmdd-HHMMSS-<6hex>` (civil_from_days UTC), name validate parity
  `is_safe_name` (≤64 ký tự, đếm chars không phải byte), duplicate uniquify
  "copy"/"copy 2", update name/spec, delete confirm:true → false khi thiếu,
  mark_applied (applied chỉ 1 entry như legacy) / clear_revert / launch_hint;
  `sanitize_spec`/`validate_spec` parity (game whitelist + coerce, launch whitelist
  bỏ giá trị falsy, jvmArgs list[str])
- `antares-launch` +exit.rs: `ExitAnalyzer` §116 — ingest→normalize→fingerprint
  (FNV-1a 64 hex 16)→classify (pattern OOM/mod-conflict/java-missing/user-cancel,
  weight 40/30/30/20)→rank evidence→recommendation; weak evidence (<20) →
  Unknown + confidence 0, không LLM; sample dòng cắt 160 ký tự parity `_short_err`
  (byte-offset an toàn Unicode); `CompanionPairing` — ghi companion.json atomic
  (tmp+rename) parity `write_pairing_for_instance` (version 1, endpoint/token/
  packetTypes, writtenAt), server off → Ok(None) không tạo dir
- `antares-net` +manifest.rs: Mojang manifest parity `ManifestService` — URL v2,
  `ManifestCache` parity `DiskCache` (`{ts,value}`, TTL 30 phút, corrupt → None,
  key sanitize /:→_), `get_manifest` cache→HTTP→stale fallback (mục 60 offline),
  `find` scan entries, `download_size` = client + libraries artifact/classifiers +
  assetIndex.totalSize (lỗi → None như legacy nuốt exception)
- Fix workspace: `antares-profiles` thiếu trong members root `Cargo.toml` (cargo
  test --workspace sẽ bỏ crate) — đã thêm; deps: profiles +antares-storage/thiserror,
  launch +serde_json, net +antares-downloads

## Feature flags (§81/§243)

```text
new_ui:        on (apps/desktop) — 10 route lazy
rust_core:     9 crates workspace (antares-profiles thêm vào members ở phase 4) —
               Batch 14 phase 1–4 code + tests (cargo test chạy trên CI;
               không có toolchain local)
python_legacy: untouched (481 tests vẫn xanh) — removal ở Batch 15
```

## Bước tiếp theo (theo thứ tự §227 — không được đảo)

```text
Batch 14 — Còn lại (phase sau khi có TLS/tokio): TLS thật, song song hoá
           endpoints, §114 stdout pipeline, runtime download Mojang
Batch 15 — parity check (kế hoạch chi tiết trong PARITY.md): B15.1 tạo 6 crate
           còn thiếu (mods/resources/visuals/optimization/system/diagnostics —
           73 method) → B15.2 wiring Tauri command → B15.3 A/B parity 118 method
           → B15.4 flag off + UI flow → B15.5 disable Python + benchmark →
           B15.6 remove bridge + cleanup (frontend/, legacy/)
Batch 16 — Windows build, NSIS, portable, updater, signing, clean-machine test
```

### Audit Batch 15 (đếm lại 2026-09-29)

```text
HANDLERS sidecar: 118 method / 23 nhóm (số 144 ghi trước đây là sai — đã sửa docs)
Dead-legacy:       0 (mọi method được gọi từ UI/tests/companion — scan chuỗi)
Rust đã bọc:       java(1) versions(1) play(2) profiles(13) net(5) + ring/pairing
B15.1a ĐÃ LÀM:     antares-system (7 method) — DiskCleaner parity cleaner.py
                   (4 SAFE_RULES, 5 PROTECTED_DIRS, trash+manifest+undo, owned
                   check mục 76, mini-glob matcher) + PowerService parity power.py
                   (powercfg /getactivescheme regex, 3 GUID plan, recommendation
                   Power Saver→High Performance hint-first mục 35/74, set_plan
                   GUID-only, non-Windows unsupported) — workspace 10 crates
B15.1b ĐÃ LÀM:     antares-diagnostics (8 method) — LogAnalyzer parity
                   log_analyzer.py (11 blueprint §41 không-LLM, severity matrix,
                   sort count→severity, firstSeen/lastSeen, lines cắt 240, read
                   limit/truncated 400, crash 3 file mới nhất, minecraft 5000
                   dòng cuối với line_no toàn file) + RepairService parity
                   repair/service.py (7 ACTIONS dry-run first §39, run thực thi
                   mkdir/write_json/rewrite_lines/trash, to_trash owned check,
                   ZIP pack.mcmeta + header parse) — workspace 11 crates
B15.1c ĐÃ LÀM:     antares-optimization (5 method) — advisor parity advisor.py
                   (memory 35% RAM trần 8GB sàn 1GB, profile low_end/performance/
                   balanced, warnings/bottlenecks; hardware inject — không psutil)
                   + 6 PROFILES parity profiles.py (PROFILE_ORDER, memory_for_
                   profile không đoán mục 72) + service parity service.py (plan
                   diff không ghi, apply snapshot opt-<ts>.json trước khi ghi mục
                   70, rollback restore nguyên vẹn instance.json + optionsRaw,
                   state optimization.json, write_options dedupe mục 10.5,
                   McValue::Owned cho after đã render) — workspace 12 crates
B15.1d ĐÃ LÀM:     antares-mods phase A (health/quarantine/list trong 15 method —
                   deflate RFC1951 thuần golden-vectors Python zlib + zipread
                   central directory method 0/8 + cap 64MB, read_mod_info
                   fabric/quilt/mods.toml/neoforge/mcmod.info, check_health
                   unreadable/wrong_loader/missing_dep/breaks, version_newer,
                   quarantine vault move/restore/delete parity mục 368,
                   list_installed/uninstall traversal-reject) — workspace 13 crates
B15.1d PHASE B:    + jar_reader decode Java class JVMS thuần (constant pool đủ
                   tags Utf8/Class/Ref/NameAndType/MethodHandle/InvokeDynamic,
                   Long/Double 2-slot, Code attribute walk opcode lengths,
                   BootstrapMethods, reflection flags) + scanner 10 rules
                   (DOWNLOAD_AND_EXECUTE 80, REVERSE_SHELL 75, CREDENTIAL 45,
                   MASS_CRYPTO 55, REGISTRY 25, HEAVY_OBFUSCATION 20, BEACON 10,
                   CUSTOM_INDY_BOOTSTRAP 35 + INDY_ABUSE 30, CALLGRAPH BFS
                   depth≤4 weight 65/55/40, REFLECTION_MH 20) — thresholds
                   SUSPICIOUS 25 / DANGEROUS 60; tests với class file dựng
                   thủ công qua constant pool builder
B15.1d PHASE C:    + modrinth.rs (search/versions URL builder parity facets +
                   urlencode thuần, pick_file primary-first, fetch qua
                   antares-downloads https seam, parse hits, build_fix_plan
                   parity KNOWN_DEPS dedupe + skip non-fixable) + mrpack.rs
                   (read info parity, detect_loader thứ tự fabric→quilt→
                   neoforge→forge, filter_files env required/optional,
                   ensure_inside mục 76, install plan downloads + overrides
                   skip 0-byte) + auto_fix.rs (flow scan→plan→download inject
                   callbacks, plan không kèm url parity, cancel propagate)
                   — mods ĐỦ 15 method logic parity (wiring download thật ở
                   tầng Tauri command)
B15.1e ĐÃ LÀM:     antares-resources phase A (asset 5 + validator — nền parity
                   mạnh nhất của resources 23 method): AssetStore parity assets.py
                   (import PNG sig+IHDR+dims 8MB/4096 mục 12.2/34/77, sha256
                   content-addressed `assets/<sha>.png` tmp+rename, duplicate
                   content reuse file, catalog schemaVersion 1 atomic corrupt→
                   backup .corrupt+reset, list filter query name/sha[:12]/tags +
                   category + tag sort newest/oldest/name/size, delete chỉ xoá
                   metadata, assign_to_project regex `^assets/[a-z0-9_-]+/
                   textures/.+\.png$` + `..`/`//` reject + project.json assets
                   array replace cùng path, safe_name/category/tag parity mục
                   12.3) + png.rs encode_png CRC32/deflate-stored/adler32 thuần
                   + decode_png_dims header-parse không decode IDAT (mục 77) +
                   validator.rs parity validator.py (validate_dir findings
                   camelCase code/severity/path/detail — mcmeta pack_format int
                   hoặc [major,minor] mục 6.3, wrong_path prefix whitelist,
                   unsupported_asset WARNING ext rỗng cũng warning, oversized
                   8MB, missing_texture header-parse parity message
                   "not a valid PNG"/"too large: WxH" KHÔNG yêu cầu IHDR như
                   legacy, invalid_json, invalid_model_reference ns:[a-z0-9_.-]
                   → assets/<ns>/textures/*.png, duplicate_file WARNING sha1
                   qua antares-downloads::sha1_hex skip pack.mcmeta, structure
                   KNOWN_MC_DIRS 9 dir + _DIR_EXT_RULES 5 rule ext rỗng bị
                   reject, validate_zip_names root whitelist pack.mcmeta/pack.png
                   case-insensitive assets/; has_errors = FAIL chỉ khi ERROR mục
                   42; read_png hash check lowercase-only hex fix + list sha
                   get(..12) chống panic catalog tay) — workspace 14 crates
B15.1f ĐÃ LÀM:     antares-visuals phase A (logic thuần của visual.* 15 method):
                   crosshair parity crosshair.py (DEFAULT_SPEC + PRESETS 7 theo
                   PRESET_ORDER mục 8.1, normalize clamp thickness 1..4/gap 0..7/
                   opacity 0.1..1.0 mục 76, int() truncate parity, bool không ăn
                   làm number, render geometry arms/circle 72 bước/dot/outline 4
                   hướng/dot giữa, PNG 16×16) + totem parity totem.py (presets 8,
                   texture 32×32 glow/head/eyes 2px/torso zigzag/legs/wings alpha
                   220, voxel_spec Quick→Advanced mục 70 4 cubes) + fx parity
                   fx.py (hit 128×128 5 kinds none/flash/vignette/arrow/cross với
                   alpha công thức từng kind, HIT_MAX_ALPHA 200, size 20..100;
                   particle atlas 16×(16×N) 5 shapes orb/spark/star/ring/smoke
                   grow/co theo frame, glow lõi +25, P_MAX_FRAMES 8; mcmeta
                   animation frames list + frametime clamp 1..10 indent 2) + hud
                   parity hud.py (WIDGETS 14, DEFAULT_LAYOUT 3, LIMITS x 0..819/y
                   0..459/scale 0.5..4, validate lỗi unknown/duplicate/not-number/
                   out-of-bounds, sanitize dedupe + clamp + round scale 2, merge
                   default append thiếu) + model3d parity model3d.py (finding
                   shape mục 15.2 severity ERROR/FATAL block — grid int dương,
                   cubes array non-empty, cap 32/64/256 mục 35, bounds [-16,32]
                   từng trục, size >0, id unique warning-only, faces 6 key,
                   texture ref hex/asset-path/key-table, UV đảo ERROR fixable +
                   ngoài [0,16] WARNING tile, normalize rename dup cube-N) +
                   renderer parity renderer.py (isometric sx=x-z sy=(x+z)*0.5-y,
                   SHADE up 1.0/east 0.8/south 0.62, FALLBACK #e8b23a, z-buffer
                   depth=x+y+z giữ MAX, scale-fit center pad 12, size clamp
                   16..1024, numeric string parse như float(); giữ quirk det
                   unscaled cho A/B byte-parity — fix đồng bộ 2 bên sau) + draft
                   parity totem_model_get/save (draft.json indent 2 atomic
                   tmp+rename, corrupt → None recovery, validate chặn trước ghi)
                   + base64 zero-dep golden RFC 4648 + data URI — render PNG tái
                   dùng antares_resources::encode_png — workspace 15 crates
B15.1f PARITY:     + golden A/B fixtures: tests/parity/gen_visuals_golden.py sinh
                   25 case RGBA (crosshair 8 mọi shape + clamp + size32, totem 5,
                   hit 6, particle 6) từ Python legacy → tests/parity/golden/
                   visuals/*.json; integration test crates/antares-visuals/tests/
                   golden_parity.rs so pixel-parity (PNG container không so byte —
                   zlib.compress vs stored-blocks) — evidence #1 PARITY.md
Cần crate mới:     0 — B15.1 ĐỦ 6 crate (a–f); defer: resource.* 18 method +
                   visual export 3 (project store/builder/installer phase sau)
                   + phần còn lại của mods
Cần wiring:        instances(4) accounts(2) dashboard(1) health/app(5) + phần
                   runtime/packet/play/profiles chưa nối
```
