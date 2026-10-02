#!/usr/bin/env python3
"""Antares legacy sidecar — §43 Python compatibility bridge (M5/M6).

Protocol: JSON Lines trên stdio (mỗi message một dòng).
- Request:  {"id": "req-1", "method": "instances.list", "params": {}}
- Response: {"id": "req-1", "ok": true, "data": {}}
- Notification (sidecar → launcher): {"event": "...", "payload": {}}

Stdlib-only import; các service legacy được import lazily trong handler.
M5 health: health.ping / health.version / health.shutdown
M6 flows:  instances.* / accounts.* / java.list / versions.list /
           play.preflight / play.launch / dashboard.summary
M7/B6:     profiles.* — Player Profiles CRUD + plan/apply/revert (§24/§107)
M7/B7:     mods.* — Mod manager: list/search/install/remove/health/scan/quarantine
           (§11/§162–§164, heuristic scan — không tuyên bố 100% safe)
M7/B7b:    modpack.* — .mrpack install async (real Task + task.updated notify);
           mod.detail — metadata/depends/breaks cho detail panel
M8/B8a:    asset.* — Asset Library: list/import(PNG b64)/get(preview data URI)/
           delete/assign_to_project (§12 content-addressed sha256)
           resource.list/get — project catalog cho editor shell
M8/B8b:    resource.* — Pack lifecycle: wizard_info/create/update/delete(confirm)/
           generate/validate/build(+build_task/build_cancel)/builds/install/
           installed/uninstall/layer.* (§33/§126–§128/§173 — PackBuilder/
           validator/PackInstaller/LayerStore qua ResourceStudioService)
M9/B9:     visual.* — Visual Studio: crosshair presets/render/export_pack,
           totem presets/render/model(get/save/render 3D)/export_pack,
           hud widgets/save_layout, fx defaults/render hit/particle/export
           (§129–§135 — VisualStudioService, mọi preview trả data URI b64)
M10/B10:   optimization.* — Game optimization: scan/plan (preview diff —
           không ghi)/apply (snapshot trước)/rollback/snapshot_info
           (§3/§70 — GameOptimizationService, instance-local §74)
           system.* — System optimization: overview/cleanup scan/clean/
           undo/empty_trash/power status/set_plan (§4/§35/§36/§37 —
           SystemOptimizationService; risk/reversible/requiresAdmin)
M11/B11:   net.* — Network Lab: endpoints (TCP launcher cần)/tcp/mc_ping
           (Server List Ping §42)/probe (RTT + jitter + network timeline)/
           dns (getaddrinfo) — NetDiagnostics + composition sidecar
M12/B12:   runtime.* — Companion runtime: endpoint/sessions/metrics(§12
           ring RAM)/start/pairing + packet ingest (valid schema v1);
           packet.* — Inspector ring buffer §121/§122 (metadata mode,
           cap 10k, oldest-first eviction, export on-request)
M13/B13:   console.* — Log center: sources/read (virtualized §16)/
           analyze/insights (blueprint — không LLM, mục 41); repair.* —
           actions/scan (dry-run "What will change?" mục 39)/run;
           diagnostic.export — gói evidence JSON on-request
"""

from __future__ import annotations

import json
import platform
import sys
import threading
import time
from pathlib import Path
from typing import Any, Callable

# Cho phép chạy từ bất kỳ cwd nào: thêm repo root vào sys.path
REPO_ROOT = Path(__file__).resolve().parent.parent.parent
if str(REPO_ROOT) not in sys.path:
    sys.path.insert(0, str(REPO_ROOT))

PROTOCOL_VERSION = 1
SERVICE_NAME = "antares-legacy"
SERVICE_VERSION = "4.0.0"

MAX_LINE_BYTES = 8 * 1024 * 1024  # đồng bộ với MAX_PAYLOAD_BYTES Rust
STDOUT_LOCK = threading.Lock()


def write_message(message: dict[str, Any]) -> None:
    """Ghi một JSON line ra stdout atomically (thread-safe)."""
    line = json.dumps(message, separators=(",", ":"))
    with STDOUT_LOCK:
        sys.stdout.write(line + "\n")
        sys.stdout.flush()


def notify(event: str, payload: Any = None) -> None:
    """Gửi notification (không id) — launcher forward thành event bridge."""
    write_message({"event": event, "payload": payload})


# ---------------------------------------------------------------------------
# Legacy context bootstrap — dùng AppContext thật của source hiện tại.
# Import lỗi (máy không đủ deps) → service methods trả error envelope gọn.
# ---------------------------------------------------------------------------

_LEGACY_CTX: Any = None
_LEGACY_CTX_ERROR: str | None = None


def _legacy_ctx() -> Any:
    global _LEGACY_CTX, _LEGACY_CTX_ERROR
    if _LEGACY_CTX is not None or _LEGACY_CTX_ERROR is not None:
        return _LEGACY_CTX
    try:
        from app.context import AppContext

        ctx = AppContext.instance() if hasattr(AppContext, "instance") else AppContext()
        _LEGACY_CTX = ctx
        return ctx
    except Exception as err:  # noqa: BLE001 — sidecar phải sống
        _LEGACY_CTX_ERROR = f"{type(err).__name__}: {err}"
        return None


def _legacy_unavailable() -> BridgeMethodError:
    detail = _LEGACY_CTX_ERROR or "legacy context not initialized"
    return BridgeMethodError("LEGACY_UNAVAILABLE", detail)


class BridgeMethodError(Exception):
    def __init__(self, code: str, message: str) -> None:
        super().__init__(message)
        self.code = code


# ---------------------------------------------------------------------------
# M5 health handlers
# ---------------------------------------------------------------------------


def handle_health_ping(params: dict[str, Any]) -> dict[str, Any]:
    return {"pong": True, "ts": params.get("ts")}


def handle_health_version(_params: dict[str, Any]) -> dict[str, Any]:
    return {
        "protocol": PROTOCOL_VERSION,
        "service": SERVICE_NAME,
        "serviceVersion": SERVICE_VERSION,
        "python": platform.python_version(),
        "legacyAvailable": _LEGACY_CTX is not None or _try_bootstrap_quiet(),
    }


def _try_bootstrap_quiet() -> bool:
    return _legacy_ctx() is not None


def handle_health_shutdown(_params: dict[str, Any]) -> dict[str, Any]:
    threading.Thread(target=_delayed_exit, daemon=True).start()
    return {"stopping": True}


def _delayed_exit() -> None:
    import time

    time.sleep(0.05)
    sys.exit(0)


def handle_app_echo(params: dict[str, Any]) -> Any:
    return params


def handle_app_storage_root(_params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    root = getattr(ctx, "paths", None)
    data_root = getattr(root, "data", None) if root else None
    return {"root": str(data_root) if data_root else str(REPO_ROOT)}


# ---------------------------------------------------------------------------
# M6 — instances
# ---------------------------------------------------------------------------


def handle_instances_list(_params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instances = ctx.get("instances").list()
    return {"instances": instances}


def handle_instances_get(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance = ctx.get("instances").get(params.get("instanceId", ""))
    if instance is None:
        raise BridgeMethodError("MC_VERSION_UNKNOWN", f"instance not found: {params.get('instanceId')}")
    return {"instance": instance}


def handle_instances_create(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    name = str(params.get("name", "")).strip()
    version = str(params.get("minecraftVersion", "")).strip()
    if not name or not version:
        raise BridgeMethodError("CONFIG_INVALID", "name and minecraftVersion are required")
    instance = ctx.get("instances").create(
        name,
        version,
        loader=params.get("loader", "vanilla"),
        memory_max_mb=int(params.get("memoryMaxMb", 2048)),
        memory_min_mb=int(params.get("memoryMinMb", 512)),
    )
    # tạo mới → auto select (khớp behavior api.py instances_create)
    ctx.config.set("selectedInstance", instance["id"], flush_now=True)
    notify("instances.changed", {"action": "create", "instanceId": instance["id"]})
    return {"instance": instance}


def handle_instances_select(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", ""))
    ctx.config.set("selectedInstance", instance_id, flush_now=True)
    notify("instances.changed", {"action": "select", "instanceId": instance_id})
    return {"selected": instance_id}


# ---------------------------------------------------------------------------
# M6 — accounts
# ---------------------------------------------------------------------------


def handle_accounts_list(_params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    accounts = ctx.get("accounts").list()
    return {"accounts": [a for a in accounts if "token" not in a]}


def handle_accounts_select(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    account_id = str(params.get("accountId", ""))
    ok = ctx.get("accounts").select(account_id)
    if not ok:
        raise BridgeMethodError("AUTH_FAILED", f"account not found: {account_id}")
    notify("selection.changed", {"accountId": account_id})
    return {"selected": account_id}


# ---------------------------------------------------------------------------
# M6 — java / versions
# ---------------------------------------------------------------------------


def handle_java_list(_params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    from services.java.discovery import java_info, scan_system_java

    homes = ctx.get("java_homes") or scan_system_java()
    infos = [info for h in homes if (info := java_info(h))]
    return {"javas": infos}


def handle_versions_list(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    from services.loaders.registry import get_loader

    loader = get_loader(str(params.get("loader", "vanilla")))
    return {"versions": loader.list_versions()}


# ---------------------------------------------------------------------------
# M6 — play preflight + launch (§8/§30)
# ---------------------------------------------------------------------------

# java major tối thiểu theo MC version — chỉ dùng cho preflight heuristic
def _required_java_major(mc_version: str) -> int:
    try:
        parts = [int(p) for p in mc_version.split(".")[:2]]
        minor = parts[1] if len(parts) > 1 else 0
    except ValueError:
        return 21  # snapshot/latest → 21
    if minor >= 20:
        return 21
    if minor >= 18:
        return 17
    if minor >= 17:
        return 16
    return 8


def _check_disk_space(ctx: Any, min_mb: int = 512) -> dict[str, Any]:
    import shutil

    usage = shutil.disk_usage(ctx.paths.instances if hasattr(ctx.paths, "instances") else ".")
    free_mb = usage.free // (1024 * 1024)
    return {"ok": free_mb >= min_mb, "freeMb": int(free_mb), "minMb": min_mb}


def handle_play_preflight(params: dict[str, Any]) -> dict[str, Any]:
    """§8/§30 — Preflight PASS/WARNING/ERROR cho instance, không spawn process."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", ""))
    instance = ctx.get("instances").get(instance_id)
    if instance is None:
        raise BridgeMethodError("INSTANCE_NOT_FOUND", f"instance not found: {instance_id}")

    checks: list[dict[str, Any]] = []

    # Java
    try:
        from services.java.discovery import java_info, scan_system_java

        homes = ctx.get("java_homes") or scan_system_java()
        javas = [info for h in homes if (info := java_info(h))]
        required = _required_java_major(str(instance.get("minecraftVersion", "")))
        ok = any(j["major"] >= required for j in javas)
        checks.append({
            "id": "java",
            "label": f"Java ≥ {required}",
            "status": "pass" if ok else "error",
            "detail": f"found {len(javas)} runtime(s)" if javas else "no Java found",
        })
    except Exception as err:  # noqa: BLE001
        checks.append({"id": "java", "label": "Java", "status": "error", "detail": str(err)})

    # Version JSON (đã cài?)
    version = str(instance.get("minecraftVersion", ""))
    checks.append({
        "id": "version",
        "label": "Version",
        "status": "pass",
        "detail": f"{version} / {instance.get('loader', 'vanilla')}",
    })

    # Account
    account = ctx.get("accounts").get_current()
    checks.append({
        "id": "account",
        "label": "Account",
        "status": "pass" if account else "warning",
        "detail": account["displayName"] if account else "no account selected",
    })

    # Disk
    try:
        disk = _check_disk_space(ctx)
        checks.append({
            "id": "disk",
            "label": "Disk",
            "status": "pass" if disk["ok"] else "error",
            "detail": f"{disk['freeMb']} MB free",
        })
    except Exception as err:  # noqa: BLE001
        checks.append({"id": "disk", "label": "Disk", "status": "warning", "detail": str(err)})

    # Mods (optional — chỉ warning nếu có file .jar lỗi tên)
    mods_dir = ctx.paths.instances / instance_id / "game" / "mods"
    mods_count = 0
    if mods_dir.exists():
        mods_count = len([p for p in mods_dir.glob("*.jar")])
    checks.append({
        "id": "mods",
        "label": "Mods",
        "status": "pass",
        "detail": f"{mods_count} installed",
    })

    blockers = sum(1 for c in checks if c["status"] == "error")
    warnings = sum(1 for c in checks if c["status"] == "warning")
    return {
        "instanceId": instance_id,
        "checks": checks,
        "blockers": blockers,
        "warnings": warnings,
        "canPlay": blockers == 0,
    }


def handle_play_launch(params: dict[str, Any]) -> dict[str, Any]:
    """§29 — Launch thật qua LaunchOrchestrator (async, trả taskId)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", ""))
    api = _legacy_api()
    if api is None:
        raise _legacy_unavailable()
    return api.minecraft_launch(instance_id)


_LEGACY_API: Any = None


def _legacy_api() -> Any:
    global _LEGACY_API
    if _LEGACY_API is not None:
        return _LEGACY_API
    ctx = _legacy_ctx()
    if ctx is None:
        return None
    try:
        from api.bridge.api import AntaresApi

        _LEGACY_API = AntaresApi(ctx)
        return _LEGACY_API
    except Exception:  # noqa: BLE001
        return None


# ---------------------------------------------------------------------------
# M6 — dashboard summary (§7)
# ---------------------------------------------------------------------------


def handle_dashboard_summary(_params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()

    instances = ctx.get("instances").list()
    account = ctx.get("accounts").get_current()
    selected = ctx.config.get("selectedInstance")

    # lastPlayed instance
    played = [i for i in instances if i.get("lastPlayedAt")]
    recent = max(played, key=lambda i: i["lastPlayedAt"]) if played else None

    return {
        "appVersion": SERVICE_VERSION,
        "instanceCount": len(instances),
        "selectedInstanceId": selected,
        "recentInstanceId": recent["id"] if recent else None,
        "recentInstanceName": recent["name"] if recent else None,
        "account": {"id": account["id"], "displayName": account["displayName"]}
        if account else None,
        "legacyAvailable": True,
    }


# ---------------------------------------------------------------------------
# B6 — profiles (§24/§106/§107 — ProfileService compatibility)
# ---------------------------------------------------------------------------


def _profiles_service(ctx: Any) -> Any:
    """ProfileService thật của legacy — 1 điểm import, không seed data."""
    from services.profiles.service import ProfileService

    return ProfileService(ctx)


def handle_profiles_list(_params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    return {"profiles": _profiles_service(ctx).list()}


def handle_profiles_get(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    profile = _profiles_service(ctx).get(str(params.get("profileId", "")))
    if profile is None:
        raise BridgeMethodError("PROFILE_NOT_FOUND", f"profile not found: {params.get('profileId')}")
    return {"profile": profile}


def handle_profiles_create(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    name = str(params.get("name", "")).strip()
    spec = params.get("spec")
    if not name:
        raise BridgeMethodError("CONFIG_INVALID", "name is required")
    if spec is not None and not isinstance(spec, dict):
        raise BridgeMethodError("CONFIG_INVALID", "spec must be an object")
    profile = _profiles_service(ctx).create(name, spec or {})
    notify("profiles.changed", {"action": "create", "profileId": profile["id"]})
    return {"profile": profile}


def handle_profiles_duplicate(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    profile = _profiles_service(ctx).duplicate(str(params.get("profileId", "")))
    notify("profiles.changed", {"action": "create", "profileId": profile["id"]})
    return {"profile": profile}


def handle_profiles_update(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    patch = params.get("patch")
    if not isinstance(patch, dict):
        raise BridgeMethodError("CONFIG_INVALID", "patch must be an object")
    profile = _profiles_service(ctx).update(str(params.get("profileId", "")), patch)
    notify("profiles.changed", {"action": "update", "profileId": profile["id"]})
    return {"profile": profile}


def handle_profiles_delete(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    if not params.get("confirm"):
        raise BridgeMethodError("CONFIG_INVALID", "Delete requires explicit confirmation")
    ok = _profiles_service(ctx).delete(str(params.get("profileId", "")), confirm=True)
    if not ok:
        raise BridgeMethodError("PROFILE_NOT_FOUND", f"profile not found: {params.get('profileId')}")
    notify("profiles.changed", {"action": "delete", "profileId": str(params.get("profileId", ""))})
    return {"deleted": True}


def handle_profiles_capture(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", ""))
    profile = _profiles_service(ctx).capture(
        instance_id,
        str(params["name"]) if params.get("name") else None,
    )
    notify("profiles.changed", {"action": "create", "profileId": profile["id"]})
    return {"profile": profile}


def handle_profiles_plan(params: dict[str, Any]) -> dict[str, Any]:
    """§107 — Diff dự kiến, không ghi gì."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    return {"plan": _profiles_service(ctx).plan(str(params.get("profileId", "")))}


def handle_profiles_apply(params: dict[str, Any]) -> dict[str, Any]:
    """§106 — Apply = patch current state (account/instance/jvm/game)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    result = _profiles_service(ctx).apply(str(params.get("profileId", "")))
    return {"applied": result}


def handle_profiles_revert(_params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    ok = _profiles_service(ctx).revert()
    return {"reverted": ok}


def handle_profiles_export(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    return {"export": _profiles_service(ctx).export_data(str(params.get("profileId", "")))}


def handle_profiles_import(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    data = params.get("data")
    if not isinstance(data, dict):
        raise BridgeMethodError("CONFIG_INVALID", "data must be an antares-profile object")
    profile = _profiles_service(ctx).import_data(data)
    notify("profiles.changed", {"action": "create", "profileId": profile["id"]})
    return {"profile": profile}


def handle_profiles_validate(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    spec = params.get("spec")
    if not isinstance(spec, dict):
        raise BridgeMethodError("CONFIG_INVALID", "spec must be an object")
    return {"issues": _profiles_service(ctx).validate_spec(spec)}


# ---------------------------------------------------------------------------
# B7 — mods (§11/§162–§164 — ModService + ModSecurity compatibility)
# ---------------------------------------------------------------------------


def _mods_service(ctx: Any) -> Any:
    """ModService thật — cần HttpClient trong ctx (seeded bởi legacy bootstrap)."""
    from services.mods.service import ModService

    return ModService(ctx, ctx.get("http_client"))


def _mod_security(ctx: Any) -> Any:
    from services.mods.security import ModSecurityService

    return ModSecurityService(ctx)


def _require_instance(ctx: Any, instance_id: str) -> dict[str, Any]:
    instance = ctx.get("instances").get(instance_id)
    if instance is None:
        raise BridgeMethodError("INSTANCE_NOT_FOUND", f"instance not found: {instance_id}")
    return instance


def handle_mods_list(params: dict[str, Any]) -> dict[str, Any]:
    """Installed mods + verdict hiện biết (readable only — không execute JAR §11)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", ""))
    _require_instance(ctx, instance_id)
    filenames = _mods_service(ctx).list_installed(instance_id)
    mods = [{"filename": f} for f in filenames]
    # Gắn metadata + verdict nếu có cache scan (không scan lại ở list — §208)
    try:
        health = _mod_security(ctx).health_check(
            instance_id,
            loader=str(ctx.get("instances").get(instance_id).get("loader", "vanilla")),
            mc_version=str(ctx.get("instances").get(instance_id).get("minecraftVersion", "")),
            include_outdated=False,
        )
        by_file = {m["filename"]: m for m in health.get("mods", [])}
        for mod in mods:
            meta = by_file.get(mod["filename"])
            if meta:
                mod["modId"] = meta.get("modId")
                mod["name"] = meta.get("name")
                mod["version"] = meta.get("version")
                mod["loader"] = meta.get("loader")
                mod["readable"] = meta.get("readable", False)
    except Exception:  # noqa: BLE001 — list phải luôn trả; metadata là best-effort
        pass
    return {"instanceId": instance_id, "mods": mods}


def handle_mods_search(params: dict[str, Any]) -> dict[str, Any]:
    """Modrinth search — online-only (§68: offline trả error, UI hiện cached)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    query = str(params.get("query", "")).strip()
    if not query:
        raise BridgeMethodError("CONFIG_INVALID", "query is required")
    hits = _mods_service(ctx).search(
        query,
        loader=str(params.get("loader", "fabric")),
        mc_version=str(params.get("mcVersion", "1.21")),
    )
    return {"hits": hits}


def handle_mods_install(params: dict[str, Any]) -> dict[str, Any]:
    """Install qua DownloadManager (checksum/resume) — sync; UI dùng task row."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    project_id = str(params.get("projectId", ""))
    instance_id = str(params.get("instanceId", ""))
    instance = _require_instance(ctx, instance_id)
    filename = _mods_service(ctx).install(
        project_id, instance_id,
        loader=str(params.get("loader") or instance.get("loader", "fabric")),
        mc_version=str(params.get("mcVersion") or instance.get("minecraftVersion", "")),
    )
    notify("mods.changed", {"action": "install", "instanceId": instance_id,
                            "filename": filename})
    return {"filename": filename}


def handle_mods_remove(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", ""))
    filename = str(params.get("filename", ""))
    if not filename or "/" in filename or "\\" in filename or ".." in filename:
        raise BridgeMethodError("CONFIG_INVALID", "invalid filename")
    removed = _mods_service(ctx).uninstall(instance_id, filename)
    if not removed:
        raise BridgeMethodError("MOD_NOT_FOUND", f"mod not found: {filename}")
    notify("mods.changed", {"action": "remove", "instanceId": instance_id,
                            "filename": filename})
    return {"removed": True}


def handle_mods_health(params: dict[str, Any]) -> dict[str, Any]:
    """§162 — dependency/compat graph dạng issues list (deps thiếu, wrong loader)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", ""))
    instance = _require_instance(ctx, instance_id)
    result = _mod_security(ctx).health_check(
        instance_id,
        loader=str(instance.get("loader", "vanilla")),
        mc_version=str(instance.get("minecraftVersion", "")),
        include_outdated=bool(params.get("includeOutdated", False)),
    )
    return {"health": result}


def handle_mods_scan(params: dict[str, Any]) -> dict[str, Any]:
    """§164 — heuristic scan toàn bộ mods dir; tự quarantine DANGEROUS.

    Sync — scan local JAR nhanh (bytecode heuristic, không network).
    """
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", ""))
    _require_instance(ctx, instance_id)
    result = _mod_security(ctx).scan_instance(instance_id, _noop_task())
    notify("mods.changed", {"action": "scan", "instanceId": instance_id})
    return {"scan": result}


class _NoopTask:
    """Task-like object cho service yêu cầu task — sidecar chạy sync."""

    cancelled = False
    progress = 0.0
    message = ""


def _noop_task() -> _NoopTask:
    return _NoopTask()


def handle_mods_autofix(params: dict[str, Any]) -> dict[str, Any]:
    """Auto-fix deps thiếu — sync qua auto_fix_instance (task=None acceptable)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", ""))
    instance = _require_instance(ctx, instance_id)
    result = _mod_security(ctx).auto_fix(
        instance_id,
        loader=str(instance.get("loader", "fabric")),
        mc_version=str(instance.get("minecraftVersion", "1.21")),
        task=_noop_task(),
    )
    notify("mods.changed", {"action": "autofix", "instanceId": instance_id})
    return {"fix": result}


def handle_mods_quarantine_list(_params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    return {"items": _mod_security(ctx).quarantine_list()}


def handle_mods_quarantine_restore(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    entry = _mod_security(ctx).quarantine_restore(str(params.get("quarantineFile", "")))
    notify("mods.changed", {"action": "quarantine_restore",
                            "instanceId": entry.get("instanceId")})
    return {"restored": entry.get("originalName")}


def handle_mods_quarantine_delete(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    if not params.get("confirm"):
        raise BridgeMethodError("CONFIG_INVALID", "Delete requires explicit confirmation")
    deleted = _mod_security(ctx).quarantine_delete(str(params.get("quarantineFile", "")))
    if not deleted:
        raise BridgeMethodError("MOD_NOT_FOUND", f"quarantine entry not found: {params.get('quarantineFile')}")
    return {"deleted": True}


# ---------------------------------------------------------------------------
# B7b — modpack (.mrpack) + mod detail (§11/§103 — async task + metadata)
# ---------------------------------------------------------------------------

_TASK_LISTENER_WIRED = False


def _ensure_task_listener(ctx: Any) -> None:
    """Wire 1 lần: TaskManager updates → notify('task.updated') cho event bridge.

    UI dùng poll modpack.status (1s coalesce §171) — notify chỉ là side channel.
    """
    global _TASK_LISTENER_WIRED
    if _TASK_LISTENER_WIRED:
        return

    def on_task_update(task: Any) -> None:
        try:
            notify("task.updated", task.to_dict())
        except Exception:  # noqa: BLE001 — listener không được giết sidecar
            pass

    ctx.tasks.set_update_listener(on_task_update)
    _TASK_LISTENER_WIRED = True


def _safe_mod_filename(filename: str) -> str:
    if not filename or "/" in filename or "\\" in filename or ".." in filename:
        raise BridgeMethodError("CONFIG_INVALID", "invalid filename")
    return filename


def handle_mod_detail(params: dict[str, Any]) -> dict[str, Any]:
    """§11 mod detail — metadata + depends/recommends/breaks (read-only)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", ""))
    filename = _safe_mod_filename(str(params.get("filename", "")))
    _require_instance(ctx, instance_id)

    from services.mods.health import read_mod_info

    jar = ctx.paths.instances / instance_id / "game" / "mods" / filename
    if not jar.exists():
        raise BridgeMethodError("MOD_NOT_FOUND", f"mod not found: {filename}")
    info = read_mod_info(jar)
    return {"detail": {
        "filename": info.filename,
        "modId": info.mod_id,
        "name": info.name,
        "version": info.version,
        "loader": info.loader,
        "mcVersions": list(info.mc_versions),
        "depends": dict(info.depends),
        "recommends": dict(info.recommends),
        "breaks": dict(info.breaks),
        "fabricModJson": info.fabric_mod_json,
        "forgeToml": info.forge_toml,
        "readable": info.readable,
    }}


def handle_modpack_info(params: dict[str, Any]) -> dict[str, Any]:
    """Preview .mrpack trước khi install — name/loader/version/optional files."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    path = str(params.get("mrpackPath", ""))
    if not path or not Path(path).exists():
        raise BridgeMethodError("CONFIG_INVALID", f"file not found: {path}")
    from services.mods.mrpack import read_mrpack_info

    return {"info": read_mrpack_info(Path(path))}


def handle_modpack_install(params: dict[str, Any]) -> dict[str, Any]:
    """Install .mrpack async (§90 Task) — trả taskId; progress qua task.updated.

    Pipeline service: download files (sha1) → extract overrides → install deps
    → update instance metadata (mrpack.py — path-safe ensure_inside).
    """
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    path = str(params.get("mrpackPath", ""))
    instance_id = str(params.get("instanceId", ""))
    _require_instance(ctx, instance_id)
    if not path or not Path(path).exists():
        raise BridgeMethodError("CONFIG_INVALID", f"file not found: {path}")

    from services.mods.mrpack import install_mrpack, read_mrpack_info

    read_mrpack_info(Path(path))  # validate trước khi trả taskId (khớp api.py)

    task = ctx.tasks.create("MODPACK_INSTALL", owner=f"instance:{instance_id}")
    ctx.tasks.start(task)
    _ensure_task_listener(ctx)

    optional = params.get("optionalSelected")
    optional_list = [str(x) for x in optional] if isinstance(optional, list) else None

    def work() -> None:
        try:
            result = install_mrpack(ctx, Path(path), instance_id, task,
                                    optional_selected=optional_list)
            ctx.tasks.complete(task, result)
        except Exception as err:  # noqa: BLE001
            to_dict = getattr(err, "to_dict", None)
            err_payload = None
            if callable(to_dict):
                try:
                    candidate = to_dict()
                    err_payload = candidate if isinstance(candidate, dict) else None
                except Exception:  # noqa: BLE001
                    err_payload = None
            ctx.tasks.fail(task, err_payload or {
                "code": "MODPACK_INSTALL_FAILED", "message": str(err)})
        finally:
            notify("mods.changed", {"action": "modpack", "instanceId": instance_id,
                                    "taskId": task.id})

    import threading

    threading.Thread(target=work, daemon=True).start()
    return {"taskId": task.id}


def handle_modpack_status(params: dict[str, Any]) -> dict[str, Any]:
    """Poll task state — UI 1s cadence (§171), trả to_dict + result."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    task_id = str(params.get("taskId", ""))
    task = ctx.tasks.get(task_id)
    if task is None:
        raise BridgeMethodError("TASK_NOT_FOUND", f"task not found: {task_id}")
    data = task.to_dict()
    data["result"] = task.result
    return {"task": data}


def handle_modpack_cancel(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    task_id = str(params.get("taskId", ""))
    cancelled = ctx.tasks.cancel(task_id)
    if not cancelled:
        raise BridgeMethodError("TASK_NOT_FOUND", f"task not found or not cancellable: {task_id}")
    return {"cancelled": True}


# ---------------------------------------------------------------------------
# B8a — asset library + resource projects (§12 — Resource Studio)
# ---------------------------------------------------------------------------


def _asset_store(ctx: Any) -> Any:
    from services.resources.assets import AssetStore

    return AssetStore(ctx)


def _resource_studio(ctx: Any) -> Any:
    from services.resources.service import ResourceStudioService

    return ResourceStudioService(ctx)


def handle_asset_list(params: dict[str, Any]) -> dict[str, Any]:
    """§12 asset catalog — query/category/tag filter + sort phía service."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    assets = _asset_store(ctx).list(
        query=str(params.get("query", "")),
        category=str(params.get("category", "")),
        tag=str(params.get("tag", "")),
        sort=str(params.get("sort", "newest")),
    )
    return {"assets": assets}


def handle_asset_import(params: dict[str, Any]) -> dict[str, Any]:
    """Import PNG (base64 từ FileReader — §12.1 validate→hash→content-addressed).

    Service tự chặn: size >8MB, signature, dims >4096, decode fail.
    """
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    import base64

    data_b64 = str(params.get("dataB64", ""))
    if not data_b64:
        raise BridgeMethodError("CONFIG_INVALID", "dataB64 is required")
    try:
        data = base64.b64decode(data_b64, validate=True)
    except Exception as err:
        raise BridgeMethodError("CONFIG_INVALID", f"invalid base64: {err}") from err
    entry = _asset_store(ctx).import_png(
        data,
        name=str(params["name"]) if params.get("name") else None,
        category=str(params.get("category", "item")),
        tags=[str(t) for t in params["tags"]] if isinstance(params.get("tags"), list) else None,
    )
    notify("assets.changed", {"action": "import", "assetId": entry["id"]})
    return {"asset": entry}


def handle_asset_get(params: dict[str, Any]) -> dict[str, Any]:
    """Asset metadata + preview data URI (việc render là của UI)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    store = _asset_store(ctx)
    asset_id = str(params.get("assetId", ""))
    asset = store.get(asset_id)
    if asset is None:
        raise BridgeMethodError("ASSET_NOT_FOUND", f"asset not found: {asset_id}")
    return {"asset": asset, "preview": store.png_data_uri(asset_id)}


def handle_asset_delete(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    deleted = _asset_store(ctx).delete(str(params.get("assetId", "")))
    if not deleted:
        raise BridgeMethodError("ASSET_NOT_FOUND", f"asset not found: {params.get('assetId')}")
    notify("assets.changed", {"action": "delete", "assetId": str(params.get("assetId", ""))})
    return {"deleted": True}


def handle_asset_assign(params: dict[str, Any]) -> dict[str, Any]:
    """Gán asset vào project path (assets/<ns>/textures/…) — validate phía service."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    result = _asset_store(ctx).assign_to_project(
        str(params.get("projectId", "")),
        str(params.get("assetId", "")),
        str(params.get("targetRel", "")),
    )
    notify("assets.changed", {"action": "assign", "assetId": str(params.get("assetId", "")),
                              "projectId": str(params.get("projectId", ""))})
    return {"assigned": result}


def handle_resource_list(_params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    return {"projects": _resource_studio(ctx).list()}


def handle_resource_get(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    project_id = str(params.get("projectId", ""))
    try:
        project = _resource_studio(ctx).get(project_id)
    except Exception as err:  # noqa: BLE001 — AntaresError giữ code, còn lại NOT_FOUND
        raise BridgeMethodError("RESOURCE_PROJECT_NOT_FOUND",
                                f"project not found: {project_id}") from err
    return {"project": project}


# ---------------------------------------------------------------------------
# B8b — resource pack lifecycle (§33/§126–§128/§173 — build/validate/install)
# ---------------------------------------------------------------------------


def handle_resource_wizard_info(_params: dict[str, Any]) -> dict[str, Any]:
    """§10.2 — Wizard metadata: templates + versions đã xác minh + default."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    return _resource_studio(ctx).wizard_info()


def handle_resource_create(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    name = str(params.get("name", "")).strip()
    mc_version = str(params.get("mcVersion", "")).strip()
    if not name or not mc_version:
        raise BridgeMethodError("CONFIG_INVALID", "name and mcVersion are required")
    project = _resource_studio(ctx).create(
        name,
        mc_version,
        template=str(params.get("template", "minimal")),
        description=str(params.get("description", "")),
    )
    notify("resources.changed", {"action": "create", "projectId": project["id"]})
    return {"project": project}


def handle_resource_update(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    patch = params.get("patch")
    if not isinstance(patch, dict):
        raise BridgeMethodError("CONFIG_INVALID", "patch must be an object")
    project = _resource_studio(ctx).update(str(params.get("projectId", "")), patch)
    notify("resources.changed", {"action": "update", "projectId": project["id"]})
    return {"project": project}


def handle_resource_delete(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    if not params.get("confirm"):
        raise BridgeMethodError("CONFIG_INVALID", "Delete requires explicit confirmation")
    # ProjectStore.delete: dir_of raise FILE_NOT_FOUND nếu project không tồn tại
    ok = _resource_studio(ctx).delete(str(params.get("projectId", "")))
    notify("resources.changed", {"action": "delete", "projectId": str(params.get("projectId", ""))})
    return {"deleted": ok}


def handle_resource_generate(params: dict[str, Any]) -> dict[str, Any]:
    """Regenerate generated/ từ template — idempotent (PackBuilder.generate)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    project_id = str(params.get("projectId", ""))
    result = _resource_studio(ctx).generate(project_id)
    notify("resources.changed", {"action": "generate", "projectId": project_id})
    return {"generated": result}


def handle_resource_validate(params: dict[str, Any]) -> dict[str, Any]:
    """§127 — findings list {code, severity, path, detail}; FAIL chỉ khi ERROR."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    return _resource_studio(ctx).validate(str(params.get("projectId", "")))


def handle_resource_build(params: dict[str, Any]) -> dict[str, Any]:
    """§126 — Validate → ZIP + sha256 manifest. ERROR → VALIDATION_FAILED."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    project_id = str(params.get("projectId", ""))
    manifest = _resource_studio(ctx).build(project_id)
    notify("resources.changed", {"action": "build", "projectId": project_id,
                                 "file": manifest.get("file")})
    return {"manifest": manifest}


def handle_resource_build_task(params: dict[str, Any]) -> dict[str, Any]:
    """Build dưới TaskManager thật (§17) — progress qua task.updated notify.

    Sync call (build local ZIP nhanh); taskId nằm trong TaskManager, UI cancel
    qua resource.build_cancel. Fail-fast: project sai → AntaresError giữ code.
    """
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    _ensure_task_listener(ctx)
    result = _resource_studio(ctx).build_task(str(params.get("projectId", "")))
    return {"manifest": result["manifest"]}


def handle_resource_build_cancel(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    task_id = str(params.get("taskId", ""))
    cancelled = ctx.tasks.cancel(task_id)
    if not cancelled:
        raise BridgeMethodError("TASK_NOT_FOUND", f"task not found: {task_id}")
    return {"cancelled": True}


def handle_resource_builds(params: dict[str, Any]) -> dict[str, Any]:
    """Danh sách build (mới nhất trước, không kèm fileHashes — chi tiết quá lớn)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    builds = _resource_studio(ctx).list_builds(str(params.get("projectId", "")))
    return {"builds": builds}


def handle_resource_install(params: dict[str, Any]) -> dict[str, Any]:
    """§33/§75 — Build nếu chưa có rồi copy ZIP vào instance resourcepacks.

    Không overwrite silent — file trùng tên cần overwrite=True (service backup).
    """
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    project_id = str(params.get("projectId", ""))
    instance_id = str(params.get("instanceId", ""))
    _require_instance(ctx, instance_id)
    result = _resource_studio(ctx).install(
        project_id, instance_id, overwrite=bool(params.get("overwrite", False)))
    notify("resources.changed", {"action": "install", "projectId": project_id,
                                 "instanceId": instance_id, "file": result.get("file")})
    return {"installed": result}


def handle_resource_installed(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    packs = _resource_studio(ctx).installed(str(params.get("instanceId", "")))
    return {"packs": packs}


def handle_resource_uninstall(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", ""))
    filename = str(params.get("filename", ""))
    if not filename or "/" in filename or "\\" in filename or ".." in filename:
        raise BridgeMethodError("CONFIG_INVALID", "invalid filename")
    removed = _resource_studio(ctx).uninstall(instance_id, filename)
    if not removed:
        raise BridgeMethodError("FILE_NOT_FOUND", f"pack not installed: {filename}")
    notify("resources.changed", {"action": "uninstall", "instanceId": instance_id,
                                 "file": filename})
    return {"removed": True}


def handle_resource_layer_get(params: dict[str, Any]) -> dict[str, Any]:
    """§173 — Layer order (thấp→cao priority) + effective preview/conflicts."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    studio = _resource_studio(ctx)
    instance_id = str(params.get("instanceId", ""))
    order = studio.layers.get_order(instance_id)
    return {"order": order, "preview": studio.layers._preview_for(instance_id, order)}


def handle_resource_layer_set(params: dict[str, Any]) -> dict[str, Any]:
    """§173 — Lưu order (validate installed/dup) + sync options.txt atomically."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    order = params.get("order")
    if not isinstance(order, list):
        raise BridgeMethodError("CONFIG_INVALID", "order must be a list of pack filenames")
    instance_id = str(params.get("instanceId", ""))
    result = _resource_studio(ctx).layers.set_order(
        instance_id, [str(n) for n in order])
    notify("resources.changed", {"action": "layer_set", "instanceId": instance_id})
    return {"order": result}


def handle_resource_layer_move(params: dict[str, Any]) -> dict[str, Any]:
    """Move pack trong layer — delta âm = lên = tăng priority."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", ""))
    filename = str(params.get("filename", ""))
    result = _resource_studio(ctx).layers.move(
        instance_id, filename, int(params.get("delta", 0)))
    notify("resources.changed", {"action": "layer_move", "instanceId": instance_id,
                                 "file": filename})
    return {"order": result}


# ---------------------------------------------------------------------------
# B9 — visual studio (§129–§135 — crosshair/totem/hud/fx qua VisualStudioService)
# ---------------------------------------------------------------------------


def _visual_studio(ctx: Any) -> Any:
    from services.visuals.service import VisualStudioService

    return VisualStudioService(ctx)


def _require_spec(params: dict[str, Any], key: str) -> dict[str, Any]:
    spec = params.get(key)
    if not isinstance(spec, dict):
        raise BridgeMethodError("CONFIG_INVALID", f"{key} must be an object")
    return spec


def handle_visual_presets(_params: dict[str, Any]) -> dict[str, Any]:
    """§134 — crosshair presets (spec đầy đủ theo preset) + default spec."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    return _visual_studio(ctx).presets()


def handle_visual_render_preview(params: dict[str, Any]) -> dict[str, Any]:
    """§132 — crosshair PNG 16x16 (data URI); UI throttle theo RAF."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    preview = _visual_studio(ctx).render_preview_b64(_require_spec(params, "spec"))
    return {"preview": preview}


def handle_visual_export_pack(params: dict[str, Any]) -> dict[str, Any]:
    """Crosshair spec → RS project → build ZIP → (tuỳ chọn) install instance."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    name = str(params.get("name", "")).strip()
    mc_version = str(params.get("mcVersion", "")).strip()
    if not name or not mc_version:
        raise BridgeMethodError("CONFIG_INVALID", "name and mcVersion are required")
    instance_id = str(params.get("installInstanceId", "")) or None
    if instance_id:
        _require_instance(ctx, instance_id)
    result = _visual_studio(ctx).export_pack(
        name, _require_spec(params, "spec"), mc_version,
        install_instance_id=instance_id,
        overwrite=bool(params.get("overwrite", False)))
    notify("visuals.changed", {"action": "export", "kind": "crosshair",
                               "projectId": result.get("projectId")})
    return {"export": result}


def handle_visual_totem_presets(_params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    return _visual_studio(ctx).totem_presets()


def handle_visual_render_totem(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    preview = _visual_studio(ctx).render_totem_b64(_require_spec(params, "spec"))
    return {"preview": preview}


def handle_visual_totem_model_get(_params: dict[str, Any]) -> dict[str, Any]:
    """Draft voxel model 3D (§129) — {spec: None} nếu chưa lưu."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    return _visual_studio(ctx).totem_model_get()


def handle_visual_totem_model_save(params: dict[str, Any]) -> dict[str, Any]:
    """Lưu draft — validate chặn ERROR, atomic write (mục 62)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    result = _visual_studio(ctx).totem_model_save(_require_spec(params, "spec"))
    notify("visuals.changed", {"action": "totem_model_save"})
    return result


def handle_visual_render_totem_model(params: dict[str, Any]) -> dict[str, Any]:
    """Static render model 3D (§130) — fallback khi không có WebGL."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    spec = _require_spec(params, "spec")
    size = int(params.get("size", 256))
    if not 64 <= size <= 512:
        raise BridgeMethodError("CONFIG_INVALID", "size must be 64..512")
    preview = _visual_studio(ctx).render_totem_model_b64(spec, size=size)
    return {"preview": preview}


def handle_visual_export_totem_pack(params: dict[str, Any]) -> dict[str, Any]:
    """Totem spec → RS project (+ model JSON version-aware) → build → install."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    name = str(params.get("name", "")).strip()
    mc_version = str(params.get("mcVersion", "")).strip()
    if not name or not mc_version:
        raise BridgeMethodError("CONFIG_INVALID", "name and mcVersion are required")
    instance_id = str(params.get("installInstanceId", "")) or None
    if instance_id:
        _require_instance(ctx, instance_id)
    result = _visual_studio(ctx).export_totem_pack(
        name, _require_spec(params, "spec"), mc_version,
        install_instance_id=instance_id,
        overwrite=bool(params.get("overwrite", False)))
    notify("visuals.changed", {"action": "export", "kind": "totem",
                               "projectId": result.get("projectId")})
    return {"export": result}


def handle_visual_hud_widgets(_params: dict[str, Any]) -> dict[str, Any]:
    """§135 — widget catalogue + layout mặc định."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    return _visual_studio(ctx).hud_widgets()


def handle_visual_save_hud_layout(params: dict[str, Any]) -> dict[str, Any]:
    """Lưu HUD layout vào visual project — validate widget id/dup/limits."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    layout = params.get("layout")
    if not isinstance(layout, list):
        raise BridgeMethodError("CONFIG_INVALID", "layout must be a list of widgets")
    result = _visual_studio(ctx).save_hud_layout(
        str(params.get("projectId", "")), layout)
    notify("visuals.changed", {"action": "hud_layout",
                               "projectId": str(params.get("projectId", ""))})
    return result


def handle_visual_fx_defaults(_params: dict[str, Any]) -> dict[str, Any]:
    """§52 — hit kinds + particle shapes + spec mặc định."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    return _visual_studio(ctx).fx_defaults()


def handle_visual_render_hit(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    preview = _visual_studio(ctx).render_hit_b64(_require_spec(params, "spec"))
    return {"preview": preview}


def handle_visual_render_particle(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    preview = _visual_studio(ctx).render_particle_b64(_require_spec(params, "spec"))
    return {"preview": preview}


def handle_visual_export_fx_pack(params: dict[str, Any]) -> dict[str, Any]:
    """Hit overlay + particle atlas → 1 RS project → build → install.

    Ít nhất 1 trong hitSpec/particleSpec phải có (service raise nếu cả hai None).
    """
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    name = str(params.get("name", "")).strip()
    mc_version = str(params.get("mcVersion", "")).strip()
    if not name or not mc_version:
        raise BridgeMethodError("CONFIG_INVALID", "name and mcVersion are required")
    hit_spec = params.get("hitSpec")
    particle_spec = params.get("particleSpec")
    if hit_spec is not None and not isinstance(hit_spec, dict):
        raise BridgeMethodError("CONFIG_INVALID", "hitSpec must be an object")
    if particle_spec is not None and not isinstance(particle_spec, dict):
        raise BridgeMethodError("CONFIG_INVALID", "particleSpec must be an object")
    if hit_spec is None and particle_spec is None:
        raise BridgeMethodError("CONFIG_INVALID",
                                "hitSpec or particleSpec is required")
    instance_id = str(params.get("installInstanceId", "")) or None
    if instance_id:
        _require_instance(ctx, instance_id)
    result = _visual_studio(ctx).export_fx_pack(
        name, hit_spec, particle_spec, mc_version,
        install_instance_id=instance_id,
        overwrite=bool(params.get("overwrite", False)))
    notify("visuals.changed", {"action": "export", "kind": "fx",
                               "projectId": result.get("projectId")})
    return {"export": result}


# ---------------------------------------------------------------------------
# B10 — game optimization (§3/§70 — plan-first + snapshot + rollback)
# ---------------------------------------------------------------------------


def _optimization_service(ctx: Any) -> Any:
    from services.optimization import GameOptimizationService

    return GameOptimizationService(ctx)


def handle_optimization_scan(params: dict[str, Any]) -> dict[str, Any]:
    """§3.1 — hardware + instance + recommendation + profile catalogue."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", "")) or None
    if instance_id:
        _require_instance(ctx, instance_id)
    return {"scan": _optimization_service(ctx).scan(instance_id)}


def handle_optimization_plan(params: dict[str, Any]) -> dict[str, Any]:
    """Preview diff — KHÔNG ghi gì (mục 3.1: preview trước apply, như profiles.plan)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", ""))
    profile_id = str(params.get("profileId", ""))
    if not instance_id or not profile_id:
        raise BridgeMethodError("CONFIG_INVALID", "instanceId and profileId are required")
    _require_instance(ctx, instance_id)
    return {"plan": _optimization_service(ctx).plan(instance_id, profile_id)}


def handle_optimization_apply(params: dict[str, Any]) -> dict[str, Any]:
    """Snapshot rồi apply profile — trả {snapshot, plan, profile} (mục 70)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", ""))
    profile_id = str(params.get("profileId", ""))
    if not instance_id or not profile_id:
        raise BridgeMethodError("CONFIG_INVALID", "instanceId and profileId are required")
    _require_instance(ctx, instance_id)
    result = _optimization_service(ctx).apply(instance_id, profile_id)
    notify("instances.changed", {"action": "optimize", "instanceId": instance_id,
                                 "profile": profile_id})
    return {"applied": result}


def handle_optimization_rollback(params: dict[str, Any]) -> dict[str, Any]:
    """Phục hồi từ snapshot gần nhất — 1 click (mục 70)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", ""))
    if not instance_id:
        raise BridgeMethodError("CONFIG_INVALID", "instanceId is required")
    _require_instance(ctx, instance_id)
    result = _optimization_service(ctx).rollback(instance_id)
    notify("instances.changed", {"action": "optimize-rollback", "instanceId": instance_id})
    return {"restored": result.get("restored", True)}


def handle_optimization_snapshot_info(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", ""))
    if not instance_id:
        raise BridgeMethodError("CONFIG_INVALID", "instanceId is required")
    snapshot = _optimization_service(ctx).snapshot_info(instance_id)
    return {"snapshot": snapshot}


# ---------------------------------------------------------------------------
# B10 — system optimization (§4/§35–§37 — cleanup/power, risk/reversible)
# ---------------------------------------------------------------------------


def _system_optimization(ctx: Any) -> Any:
    from services.system.service import SystemOptimizationService

    return SystemOptimizationService(ctx)


def handle_system_overview(_params: dict[str, Any]) -> dict[str, Any]:
    """§4.2 — hardware + power status + cleanup preview (một view hợp nhất)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    return {"overview": _system_optimization(ctx).overview()}


def handle_system_cleanup_scan(_params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    return {"preview": _system_optimization(ctx).cleanup_scan()}


def handle_system_cleanup_clean(params: dict[str, Any]) -> dict[str, Any]:
    """§37 — clean paths chọn; undo-able qua clean_id (cleaner tự manifest)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    paths = params.get("paths")
    if not isinstance(paths, list) or not paths or not all(isinstance(p, str) for p in paths):
        raise BridgeMethodError("CONFIG_INVALID", "paths must be a non-empty list of strings")
    result = _system_optimization(ctx).cleanup_clean([str(p) for p in paths])
    notify("system.changed", {"action": "clean", "bytes": result.get("bytes", 0)})
    return {"clean": result}


def handle_system_cleanup_undo(params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    clean_id = str(params.get("cleanId", ""))
    if not clean_id:
        raise BridgeMethodError("CONFIG_INVALID", "cleanId is required")
    result = _system_optimization(ctx).cleanup_undo(clean_id)
    notify("system.changed", {"action": "clean-undo"})
    return {"undo": result}


def handle_system_cleanup_empty_trash(_params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    removed = _system_optimization(ctx).cleanup_empty_trash()
    notify("system.changed", {"action": "empty-trash", "removed": removed})
    return {"removed": removed}


def handle_system_power_status(_params: dict[str, Any]) -> dict[str, Any]:
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    return {"power": _system_optimization(ctx).power_status()}


def handle_system_power_set_plan(params: dict[str, Any]) -> dict[str, Any]:
    """§36 — set power plan (balanced/high_performance/...); yêu cầu service thật."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    plan_id = str(params.get("planId", ""))
    if not plan_id:
        raise BridgeMethodError("CONFIG_INVALID", "planId is required")
    ok = _system_optimization(ctx).power_set_plan(plan_id)
    if not ok:
        raise BridgeMethodError("CONFIG_INVALID", f"unknown or failed plan: {plan_id}")
    notify("system.changed", {"action": "power-plan", "planId": plan_id})
    return {"set": True}


# ---------------------------------------------------------------------------
# B11 — network lab (§42 — TCP/Server List Ping + probe RTT/jitter + DNS)
# ---------------------------------------------------------------------------


def _require_host(params: dict[str, Any]) -> tuple[str, int]:
    host = str(params.get("host", "")).strip()
    port = int(params.get("port", 25565))
    if not host or not (0 < port < 65536):
        raise BridgeMethodError("CONFIG_INVALID", "valid host and port are required")
    return host, port


def handle_net_endpoints(_params: dict[str, Any]) -> dict[str, Any]:
    """§42 — TCP check song song các endpoint launcher cần (mojang/modrinth/…)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    from services.diagnostics.net import check_endpoints

    return {"endpoints": check_endpoints()}


def handle_net_tcp(params: dict[str, Any]) -> dict[str, Any]:
    """1 TCP connect + RTT ms cho host:port tuỳ ý."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    from services.diagnostics.net import tcp_check

    host, port = _require_host(params)
    timeout = min(float(params.get("timeout", 4.0)), 10.0)
    return {"tcp": tcp_check(host, port, timeout=timeout)}


def handle_net_ping(params: dict[str, Any]) -> dict[str, Any]:
    """§42 — Minecraft Server List Ping: MOTD/players/version/latency/favicon."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    from services.diagnostics.net import mc_ping

    host, port = _require_host(params)
    timeout = min(float(params.get("timeout", 4.0)), 10.0)
    return {"ping": mc_ping(host, port, timeout=timeout)}


def _probe_stats(samples: list[float]) -> dict[str, Any]:
    """RTT stats từ samples ms — min/avg/max/jitter (mục 42 RTT + jitter)."""
    ok = [s for s in samples if s is not None]
    if not ok:
        return {"min": None, "avg": None, "max": None, "jitter": None,
                "loss": 100.0}
    diffs = [abs(ok[i + 1] - ok[i]) for i in range(len(ok) - 1)]
    return {
        "min": round(min(ok), 1),
        "avg": round(sum(ok) / len(ok), 1),
        "max": round(max(ok), 1),
        # jitter = trung bình delta tuyệt đối giữa các mẫu liên tiếp (RFC-ish)
        "jitter": round(sum(diffs) / len(diffs), 1) if diffs else 0.0,
        "loss": round(100.0 * (len(samples) - len(ok)) / len(samples), 1),
    }


def handle_net_probe(params: dict[str, Any]) -> dict[str, Any]:
    """Network timeline (mục 42): N lần TCP connect → RTT/jitter/loss + timeline.

    Sync — N nhỏ (mặc định 10, cap 30) và mỗi sample có timeout riêng; UI
    gọi khi bấm nút, không poll liên tục (§171).
    """
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    from services.diagnostics.net import tcp_check

    host, port = _require_host(params)
    count = max(1, min(int(params.get("count", 10)), 30))
    timeout = min(float(params.get("timeout", 2.0)), 5.0)

    timeline: list[dict[str, Any]] = []
    for i in range(count):
        started = time.perf_counter()
        result = tcp_check(host, port, timeout=timeout)
        timeline.append({
            "seq": i,
            "ok": result.get("ok", False),
            "ms": result.get("ms"),
            "error": result.get("error"),
            "at": round(time.perf_counter() - started, 4),
        })
    # None cho mẫu fail — _probe_stats cần tổng số mẫu để tính loss
    samples = [t["ms"] if t["ok"] else None for t in timeline]
    ok_count = sum(1 for t in timeline if t["ok"])
    return {
        "probe": {
            "host": host,
            "port": port,
            "count": count,
            "ok": ok_count,
            "stats": _probe_stats(samples),
            "timeline": timeline,
        }
    }


def handle_net_dns(params: dict[str, Any]) -> dict[str, Any]:
    """DNS resolve qua socket.getaddrinfo — trả IPv4/IPv6 + TTL-ish (hop thời gian)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    import socket

    host = str(params.get("host", "")).strip()
    if not host:
        raise BridgeMethodError("CONFIG_INVALID", "host is required")
    started = time.perf_counter()
    try:
        infos = socket.getaddrinfo(host, None)
    except OSError as err:
        return {"dns": {"host": host, "ok": False,
                        "addresses": [], "error": str(err)[:160]}}
    addresses: list[str] = []
    for info in infos:
        addr = info[4][0]
        if addr not in addresses:
            addresses.append(addr)
    return {
        "dns": {
            "host": host,
            "ok": True,
            "addresses": addresses,
            "ms": round((time.perf_counter() - started) * 1000, 1),
        }
    }


# ---------------------------------------------------------------------------
# B12 — runtime companion + packet inspector (§12/§13/§121/§122)
# ---------------------------------------------------------------------------

#: §122 — ring buffer: 10k default, 100k developer. Memory only (mục 62).
PACKET_RING_DEFAULT = 10_000
PACKET_RING_DEV = 100_000
_PACKET_RING_CAP = PACKET_RING_DEFAULT


class _PacketRing:
    """Ring buffer metadata packet (§122) — oldest-first eviction, không ghi disk.

    Entries: {timestamp, direction, packetId, name, size, payload?}.
    Payload chỉ giữ khi capture bật (§121 DEBUG PAYLOAD — manual, bounded).
    """

    def __init__(self, cap: int = PACKET_RING_DEFAULT) -> None:
        self._entries: list[dict[str, Any]] = []
        self._lock = threading.Lock()
        self._cap = cap
        self._next_id = 1
        self.capture_payload = False
        self.dropped = 0

    @property
    def cap(self) -> int:
        return self._cap

    def set_cap(self, cap: int) -> None:
        with self._lock:
            self._cap = max(100, min(int(cap), PACKET_RING_DEV))
            overflow = len(self._entries) - self._cap
            if overflow > 0:
                del self._entries[:overflow]
                self.dropped += overflow

    def append(self, name: str, size: int, direction: str,
               payload: dict[str, Any] | None = None) -> int:
        with self._lock:
            packet_id = self._next_id
            self._next_id += 1
            entry: dict[str, Any] = {
                "timestamp": time.time(),
                "direction": direction if direction in ("in", "out") else "in",
                "packetId": packet_id,
                "name": str(name)[:64],
                "size": int(size),
            }
            if self.capture_payload and isinstance(payload, dict):
                entry["payload"] = payload
            self._entries.append(entry)
            if len(self._entries) > self._cap:
                del self._entries[:len(self._entries) - self._cap]
                self.dropped += 1
            return packet_id

    def list(self, limit: int = 200) -> list[dict[str, Any]]:
        with self._lock:
            return list(self._entries[-max(1, min(int(limit), self._cap)):])

    def stats(self) -> dict[str, Any]:
        with self._lock:
            total_bytes = sum(e["size"] for e in self._entries)
            return {
                "count": len(self._entries),
                "cap": self._cap,
                "dropped": self.dropped,
                "bytes": total_bytes,
                "capturePayload": self.capture_payload,
            }

    def clear(self) -> None:
        with self._lock:
            self._entries.clear()


_PACKET_RING: _PacketRing | None = None


def _packet_ring() -> _PacketRing:
    global _PACKET_RING
    if _PACKET_RING is None:
        _PACKET_RING = _PacketRing()
    return _PACKET_RING


def _runtime_service(ctx: Any) -> Any:
    from services.runtime import RuntimeService

    return ctx.get("runtime") if hasattr(ctx, "get") else RuntimeService(ctx)


def handle_runtime_endpoint(_params: dict[str, Any]) -> dict[str, Any]:
    """§12 — IPC endpoint + token + packetTypes cho companion setup."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    return {"endpoint": _runtime_service(ctx).endpoint()}


def handle_runtime_start(_params: dict[str, Any]) -> dict[str, Any]:
    """Start IPC server (token xoay mỗi lần start — server.py)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    return {"started": _runtime_service(ctx).start()}


def handle_runtime_sessions(_params: dict[str, Any]) -> dict[str, Any]:
    """§12 — companion sessions đang kết nối (mirror api.py runtime_sessions)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    return {"sessions": _runtime_service(ctx).sessions()}


def handle_runtime_metrics(params: dict[str, Any]) -> dict[str, Any]:
    """§12 — FPS/frameMs history ring RAM (mục 62 memory-only)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    session_id = str(params.get("sessionId", "")) or None
    return {"metrics": _runtime_service(ctx).metrics(session_id)}


def handle_runtime_pairing(params: dict[str, Any]) -> dict[str, Any]:
    """Auto-pairing: ghi companion.json vào game dir của instance (mục 12)."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", ""))
    _require_instance(ctx, instance_id)
    result = _runtime_service(ctx).write_pairing_for_instance(instance_id)
    return {"pairing": result}


def handle_packet_ingest(params: dict[str, Any]) -> dict[str, Any]:
    """Nhận 1 packet từ companion qua bridge (packetTypes valid — schema v1).

    Đẩy qua RuntimeService._on_packet (events + metrics ring RAM) VÀ ghi metadata
    vào ring buffer inspector (§121). Return packetId của inspector entry.
    """
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    packet = params.get("packet")
    if not isinstance(packet, dict):
        raise BridgeMethodError("CONFIG_INVALID", "packet must be an object")
    if packet.get("version") != 1:
        raise BridgeMethodError("CONFIG_INVALID", "packet version must be 1")
    ptype = packet.get("type")
    if not isinstance(ptype, str) or not ptype:
        raise BridgeMethodError("CONFIG_INVALID", "packet.type is required")
    if not isinstance(packet.get("payload"), dict):
        raise BridgeMethodError("CONFIG_INVALID", "packet.payload must be an object")

    import json as _json

    size = len(_json.dumps(packet, separators=(",", ":")).encode("utf-8"))
    packet_id = _packet_ring().append(ptype, size,
                                      str(params.get("direction", "in")),
                                      packet.get("payload"))
    try:
        _runtime_service(ctx)._on_packet(packet, "bridge")
    except Exception:  # noqa: BLE001 — ingest phải luôn trả packetId
        pass
    return {"packetId": packet_id}


def handle_packet_list(params: dict[str, Any]) -> dict[str, Any]:
    """§121 — packet metadata list (mới nhất cuối) + stats ring."""
    limit = int(params.get("limit", 200))
    ring = _packet_ring()
    return {"packets": ring.list(limit=limit), "stats": ring.stats()}


def handle_packet_stats(_params: dict[str, Any]) -> dict[str, Any]:
    ring = _packet_ring()
    return {"stats": ring.stats()}


def handle_packet_capture(params: dict[str, Any]) -> dict[str, Any]:
    """§121 — DEBUG PAYLOAD mode: manual on/off (memory bounded — payload lưu)."""
    enabled = bool(params.get("enabled", False))
    ring = _packet_ring()
    ring.capture_payload = enabled
    if params.get("cap") is not None:
        ring.set_cap(int(params["cap"]))
    return {"capture": ring.capture_payload, "stats": ring.stats()}


def handle_packet_clear(_params: dict[str, Any]) -> dict[str, Any]:
    ring = _packet_ring()
    ring.clear()
    return {"cleared": True, "stats": ring.stats()}


def handle_packet_export(_params: dict[str, Any]) -> dict[str, Any]:
    """§122 — export CHỈ khi user yêu cầu (không ghi disk liên tục)."""
    ring = _packet_ring()
    return {"export": {"packets": ring.list(limit=ring.cap), "stats": ring.stats()}}


# ---------------------------------------------------------------------------
# B13 — diagnostics: log center + repair + evidence export (§41/§39/§16)
# ---------------------------------------------------------------------------


def _log_analyzer(ctx: Any) -> Any:
    from services.diagnostics.log_analyzer import LogAnalyzer

    return LogAnalyzer(ctx)


def _repair_service(ctx: Any) -> Any:
    from services.repair.service import RepairService

    return RepairService(ctx)


def handle_console_sources(params: dict[str, Any]) -> dict[str, Any]:
    """§41 — nguồn log (launcher/minecraft/crash) + trạng thái + bytes."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", "")) or None
    return {"sources": _log_analyzer(ctx).sources(instance_id)}


def handle_console_read(params: dict[str, Any]) -> dict[str, Any]:
    """§16 — đọc 1 nguồn → [{n, text}] cho console ảo hoá; limit cap 20000."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    source = str(params.get("source", ""))
    if not source:
        raise BridgeMethodError("CONFIG_INVALID", "source is required")
    limit = max(1, min(int(params.get("limit", 5000)), 20000))
    instance_id = str(params.get("instanceId", "")) or None
    return {"log": _log_analyzer(ctx).read(source, instance_id, limit=limit)}


def handle_console_analyze(params: dict[str, Any]) -> dict[str, Any]:
    """§41 — blueprint insights (không LLM — mục 24): id/count/severity/lines."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", "")) or None
    source = str(params.get("source", "")) or None
    return {"analysis": _log_analyzer(ctx).analyze(instance_id, source)}


def handle_console_insights(params: dict[str, Any]) -> dict[str, Any]:
    """Chip insights gọn: insights + errorCount/warnCount."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", "")) or None
    analysis = _log_analyzer(ctx).analyze(instance_id)
    return {
        "insights": analysis["insights"],
        "errorCount": analysis["errorCount"],
        "warnCount": analysis["warnCount"],
    }


def handle_repair_actions(_params: dict[str, Any]) -> dict[str, Any]:
    """§39 — danh sách repair action khả dụng."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    from services.repair.service import ACTIONS

    return {"actions": list(ACTIONS)}


def handle_repair_scan(params: dict[str, Any]) -> dict[str, Any]:
    """§39 — dry-run "What will change?": findings + planned, KHÔNG ghi."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    action = str(params.get("action", ""))
    if not action:
        raise BridgeMethodError("CONFIG_INVALID", "action is required")
    instance_id = str(params.get("instanceId", "")) or None
    if instance_id:
        _require_instance(ctx, instance_id)
    return {"scan": _repair_service(ctx).scan(action, instance_id)}


def handle_repair_run(params: dict[str, Any]) -> dict[str, Any]:
    """§39 — thực thi fix đã planned; trả summary những gì đã đổi."""
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    action = str(params.get("action", ""))
    if not action:
        raise BridgeMethodError("CONFIG_INVALID", "action is required")
    instance_id = str(params.get("instanceId", "")) or None
    if instance_id:
        _require_instance(ctx, instance_id)
    result = _repair_service(ctx).run(action, instance_id)
    notify("diagnostics.changed", {"action": "repair-run", "repair": action})
    return {"result": result}


def handle_diagnostic_export(params: dict[str, Any]) -> dict[str, Any]:
    """Evidence export (mục 41) — gói JSON on-request: insights + sources +
    repair scan cho từng action + app version. Không chứa token/secret
    (chỉ data diagnostics thuần).
    """
    ctx = _legacy_ctx()
    if ctx is None:
        raise _legacy_unavailable()
    instance_id = str(params.get("instanceId", "")) or None
    analyzer = _log_analyzer(ctx)
    analysis = analyzer.analyze(instance_id)
    sources = analyzer.sources(instance_id)
    repair_scans: list[dict[str, Any]] = []
    try:
        repair = _repair_service(ctx)
        for action in ("instance_metadata", "missing_dirs", "option_files"):
            repair_scans.append(repair.scan(action, instance_id))
    except Exception:  # noqa: BLE001 — export phải luôn trả gói
        pass
    return {
        "export": {
            "generatedAt": time.time(),
            "appVersion": SERVICE_VERSION,
            "instanceId": instance_id,
            "sources": sources,
            "analysis": {
                "insights": analysis["insights"],
                "errorCount": analysis["errorCount"],
                "warnCount": analysis["warnCount"],
            },
            "repairScans": repair_scans,
        }
    }


# ---------------------------------------------------------------------------
# Registry + dispatch
# ---------------------------------------------------------------------------

HANDLERS: dict[str, Callable[[dict[str, Any]], Any]] = {
    "health.ping": handle_health_ping,
    "health.version": handle_health_version,
    "health.shutdown": handle_health_shutdown,
    "app.echo": handle_app_echo,
    "app.storage_root": handle_app_storage_root,
    # M6
    "instances.list": handle_instances_list,
    "instances.get": handle_instances_get,
    "instances.create": handle_instances_create,
    "instances.select": handle_instances_select,
    "accounts.list": handle_accounts_list,
    "accounts.select": handle_accounts_select,
    "java.list": handle_java_list,
    "versions.list": handle_versions_list,
    "play.preflight": handle_play_preflight,
    "play.launch": handle_play_launch,
    "dashboard.summary": handle_dashboard_summary,
    # B6/M7 — profiles
    "profiles.list": handle_profiles_list,
    "profiles.get": handle_profiles_get,
    "profiles.create": handle_profiles_create,
    "profiles.duplicate": handle_profiles_duplicate,
    "profiles.update": handle_profiles_update,
    "profiles.delete": handle_profiles_delete,
    "profiles.capture": handle_profiles_capture,
    "profiles.plan": handle_profiles_plan,
    "profiles.apply": handle_profiles_apply,
    "profiles.revert": handle_profiles_revert,
    "profiles.export": handle_profiles_export,
    "profiles.import": handle_profiles_import,
    "profiles.validate": handle_profiles_validate,
    # B7 — mods
    "mods.list": handle_mods_list,
    "mods.search": handle_mods_search,
    "mods.install": handle_mods_install,
    "mods.remove": handle_mods_remove,
    "mods.health": handle_mods_health,
    "mods.scan": handle_mods_scan,
    "mods.autofix": handle_mods_autofix,
    "mods.quarantine.list": handle_mods_quarantine_list,
    "mods.quarantine.restore": handle_mods_quarantine_restore,
    "mods.quarantine.delete": handle_mods_quarantine_delete,
    # B7b — modpack + mod detail
    "modpack.info": handle_modpack_info,
    "modpack.install": handle_modpack_install,
    "modpack.status": handle_modpack_status,
    "modpack.cancel": handle_modpack_cancel,
    "mod.detail": handle_mod_detail,
    # B8a — asset library + resource projects
    "asset.list": handle_asset_list,
    "asset.import": handle_asset_import,
    "asset.get": handle_asset_get,
    "asset.delete": handle_asset_delete,
    "asset.assign": handle_asset_assign,
    "resource.list": handle_resource_list,
    "resource.get": handle_resource_get,
    # B8b — resource pack lifecycle
    "resource.wizard_info": handle_resource_wizard_info,
    "resource.create": handle_resource_create,
    "resource.update": handle_resource_update,
    "resource.delete": handle_resource_delete,
    "resource.generate": handle_resource_generate,
    "resource.validate": handle_resource_validate,
    "resource.build": handle_resource_build,
    "resource.build_task": handle_resource_build_task,
    "resource.build_cancel": handle_resource_build_cancel,
    "resource.builds": handle_resource_builds,
    "resource.install": handle_resource_install,
    "resource.installed": handle_resource_installed,
    "resource.uninstall": handle_resource_uninstall,
    "resource.layer.get": handle_resource_layer_get,
    "resource.layer.set": handle_resource_layer_set,
    "resource.layer.move": handle_resource_layer_move,
    # B9 — visual studio
    "visual.presets": handle_visual_presets,
    "visual.render_preview": handle_visual_render_preview,
    "visual.export_pack": handle_visual_export_pack,
    "visual.totem_presets": handle_visual_totem_presets,
    "visual.render_totem": handle_visual_render_totem,
    "visual.totem_model.get": handle_visual_totem_model_get,
    "visual.totem_model.save": handle_visual_totem_model_save,
    "visual.render_totem_model": handle_visual_render_totem_model,
    "visual.export_totem_pack": handle_visual_export_totem_pack,
    "visual.hud_widgets": handle_visual_hud_widgets,
    "visual.save_hud_layout": handle_visual_save_hud_layout,
    "visual.fx_defaults": handle_visual_fx_defaults,
    "visual.render_hit": handle_visual_render_hit,
    "visual.render_particle": handle_visual_render_particle,
    "visual.export_fx_pack": handle_visual_export_fx_pack,
    # B10 — game optimization
    "optimization.scan": handle_optimization_scan,
    "optimization.plan": handle_optimization_plan,
    "optimization.apply": handle_optimization_apply,
    "optimization.rollback": handle_optimization_rollback,
    "optimization.snapshot_info": handle_optimization_snapshot_info,
    # B10 — system optimization
    "system.overview": handle_system_overview,
    "system.cleanup.scan": handle_system_cleanup_scan,
    "system.cleanup.clean": handle_system_cleanup_clean,
    "system.cleanup.undo": handle_system_cleanup_undo,
    "system.cleanup.empty_trash": handle_system_cleanup_empty_trash,
    "system.power.status": handle_system_power_status,
    "system.power.set_plan": handle_system_power_set_plan,
    # B11 — network lab
    "net.endpoints": handle_net_endpoints,
    "net.tcp": handle_net_tcp,
    "net.ping": handle_net_ping,
    "net.probe": handle_net_probe,
    "net.dns": handle_net_dns,
    # B12 — runtime companion + packet inspector
    "runtime.endpoint": handle_runtime_endpoint,
    "runtime.start": handle_runtime_start,
    "runtime.sessions": handle_runtime_sessions,
    "runtime.metrics": handle_runtime_metrics,
    "runtime.pairing": handle_runtime_pairing,
    "packet.ingest": handle_packet_ingest,
    "packet.list": handle_packet_list,
    "packet.stats": handle_packet_stats,
    "packet.capture": handle_packet_capture,
    "packet.clear": handle_packet_clear,
    "packet.export": handle_packet_export,
    # B13 — diagnostics
    "console.sources": handle_console_sources,
    "console.read": handle_console_read,
    "console.analyze": handle_console_analyze,
    "console.insights": handle_console_insights,
    "repair.actions": handle_repair_actions,
    "repair.scan": handle_repair_scan,
    "repair.run": handle_repair_run,
    "diagnostic.export": handle_diagnostic_export,
}

# Methods bắt buộc phải có để handshake pass (§234)
REQUIRED_HANDLERS = ("health.ping", "health.version", "health.shutdown")


def assert_contract() -> None:
    missing = [m for m in REQUIRED_HANDLERS if m not in HANDLERS]
    if missing:
        raise RuntimeError(f"sidecar contract missing handlers: {missing}")


def handle_request(request: dict[str, Any]) -> dict[str, Any]:
    request_id = request.get("id")
    method = request.get("method", "")
    params = request.get("params") or {}

    handler = HANDLERS.get(method)
    if handler is None:
        return {
            "id": request_id,
            "ok": False,
            "error": {"code": "METHOD_NOT_FOUND", "message": method},
        }

    try:
        data = handler(params if isinstance(params, dict) else {})
        return {"id": request_id, "ok": True, "data": data}
    except BridgeMethodError as err:
        return {
            "id": request_id,
            "ok": False,
            "error": {"code": err.code, "message": str(err)},
        }
    except Exception as err:  # noqa: BLE001 — không bao giờ chết giữa dòng
        # AntaresError (legacy service) → giữ code taxonomy §117, không nuốt vào LEGACY_INTERNAL.
        to_dict = getattr(err, "to_dict", None)
        if callable(to_dict) and isinstance(getattr(err, "code", None), str):
            try:
                detail = to_dict()
                if isinstance(detail, dict) and isinstance(detail.get("code"), str):
                    return {
                        "id": request_id,
                        "ok": False,
                        "error": {
                            "code": detail["code"],
                            "message": str(detail.get("message") or err),
                        },
                    }
            except Exception:  # noqa: BLE001
                pass
        return {
            "id": request_id,
            "ok": False,
            "error": {"code": "LEGACY_INTERNAL", "message": f"{type(err).__name__}: {err}"},
        }


def main() -> int:
    assert_contract()
    notify("sidecar.started", {"service": SERVICE_NAME, "protocol": PROTOCOL_VERSION})
    try:
        for raw_line in sys.stdin:
            if len(raw_line.encode("utf-8", errors="replace")) > MAX_LINE_BYTES:
                continue  # oversized — reject line, giữ sống
            raw_line = raw_line.strip()
            if not raw_line:
                continue
            try:
                request = json.loads(raw_line)
            except json.JSONDecodeError:
                # §31: invalid → reject an toàn, không crash
                continue
            if not isinstance(request, dict) or "id" not in request:
                continue
            write_message(handle_request(request))
    except (BrokenPipeError, KeyboardInterrupt):
        pass
    notify("sidecar.stopped", {"service": SERVICE_NAME})
    return 0


if __name__ == "__main__":
    sys.exit(main())
