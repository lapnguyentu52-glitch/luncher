# Antares Launcher 4.0

Minecraft Control Center — launcher + instance manager + mod manager + resource editor +
optimization center + runtime monitor + diagnostics.

Kiến trúc đích: **Tauri 2 + Vue 3 + TypeScript (UI) · Rust (core) · Python legacy (bridge, temporary) · Fabric companion mod (runtime telemetry)**.

Xem `remake.md` cho master plan và `docs/migration/` cho trạng thái migration.

## Cấu trúc

```text
apps/desktop/      Vue 3 + TypeScript + Vite (UI mới)
src-tauri/         Tauri 2 shell (Rust)
crates/            Rust core workspace (storage, core, bridge, downloads, process,
                   java, launch, net, profiles — 9 crates)
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
python -m pytest tests -q    # 254 passed — không được phá trong migration
```

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
  `antares-process` (supervisor §113, cleanup policy; phase 2: spawn/wait/stop thật),
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
  `docs/migration/PARITY.md` + CI job `cargo test --workspace` (3 OS, exclude Tauri shell)
- [x] Batch 6 Profiles · 7a Mods · 7b Modpack/Detail · 8a Asset Library · 8b Resource
  Studio · 9 Visual Studio (+ Three.js totem) · 10 Optimization · 11 Network Lab ·
  12 Runtime/Packet · 13 Diagnostics — qua legacy bridge (chi tiết trong BASELINE.md)
