ANTARES LAUNCHER 4.0 — DEEP SOURCE AUDIT + NO-PYTHON REFACTOR PLAN

Repository: lapnguyentu52-glitch/luncher

Branch inspected: codespace-cautious-dollop-wvw4x59pg76h5qq9

Audit date: 2026-10-02

Primary goal: move the launcher from a hybrid Tauri/Vue/Rust + Python legacy architecture to a production runtime/build that does not require Python.

1. AUDIT SCOPE AND LIMITATIONS

This audit used the public GitHub repository pages/raw source, the repository's own migration documents, the Cargo manifests, Tauri configuration, CI workflow, and the main Rust modules that were reachable through the public raw tree.

A direct git clone from the execution environment failed because the environment could not resolve github.com. Therefore:

source-level review was performed against the public raw repository contents;

migration/architecture inconsistencies were cross-checked against README.md, remake.md, BASELINE.md, PARITY.md, Cargo.toml, the Tauri config and CI;

no claim is made that cargo test, cargo clippy, pnpm build, or a Windows installer was executed by this audit environment;

findings marked Confirmed are directly visible in source/config/docs; findings marked Gap are missing validation rather than proven runtime failure.

The repo's own migration docs state that the original Python baseline had 254 passing tests, and later migration gates grew the suite substantially; the baseline document records the original 254/254 result on 2026-09-27. Treat those as historical evidence, not as a fresh 2026-10-02 execution result.

2. EXECUTIVE SUMMARY

Current architecture

The repository has already moved a large part of the architecture toward:

Vue 3 + TypeScript
        ↓
      Tauri 2
        ↓
      Rust core
        ↓
Rust crates for storage/downloads/process/java/launch/network/
profiles/system/diagnostics/optimization/mods/resources/visuals
        ↓
Python legacy sidecar + legacy Python service tree   ← current blocker
        ↓
Fabric companion mod

The repository itself describes Python as temporary, but the migration is not yet complete. PARITY.md records 118 sidecar handler methods across 23 groups and says none were dead legacy at the 2026-09-29 scan; several are still only wired through the sidecar, while others have Rust logic but no final Tauri command wiring yet.

The biggest practical conclusion is:

The Rust foundation is far enough along to remove Python eventually, but the repository is not yet a clean no-Python application. The remaining work is mostly integration/wiring, missing runtime capabilities, and a few correctness/security gaps in the new Rust layer.

3. PRIORITY FINDINGS

ID

Severity

Area

Finding

Status

F-01

BLOCKER

Networking

HTTPS/TLS is not implemented in the Rust downloader. https:// URLs are rewritten to http://127.0.0.1:1/..., intentionally forcing failure.

Confirmed

F-02

BLOCKER

Python removal

Tauri currently exposes legacy bridge commands (legacy_*) while most migrated UI flows still depend on the Python sidecar.

Confirmed

F-03

HIGH

Task system

TaskRegistry::fail() performs the state transition, then updates the stored message, but returns the pre-message clone. Callers can observe a failed task without the failure message in the returned object.

Confirmed

F-04

HIGH

Windows process support

Core process behavior tests are mostly #[cfg(unix)]; the CI matrix includes Windows, but those tests do not exercise Windows process semantics.

Confirmed gap

F-05

HIGH

Security

Tauri security.csp is explicitly null, so the production webview has no configured Content Security Policy.

Confirmed hardening gap

F-06

HIGH

Docs/version drift

pyproject.toml reports project version 3.0.0 while the Tauri/package/Cargo workspace are 4.0.0.

Confirmed

F-07

MEDIUM

Docs drift

README says the Rust workspace contains 9 crates; root Cargo.toml currently lists 15 Rust crates plus the Tauri shell.

Confirmed

F-08

MEDIUM

CI/docs drift

Migration docs refer to a 3-OS Rust validation, but the checked-in CI workflow currently has only Ubuntu + Windows in the Rust matrix.

Confirmed

F-09

MEDIUM

HTTP parser

parse_url() is a minimal parser; it does not robustly handle IPv6 literal authorities or some URL forms without a slash before a query.

Confirmed gap/risk

F-10

MEDIUM

HTTP body limit

read_full_body() uses Read::take(2GB) for connection-close responses without an explicit over-limit detection step; a body larger than the cap can become a truncated successful body instead of a hard size error.

Confirmed risk

F-11

MEDIUM

Bridge robustness

Bridge crash detection is an explicit polling API (check_health()), not a continuously owned supervisor; EOF from the reader alone does not update bridge state.

Confirmed design gap

F-12

MEDIUM

Process logs

antares-process still lacks the planned timestamped stdout/stderr merge pipeline and scoped log storage.

Confirmed incomplete

F-13

MEDIUM

Download concurrency

The current HTTP/downloader implementation is synchronous; endpoint checks are still sequential and the repo explicitly defers parallelisation until a Tokio-based stage.

Confirmed incomplete

F-14

MEDIUM

Runtime scope

Root Tauri commands currently cover app/core/legacy infrastructure only; the migrated service crates are not yet exposed as the final app command surface.

Confirmed

4. TOP-LEVEL REPOSITORY AUDIT

4.1 apps/desktop/

What is good

Vue 3 + TypeScript.

Vite build.

Pinia state management.

Vitest + jsdom test setup.

vue-tsc strict-style typecheck flow.

code splitting through Vite manual chunks.

Problems / risks

A-01 — Frontend is modern, but final backend contract is not fully Rust-native

The frontend package is correctly positioned as the new UI layer, but the repo still contains legacy service calls and sidecar integration as the bridge for major features.

Action: every feature store/service must be migrated to a typed Tauri command/event contract before Python is deleted.

A-02 — No generated end-to-end contract barrier

The architecture plan explicitly wants typed command/event contracts, but the current workspace still contains a mixed typed API + legacy bridge model.

Action: define one schema source (Rust structs or a stable JSON schema), generate/maintain TypeScript types from it, and ban ad-hoc sidecar JSON from production UI code.

A-03 — Bundle source maps are enabled in production

vite.config.ts sets build.sourcemap: true.

This is not automatically wrong, but for a Windows desktop production build it increases artifact size and can expose implementation details if source maps are shipped with the executable/assets.

Action: use an explicit release policy:

release: sourcemap=false or hidden/secure artifact upload
nightly/dev: sourcemap=true

5. src-tauri/ AUDIT

5.1 src-tauri/Cargo.toml

Current dependencies are deliberately small and mainly connect antares-core and antares-bridge to Tauri.

Main issue

The shell dependency graph proves the migration is not finished: the Tauri shell currently depends on the new core + bridge, but does not directly expose the full set of migrated functional crates as application commands.

Target: Tauri should become a thin boundary around a Rust application service layer, not a thin boundary around legacy_* commands.

5.2 tauri.conf.json

Confirmed hardening issue — security.csp = null

Production should have a restrictive CSP appropriate to bundled local assets.

Target direction:

script-src 'self' 'unsafe-inline' (only if actually required)
style-src 'self' 'unsafe-inline'
img-src 'self' data: blob:
connect-src 'self' https://official-required-hosts.example
font-src 'self' data:

The exact CSP should be derived from the actual build output rather than copied blindly.

Packaging issue

Only NSIS is configured as the bundle target.

The root package script named tauri:build:portable uses tauri build --no-bundle. That is useful for obtaining raw build artifacts, but it is not the same thing as a formally tested portable distribution.

Action: create separate CI artifacts:

Antares-Setup-x64.exe      ← installer
Antares-Portable-x64.zip   ← real portable bundle

and test both on a clean Windows VM/runner.

5.3 src-tauri/src/lib.rs

Confirmed architectural issue

Registered commands are currently:

app_*
core_*
legacy_*

There is no final command surface for the already-created service crates such as:

antares-downloads
antares-java
antares-launch
antares-net
antares-profiles
antares-system
antares-diagnostics
antares-optimization
antares-mods
antares-resources
antares-visuals

That means a large part of the Rust code exists as a migration substrate, not yet as the actual application runtime.

Priority: implement feature commands behind stable names, then delete legacy_* registrations after the flag-off validation passes.

5.4 state/core_state.rs

Good

portable/installed storage mode is centralized;

data root is shared with the legacy sidecar during migration;

state ownership is clear.

Migration debt

CoreState still stores LegacyBridge as a first-class component of the app state.

Target after migration:

CoreState
├── AppState
├── Storage
├── RuntimeSupervisor
├── DownloadManager
├── JavaManager
├── MinecraftService
├── NetworkService
├── DiagnosticsService
└── PluginRuntime

No sidecar handle should remain in the production state graph once Python is removed.

5.5 events/bridge.rs

The bridge thread polls every 50 ms and emits drained events individually.

Performance/design concern

At high event rates this can become:

producer burst
   ↓
EventHub
   ↓
50 ms drain
   ↓
N individual app.emit() calls

The design is safe enough for low-rate events, but not ideal for high-frequency telemetry.

Target: batch event envelopes into one UI emission where possible, and classify streams:

Latest       → UI state
Coalesce     → keyed progress/state
Batched      → logs/notifications
Lossless     → truly critical events only

The current EventHub is explicitly bounded, which is good, but the Lossless queue is still capped at 1024 and therefore can drop oldest events. That is a policy decision that must be documented clearly: it is “bounded-lossless-until-capacity”, not mathematically lossless.

6. crates/antares-core/ AUDIT

6.1 EventHub

Good

bounded queues;

separate QoS classes;

no infinite producer blocking;

coalesce/latest/batched/lossless semantics are explicit.

Finding

Lossless still drops oldest events at capacity. This is reasonable for bounded memory, but the name can mislead callers.

Rename/documentation recommendation:

LosslessUntilCapacity

or keep Lossless but document the hard cap as part of the contract.

6.2 TaskRegistry

Confirmed correctness bug — fail() stale return value

Current flow:

transition(task_id, Failed)
        ↓
returns cloned Task
        ↓
write Task.message = failure message
        ↓
return OLD clone

So the registry stores the message, but the caller gets a stale Task value without that new message.

Fix

Refactor to mutate message before cloning the final value, for example:

pub fn fail(&self, task_id: &str, message: impl Into<String>) -> CoreResult<Task> {
    let mut tasks = self.tasks.write();
    let task = tasks
        .get_mut(task_id)
        .ok_or_else(|| CoreError::TaskNotFound(task_id.to_string()))?;

    if !transition_allowed(task.state, TaskState::Failed) {
        return Err(CoreError::InvalidTransition { /* ... */ });
    }

    task.state = TaskState::Failed;
    task.finished_at_ms = Some(now_ms());
    task.message = Some(message.into());

    // remove dedupe mapping after final state is known
    // ...

    Ok(task.clone())
}

Also add a regression test that checks both stored and returned message.

6.3 Task lifecycle cleanup

The task queue is stored separately from the task map. A future production pass should make sure terminal task IDs do not accumulate forever in the queue/map.

Required lifecycle policy:

Queued → Running → Completed/Failed/Cancelled
                         ↓
                dedupe mapping removed
                         ↓
              history retention policy

Use bounded history rather than unlimited in-memory task retention.

7. crates/antares-storage/ AUDIT

This is one of the stronger foundations.

Confirmed good areas

scoped roots;

traversal rejection;

atomic writes;

safe listing/removal;

tests for traversal and atomic roundtrip.

Remaining requirement

Before Python removal, all legacy file formats need explicit Rust migration readers.

Do not change storage schema casually during the no-Python cutover.

Target migration rule:

read old JSON → validate → normalize → rewrite new schema only after success

Never do destructive conversion before a validated backup exists.

8. crates/antares-downloads/ AUDIT

8.1 Major blocker — HTTPS/TLS

The current HTTP engine is intentionally zero-dependency TCP HTTP/1.1.

For https://, the current seam rewrites the URL to:

http://127.0.0.1:1/...

which guarantees failure until a real TLS connector is bundled.

This means the launcher cannot yet perform normal HTTPS artifact downloads through the migrated Rust engine.

Required fix

Replace the seam with a real TLS-capable HTTP stack.

Recommended architecture:

DownloadService
    ↓
HttpClient trait
    ├── ProductionHttpClient (TLS)
    └── TestHttpClient (local fixture)

Keep the crate API independent of the chosen HTTP implementation.

Security requirements for the new HTTP layer

HTTPS certificate validation on by default;

no insecure TLS fallback;

timeout per connect/read/write stage;

redirect limit;

redirect auth-header stripping policy;

maximum response size;

checksum verification before commit;

atomic finalization;

safe resumable metadata;

cancellation support.

8.2 URL parsing risks

parse_url() is deliberately minimal.

Known limitations to remove:

IPv6 literal authorities ([::1] form);

query-only URLs without /;

more rigorous authority parsing;

explicit scheme preservation across redirects;

URL decoding/normalization policy.

Do not keep expanding a hand-written URL parser once a real HTTP stack is introduced.

8.3 Body cap correctness

read_full_body() limits the reader with take(MAX_BODY_BYTES) when there is no Content-Length and no chunked encoding.

That prevents unbounded memory growth, but does not by itself prove that the server body was not larger than the cap.

Target behavior:

bytes == cap and stream continues
        ↓
HARD_BODY_LIMIT

Do not silently finalize truncated data.

8.4 Download retries

The current pipeline has an explicit 8-attempt default and checksum-based retry behavior.

Keep the existing parity tests, but add:

TLS failure
HTTP 404
HTTP 429
HTTP 5xx
redirect loop
body too large
checksum mismatch
cancel during stream
resume with changed URL
resume with stale partial file

9. crates/antares-process/ AUDIT

Good

native std::process spawn;

stdin/stdout/stderr ownership is explicit;

graceful stop then kill fallback;

process records track owner/instance/pid/exit state.

9.1 Windows validation gap

Several important tests are behind #[cfg(unix)].

For a Windows-first launcher this is not enough.

Required Windows tests:

spawn executable
stdout capture
stderr capture
non-zero exit
stdin graceful stop
forced kill
working directory
Unicode path
space-containing path
child survives/terminates according to cleanup policy

9.2 Missing log pipeline

Migration docs already identify the missing combined stdout/stderr timestamp pipeline and scoped log storage.

Target:

child stdout ─┐
              ├→ LogMux → bounded ring → LogsRoot → diagnostics
child stderr ─┘

Do not use one blocking reader per process if 10–20 Minecraft instances are a supported scale target. Use bounded channels and dedicated lightweight reader threads or an async runtime, with per-process caps.

10. crates/antares-java/ AUDIT

Good

resolution order is explicit;

system discovery exists;

JAVA_HOME and PATH discovery are included;

java -showversion timeout exists;

Java 8 legacy parsing is covered.

Missing for complete Python removal

managed/Mojang runtime download and installation;

verified runtime metadata;

complete version-to-Java capability matrix tied to Minecraft artifact metadata.

Target API:

JavaManager
├── discover_system()
├── discover_managed()
├── resolve(instance, profile)
├── ensure(required_major)
├── verify(runtime)
└── launch_command(runtime)

11. crates/antares-launch/ AUDIT

This crate is one of the more mature migration areas.

Good

launch session state machine;

preflight;

Java resolver;

artifact resolver;

Java-major planning;

exit analyzer;

companion pairing.

Remaining gap

The planner and analyzers exist, but the complete real launch orchestration still has to be wired through the application service layer and process supervisor.

Target execution graph:

UI Play
 ↓
LaunchCommand
 ↓
Resolve Instance
 ↓
Resolve Java
 ↓
Resolve Artifacts
 ↓
Preflight
 ↓
Build Command
 ↓
Spawn Process
 ↓
Attach log streams
 ↓
Attach companion runtime channel
 ↓
Watch process
 ↓
Analyze exit
 ↓
Persist session summary

No UI layer should directly know how to build the Java command line.

12. crates/antares-net/ AUDIT

Good

Minecraft Server List Ping parser;

RTT statistics;

TCP probes;

DNS checks;

manifest cache;

packet ring model;

endpoint health checks.

Remaining gaps

endpoint concurrency is still sequential;

HTTPS is blocked by the downloader TLS seam;

packet inspection is mostly a model/ring layer until runtime packet ingest is fully wired;

high-frequency network telemetry needs a single producer pipeline instead of repeated UI polling.

Target:

NetworkService
├── HttpClient
├── ServerPing
├── EndpointProbe
├── DnsProbe
├── ManifestCache
└── PacketStream

Use async I/O only where it materially improves concurrency; do not move everything into Tokio just because it is available.

13. crates/antares-profiles/ AUDIT

Good

options parsing whitelist;

coercion parity;

diff engine;

atomic profile store;

sanitize/validate logic;

duplicate profile naming behavior.

Remaining gaps

The migration documents identify capture/plan/apply/revert integration and profile import/export as remaining tasks.

Required no-Python cutover condition:

profiles.capture   → Rust
profiles.plan      → Rust
profiles.apply     → Rust
profiles.revert    → Rust
profiles.export    → Rust
profiles.import    → Rust

Do not route any of these through legacy_call after cutover.

14. crates/antares-system/ AUDIT

The design is intentionally safe-first.

Good

launcher-owned paths;

protected folders;

trash/manifest model;

undo-first deletion;

Windows powercfg path is isolated.

No-Python integration requirement

Hardware metrics must be collected natively. Do not reintroduce a Python/psutil dependency through a “temporary” helper.

Windows system APIs should be isolated behind a platform module.

15. crates/antares-diagnostics/ AUDIT

Good

deterministic log pattern analysis;

no LLM dependency;

source-aware insights;

repair dry-run first;

bounded console reads.

Remaining gap

The analyzer needs first-class inputs from the new native runtime pipeline:

Minecraft logs
Crash reports
Launcher logs
Runtime telemetry
Process exit metadata

Do not recreate Python's text-processing dependency in another scripting runtime. Keep analysis in Rust and make it deterministic/golden-testable.

16. crates/antares-optimization/ AUDIT

The current direction is sound:

scan → recommendation → preview diff → snapshot → apply → rollback

Important rule

Keep optimization instance-local by default. Any system-wide action must be explicit, reversible, separately permissioned, and platform-specific.

No Python requirement should be introduced for hardware probing.

17. crates/antares-mods/ AUDIT

The crate is already significantly more ambitious than a simple file scanner:

ZIP reader;

deflate decompression;

metadata readers;

health checks;

quarantine;

Java class-file reader;

heuristic security scanner;

Modrinth integration planning.

Main issue for cutover

Mod discovery/download/fix flows still depend on the broader application wiring and, until TLS is fixed, online flows cannot be considered production-complete in Rust.

Security requirements

Keep the existing safety boundary:

scan ≠ prove safe

Never execute downloaded JARs as part of scanning.

18. crates/antares-resources/ + crates/antares-visuals/

These crates demonstrate a good direction toward Python-independent deterministic tooling:

resource validation
PNG parsing/generation
content-addressed assets
crosshair rendering
FX data
HUD models
voxel model specs
static renderer

Remaining gap

The migration docs identify resource project/build/install flows and visual export/wiring as remaining work.

Cutover requirement:

resource.create/update/delete/generate/validate/build/install
visual.export/save/render

must be fully callable through Rust commands and tested without sidecar fallback.

19. legacy/python/, api/, core/, services/, infrastructure/, frontend/

These are the main Python removal targets, but they should be removed only after behavior parity is proven.

The repository's migration inventory records 118 sidecar methods across 23 groups and explicitly says the old handlers were still in use at the 2026-09-29 scan.

Therefore the correct deletion strategy is:

DO NOT:
rm -rf legacy/python

DO:
Rust implementation
    ↓
Tauri command wiring
    ↓
UI flow validation
    ↓
A/B parity
    ↓
feature flag off
    ↓
full regression
    ↓
package test
    ↓
remove Python

20. README.md, pyproject.toml, requirements.txt — DOCUMENTATION/CONFIG DRIFT

20.1 Version mismatch

Current files show:

Tauri/package/Cargo workspace → 4.0.0
pyproject.toml               → 3.0.0

This must be unified before release automation starts trusting version strings.

Recommended single source of truth:

release_version.json

or derive all release-facing versions from Cargo/Tauri release metadata.

20.2 README crate count mismatch

README describes a 9-crate Rust workspace, while the current root Cargo.toml lists 15 Rust crates plus src-tauri.

This is an architecture documentation defect, not just cosmetic drift.

20.3 CI matrix mismatch

Migration docs mention a 3-OS Rust validation, while the current CI workflow uses:

ubuntu-latest
windows-latest

Either add the third platform or correct the docs. Do not let the release checklist report a validation that CI does not perform.

21. TARGET NO-PYTHON ARCHITECTURE

┌──────────────────────────────────────────────────────────┐
│                    ANTARES DESKTOP                       │
├──────────────────────────────────────────────────────────┤
│ Vue 3 + TypeScript                                       │
│  ├─ Dashboard                                            │
│  ├─ Play                                                 │
│  ├─ Instances                                            │
│  ├─ Accounts                                             │
│  ├─ Mods                                                 │
│  ├─ Profiles                                             │
│  ├─ Resource Studio                                      │
│  ├─ Visual Studio                                        │
│  ├─ Optimization                                         │
│  ├─ Network Lab                                          │
│  ├─ Runtime                                               │
│  └─ Diagnostics                                          │
├──────────────────────────────────────────────────────────┤
│ Typed Tauri Command / Event API                          │
├──────────────────────────────────────────────────────────┤
│ Rust Application Layer                                   │
│  ├─ AppState                                              │
│  ├─ Command services                                      │
│  ├─ Task supervisor                                       │
│  ├─ EventHub                                              │
│  └─ Plugin runtime                                        │
├──────────────────────────────────────────────────────────┤
│ Rust Domain / Infrastructure                              │
│  ├─ Storage                                               │
│  ├─ Downloads + TLS                                       │
│  ├─ Processes                                             │
│  ├─ Java                                                  │
│  ├─ Minecraft launcher                                    │
│  ├─ Network                                               │
│  ├─ Profiles                                              │
│  ├─ Mods                                                  │
│  ├─ Resources                                             │
│  ├─ Visuals                                               │
│  ├─ Optimization                                          │
│  └─ Diagnostics                                           │
├──────────────────────────────────────────────────────────┤
│ Fabric Companion                                         │
└──────────────────────────────────────────────────────────┘

NO PYTHON PROCESS
NO PYWEBVIEW
NO LOCALHOST UI SERVER
NO PYTHON-ONLY BUILD STEP

22. NO-PYTHON MIGRATION ORDER

M0 — Freeze and inventory

record current Rust test result in CI;

record current frontend typecheck/lint/test/build;

record current Windows shell build;

snapshot representative user data;

snapshot profile/instance/mod/resource fixtures;

define launcher startup/memory baselines;

define supported Minecraft/Java matrix;

generate a machine-readable inventory of all 118 legacy handlers.

M1 — Fix correctness blockers in Rust

fix TaskRegistry::fail() stale return value;

add returned-message regression test;

define bounded task-history eviction;

strengthen HTTP body-limit detection;

replace fragile URL parsing with production HTTP client abstraction;

define bridge shutdown/crash ownership semantics;

add Windows process tests.

M2 — Production HTTP/TLS

add a real HTTPS client;

preserve redirect scheme;

add certificate validation tests;

add retry taxonomy for 4xx/5xx/network/TLS;

test cancellation during stream;

test resume after process restart;

verify downloads against golden checksums.

M3 — Native application service layer

Create one native service composition root.

Suggested structure:

crates/antares-app/
├── commands/
├── services/
├── runtime/
├── auth/
├── instances/
└── wiring/

The important change is architectural: Tauri commands call application services, not low-level crates directly and not Python.

M4 — Replace legacy handler groups

Recommended order:

health/app
↓
instances
↓
accounts/auth
↓
java
↓
versions
↓
play
↓
dashboard
↓
profiles
↓
mods
↓
resources
↓
visuals
↓
optimization
↓
system
↓
network
↓
runtime
↓
console/diagnostics/repair
↓
packet

After each group:

Rust command
→ UI store/service
→ golden tests
→ sidecar comparison
→ flag-off test

M5 — Cut sidecar dependency from the UI

Introduce:

ANTA_RUST_ONLY=1

or an application feature/config equivalent.

When enabled:

no legacy_call from production UI;

no sidecar auto-start;

no Python subprocess;

all main tabs must work.

M6 — Remove Python development/build dependency

Delete/replace:

requirements.txt
pyproject.toml
setup.py / Python packaging files if present
Python-only CI job
Python test commands
Python-specific devcontainer/bootstrap

Replace with:

rust-toolchain.toml
package.json / pnpm-lock.yaml
Cargo.lock
GitHub Actions Rust + Node
Windows build workflow

M7 — Remove legacy directories

Only after B15-style gates are green:

legacy/python/
frontend/legacy UI
api/
core/legacy service tree
services/
infrastructure/
legacy-only test modules

Do not delete user-data migration readers until old installations have a supported conversion path.

M8 — Production verification

clean Windows machine with no Python installed;

no Python PATH entry;

install launcher;

first-run setup;

Microsoft auth;

offline auth;

create instance;

install Minecraft version;

launch game;

download mods;

manage profiles;

resource build;

visuals export;

optimization scan/apply/rollback;

diagnostics;

runtime telemetry;

launcher close/reopen;

Windows uninstall/reinstall;

verify no Python process/file is required.

23. LEGACY HANDLER CUTOVER MATRIX

The repository's parity docs already provide the authoritative 118-method inventory. The implementation rule for the new pass should be:

Legacy group

Native target

Cutover condition

health/app

Tauri core

command parity

instances

Rust instance service

CRUD + launch fixture

accounts

Rust auth service

offline + Microsoft flows

java

antares-java

native discovery + runtime ensure

versions

manifest/version service

official metadata fixture

play

antares-launch + process

real Minecraft launch fixture

dashboard

app aggregate service

no bridge calls

profiles

antares-profiles

capture/plan/apply/revert/export/import

net

antares-net

HTTPS + ping/probe fixtures

system

antares-system

Windows/native tests

console/repair/diagnostic

antares-diagnostics

log/crash golden fixtures

optimization

antares-optimization

plan/apply/rollback

mods

antares-mods

install/health/quarantine/modpack

resources

antares-resources

project/build/install

visuals

antares-visuals

render/export/hud wiring

runtime/packet

runtime service + packet bus

real companion IPC

24. TEST STRATEGY AFTER PYTHON IS GONE

Rust unit tests

Each crate should test:

happy path
invalid input
filesystem failure
network failure
cancellation
partial state
recovery
boundary conditions
Unicode
Windows paths
long paths where supported

Cross-crate integration tests

Required:

create instance → plan launch → resolve Java → resolve artifacts → spawn

create profile → diff → apply → rollback

install mod → scan → quarantine → restore

resource import → validate → build → install

runtime start → companion pair → ingest telemetry → stop

Frontend tests

The UI should test against the native command contract, not against the legacy bridge.

E2E Windows matrix

At minimum:

Windows 10 x64
Windows 11 x64

No Python installed
Fresh profile
Existing migrated profile
Offline mode
Microsoft-authenticated state

25. CI DESIGN AFTER MIGRATION

Replace the current split Python-era pipeline with:

job: rust
  cargo fmt --check
  cargo clippy --workspace -- -D warnings
  cargo test --workspace

job: frontend
  pnpm install --frozen-lockfile
  pnpm typecheck
  pnpm lint
  pnpm test
  pnpm build

job: tauri-windows
  build frontend
  cargo check -p antares-desktop
  tauri build
  upload installer
  upload portable zip

job: e2e-windows
  install artifact
  run smoke tests
  assert python.exe is not required

The Python CI job must not be reintroduced once the cutover branch is officially no-Python.

26. SECURITY HARDENING CHECKLIST

restrictive Tauri CSP;

minimum capabilities/permissions;

no broad filesystem capability by default;

no arbitrary shell execution from UI;

download host allow/validation policy where appropriate;

checksum before commit;

secure auth-token storage;

secrets never placed in logs;

plugin permissions are scoped;

mod scanner does not execute JARs;

crash/log exports sanitize paths and tokens;

bridge code is removed rather than left dormant in production package.

27. PERFORMANCE TARGETS

Recommended budgets for a Windows-first launcher:

Desktop UI idle:             low CPU, no polling storm
Startup:                     no Python startup cost
Background telemetry:        bounded event rate
Download memory:             streaming, not body-sized buffering
Task history:                bounded
Packet history:              bounded
Log history:                 bounded
Frontend:                    lazy feature chunks
Glass effects:               limited to overlay surfaces

For Minecraft workloads, prioritize stable behavior over aggressive process priority tricks.

28. FILE-BY-FILE DELETE PLAN

Do not delete blindly. Use this order.

Delete only after replacement

legacy/python/**
api/**
services/**
infrastructure/**
legacy-only tests/**
legacy pywebview frontend/**

Then delete Python metadata

requirements.txt
pyproject.toml
Python build scripts
Python CI setup

Keep during final transition if still needed for migration

schema fixtures
old JSON readers
migration documentation
legacy test fixtures

Never delete automatically

user accounts
profile JSON
instance metadata
saved resource projects
backups
user mods
user resource packs
logs needed for diagnostics

29. ACCEPTANCE CRITERIA — “NO PYTHON” RELEASE

The release is not considered migrated until all conditions are true:

no production code starts python, python3, py, or a Python sidecar;

no production UI command routes through legacy_*;

no pywebview runtime;

no Python dependency in installer;

no Python dependency in GitHub Actions release build;

cargo test --workspace green;

frontend typecheck/lint/test/build green;

Windows Tauri build green;

clean Windows machine launches with Python absent;

HTTPS downloads work;

all critical handler groups have native parity evidence;

migration of old user data is tested;

release installer + portable artifact both verified.

30. RECOMMENDED IMPLEMENTATION BATCHES

To keep changes reviewable, use approximately 5–10 production files per batch.

Batch 01

fix TaskRegistry failure result;

add regression tests;

add task-history cap.

Batch 02

production HTTP abstraction;

TLS client;

body-limit correctness;

URL/redirect tests.

Batch 03

process Windows implementation/tests;

log mux;

scoped log storage.

Batch 04

native app composition root;

Tauri command wiring base;

typed command errors.

Batch 05

health/app/instances commands;

remove first sidecar calls from UI.

Batch 06

auth/accounts;

Java command wiring;

versions command wiring.

Batch 07

play/launch/process integration;

dashboard aggregate.

Batch 08

profiles complete;

profile import/export.

Batch 09

mods + Modrinth online paths.

Batch 10

resources + visuals.

Batch 11

optimization + system.

Batch 12

network + runtime + packet.

Batch 13

diagnostics + repair + console.

Batch 14

disable legacy_* routes;

run complete UI regression.

Batch 15

remove Python CI/build metadata;

remove Python source tree;

remove bridge crate;

cleanup docs.

Batch 16

clean Windows release build;

installer + portable artifact;

final source/security scan.

31. FINAL REFACTOR TREE

antares/
├── apps/
│   └── desktop/
│       ├── src/
│       └── public/
├── companion/
│   └── minecraft/
├── crates/
│   ├── antares-core/
│   ├── antares-storage/
│   ├── antares-app/
│   ├── antares-downloads/
│   ├── antares-process/
│   ├── antares-java/
│   ├── antares-launch/
│   ├── antares-net/
│   ├── antares-profiles/
│   ├── antares-system/
│   ├── antares-diagnostics/
│   ├── antares-optimization/
│   ├── antares-mods/
│   ├── antares-resources/
│   └── antares-visuals/
├── src-tauri/
│   ├── src/
│   │   ├── commands/
│   │   ├── events/
│   │   ├── state/
│   │   └── main.rs
│   ├── capabilities/
│   ├── icons/
│   └── tauri.conf.json
├── docs/
│   ├── architecture/
│   ├── minecraft/
│   ├── network/
│   ├── storage/
│   ├── diagnostics/
│   ├── plugins/
│   └── release/
├── tests/
├── Cargo.toml
├── Cargo.lock
├── package.json
├── pnpm-lock.yaml
└── pnpm-workspace.yaml

Notably absent:

legacy/
api/
core/legacy
services/
infrastructure/
frontend/legacy
pyproject.toml
requirements.txt

32. SOURCE EVIDENCE INDEX

Primary repository evidence used for this audit:

README.md — current architecture, folder overview, Python temporary status, migration milestones.

Cargo.toml — active Rust workspace members and workspace release profile.

pyproject.toml — legacy Python project metadata and version 3.0.0.

requirements.txt — legacy Python dependency set.

.github/workflows/ci.yml — actual CI matrix and Python job.

src-tauri/tauri.conf.json — Tauri packaging, CSP and window/bundle configuration.

src-tauri/src/lib.rs — current Tauri command registration.

src-tauri/src/state/core_state.rs — app storage and legacy bridge ownership.

crates/antares-core/src/tasks.rs — task lifecycle implementation.

crates/antares-core/src/events.rs — event QoS/buffer implementation.

crates/antares-downloads/src/http.rs — current HTTP/TLS seam.

crates/antares-process/src/spawn.rs — native process layer and test gating.

docs/migration/PARITY.md — 118 legacy methods / migration inventory.

docs/migration/BASELINE.md — historical baseline and migration status.

remake.md — long-form target architecture and migration order.

33. BOTTOM LINE FOR IMPLEMENTATION

The correct next move is not to delete Python immediately.

The correct order is:

1. Fix Rust correctness gaps
2. Add real HTTPS/TLS
3. Add native application-service wiring
4. Replace every live legacy handler group
5. Run Rust-only UI mode
6. Verify Windows on a machine with no Python
7. Remove Python CI/build metadata
8. Remove the sidecar + legacy trees
9. Update docs/version/config
10. Ship installer + portable artifact

The repository is therefore best treated as a partially completed migration, not as a broken foundation that needs a blind rewrite.

The Rust pieces that already exist should be preserved and integrated; the highest-risk work is finishing the application wiring, implementing production TLS, closing Windows/runtime test gaps, and then removing the sidecar only after a full Rust-only regression gate.