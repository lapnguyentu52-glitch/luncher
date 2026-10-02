"""AntaresApi — PyWebView js_api bridge (mục 335-339).

Quy tắc: frontend chỉ gọi method trên class này; mọi trả về là dict
serialize được; mọi lỗi là {ok: False, error: {code,...}}.
"""
from __future__ import annotations

import traceback

from pathlib import Path

from api.events.bridge import EventBridge
from app.context import AppContext
from core.errors import codes
from core.errors.base import AntaresError
from core.events import names as ev
from app.version import APP_VERSION
from core.logging.setup import get_logger

# codes.ZIP_TOO_LARGE được định nghĩa runtime-optional — module importer dùng
# ZIP_INVALID khi thiếu. Đảm bảo attr tồn tại:
if not hasattr(codes, "ZIP_TOO_LARGE"):
    codes.ZIP_TOO_LARGE = "ZIP_TOO_LARGE"

logger = get_logger("api")

_SETTINGS_SECTIONS = ("ui", "performance", "network", "servers", "java", "app")

NOTIFICATION_SEVERITIES = ("INFO", "SUCCESS", "WARNING", "ERROR", "CRITICAL")


class AntaresApi:
    """Expose commands cho frontend qua pywebview js_api."""

    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx
        # Kênh đẩy event xuống UI (thay cho poll định kỳ — mục 8.2, 15.2).
        # Hoạt động cả khi chưa có cửa sổ: event chờ sẵn cho events_drain().
        self.events = EventBridge(ctx.events)
        ctx.set("event_bridge", self.events)
        self._wire_services()

    # ---------- events ----------
    def attach_window(self, window) -> None:
        """Gọi từ main.py sau khi tạo cửa sổ để bật thread đẩy event."""
        self.events.attach_window(window)

    def events_drain(self) -> dict:
        """Lấy event đang chờ — đường dự phòng khi kênh đẩy không khả dụng."""
        try:
            return self._ok({
                "events": self.events.flush(),
                "pushing": self.events.can_push(),
            })
        except Exception as e:
            return self._err(e)

    def events_sync_log_cursor(self, cursor: int) -> dict:
        """UI nạp snapshot log xong -> báo bridge bỏ qua các dòng ≤ cursor."""
        try:
            self.events.sync_log_cursor(int(cursor))
            return self._ok({"synced": True})
        except Exception as e:
            return self._err(e)

    def events_stats(self) -> dict:
        try:
            return self._ok({
                "stats": dict(self.events.stats),
                "pending": self.events.pending(),
                "pushing": self.events.can_push(),
            })
        except Exception as e:
            return self._err(e)

    # ---------- plugins (mục 84, 85) ----------
    def plugins_list(self) -> dict:
        """Danh sách plugin tìm thấy + trạng thái + quyền."""
        try:
            loader = self._ctx.get("plugins")
            if loader is None:
                return self._err(FileNotFoundError("PluginLoader chưa khởi tạo"))
            return self._ok({"plugins": loader.scan()})
        except Exception as e:
            return self._err(e)

    def plugins_set_enabled(self, plugin_id: str, enabled: bool) -> dict:
        """Bật/tắt plugin (mục 76: reversible — disable không xoá code)."""
        try:
            loader = self._ctx.get("plugins")
            if loader is None:
                return self._err(FileNotFoundError("PluginLoader chưa khởi tạo"))
            r = loader.set_enabled(str(plugin_id), bool(enabled))
            if not r.get("ok", True):
                return {"ok": False, "error": r.get("error", {"code": "ERROR", "message": "lỗi không rõ"})}
            # PLUGIN_LOADED/UNLOADED đã do loader publish khi load/unload
            return self._ok({"id": plugin_id, "enabled": bool(enabled)})
        except Exception as e:
            return self._err(e)

    def plugins_registry(self) -> dict:
        """Những gì các plugin đã đăng ký: tabs/panels/commands/... (mục 84)."""
        try:
            loader = self._ctx.get("plugins")
            if loader is None:
                return self._err(FileNotFoundError("PluginLoader chưa khởi tạo"))
            reg = loader.registry()
            return self._ok({
                "tabs": reg.tabs(),
                "panels": {host: reg.panels_for(host) for host in ("dashboard", "play", "performance")},
                "commands": [c["id"] for c in reg.commands()],
            })
        except Exception as e:
            return self._err(e)

    def plugins_run_command(self, command_id: str) -> dict:
        """Chạy command do plugin đăng ký (command palette)."""
        try:
            loader = self._ctx.get("plugins")
            if loader is None:
                return self._err(FileNotFoundError("PluginLoader chưa khởi tạo"))
            r = loader.registry().run_command(str(command_id))
            if not r.get("ok"):
                return {"ok": False, "error": r.get("error", {"code": "ERROR", "message": "lỗi không rõ"})}
            return self._ok(r.get("data"))
        except Exception as e:
            return self._err(e)

    def _publish(self, name: str, payload: dict | None = None, **kw) -> None:
        """Báo cho UI biết state vừa đổi (không làm hỏng command nếu lỗi)."""
        try:
            self._ctx.events.publish(name, payload or {}, **kw)
        except Exception as err:  # noqa: BLE001 — publish không được phá command
            logger.warning("event publish failed (%s): %s", name, err)

    def _wire_services(self) -> None:
        ctx = self._ctx
        from infrastructure.http.client import HttpClient
        from infrastructure.process.manager import ProcessManager
        from infrastructure.cache.manifests import DiskCache
        from services.downloads.manager import DownloadManager
        from services.accounts.service import AccountService
        from services.accounts.secure_store import SecureStore
        from services.accounts.ely import ElyAuthProvider
        from services.instances.service import InstanceService
        from services.java.manager import JavaManager
        from services.java.discovery import scan_system_java
        from services.mods.service import ModService
        from services.servers.service import ServerService
        from services.notifications import NotificationService
        from core.scheduler.policy import GameModeGovernor
        from services.performance import SystemTelemetryService
        from services.optimization import GameOptimizationService
        from services.profiles import ProfileService
        from services.system import SystemOptimizationService
        from services.resources import ResourceStudioService
        from services.visuals import VisualStudioService
        from services.backups import BackupService
        from services.repair import RepairService
        from services.runtime import RuntimeService
        from services.plugins.loader import PluginLoader

        http = HttpClient()
        processes = ProcessManager()
        dm = DownloadManager(http, ctx.tasks, ctx.paths.cache / "downloads")

        secure = SecureStore(ctx.paths.accounts / "secure")
        ely = ElyAuthProvider()
        from services.mods.security import ModSecurityService
        security = ModSecurityService(ctx)

        ctx.set("http_client", http)
        ctx.set("process_manager", processes)
        ctx.set("download_manager", dm)
        ctx.set("secure_store", secure)
        ctx.set("ely_provider", ely)
        ctx.set("accounts", AccountService(ctx))
        ctx.set("instances", InstanceService(ctx))
        ctx.set("java", JavaManager())
        ctx.set("mods", ModService(ctx, http))
        ctx.set("servers", ServerService(ctx, processes))
        ctx.set("mod_security", security)
        # Notification Center — lắng nghe EventBus tự sinh notification (mục 6)
        ctx.set("notifications", NotificationService(ctx.events))
        # System Optimization Center — cleanup/power/process (mục 4, 35-37)
        system_opt = SystemOptimizationService(ctx)
        ctx.set("system_opt", system_opt)
        # Resource Pack Studio — project/build/validate/install (mục 10, 33)
        ctx.set("resource_studio", ResourceStudioService(ctx))
        # Asset Library — import PNG/hash/catalog (RS v2 Batch 5, mục 12)
        from services.resources.assets import AssetStore
        ctx.set("assets", AssetStore(ctx))
        # Visual Studio — crosshair/totem/hud (mục 8, 9, 11)
        ctx.set("visual_studio", VisualStudioService(ctx))
        # Backup & Restore Center — snapshot/restore/export (mục 22)
        ctx.set("backups", BackupService(ctx))
        # Repair Center — dry-run scan + run (mục 39)
        ctx.set("repair", RepairService(ctx))
        # Runtime Companion — IPC loopback nhận telemetry từ game (mục 12, 13)
        runtime = RuntimeService(ctx)
        ctx.set("runtime", runtime)
        # Plugin SDK — load plugin user (mục 84, 85); safe-mode bỏ qua plugin
        plugins = PluginLoader(ctx)
        ctx.set("plugins", plugins)
        if not ctx.dev_mode:
            plugins.load_all()

        # Performance Center — governor + telemetry sampler (mục 5, 30, 89)
        # Priority hook (mục 36): chỉ chạy nếu user bật gamePriority != normal (mục 74)
        def apply_game_priority(instance_id: str) -> None:
            level = str(ctx.config.get("performance.gamePriority") or "normal")
            if level and level != "normal":
                system_opt.process_apply_priority(f"minecraft:{instance_id}", level)

        governor = GameModeGovernor(ctx.events, process_priority_hook=apply_game_priority)
        telemetry = SystemTelemetryService(governor)
        telemetry.start()
        ctx.set("governor", governor)
        ctx.set("telemetry", telemetry)
        # Game Optimization Center — plan/apply/rollback instance-local (mục 3, 70)
        ctx.set("optimization", GameOptimizationService(ctx))
        # Player Profiles — 1 preset = account + instance + JVM + game + launch (mục 40)
        ctx.set("profiles", ProfileService(ctx))
        # Console & Net diagnostics — phân tích log + TCP/Server List Ping (mục 41, 42)
        from services.diagnostics.log_analyzer import LogAnalyzer
        ctx.set("log_analyzer", LogAnalyzer(ctx))
        # Skin & Cape Studio — import/generate/apply skin, cape (mục 43)
        from services.skins import SkinCapeService
        ctx.set("skins", SkinCapeService(ctx))

        # TaskManager -> event: UI theo dõi tiến trình bằng event thay vì
        # poll tasks_list mỗi vài trăm ms (mục 15.2, 209).
        def on_task_update(task) -> None:
            try:
                ctx.events.publish(ev.TASK_UPDATED, task.to_dict(), task_id=task.id)
            except Exception as err:  # noqa: BLE001 — listener không được ném ra TaskManager
                logger.warning("task update event failed: %s", err)
        ctx.tasks.set_update_listener(on_task_update)

        # Java discovery nâng cao chạy nền — không block startup (mục 39)
        def warm_java():
            try:
                ctx.set("java_homes", scan_system_java())
            except Exception as err:  # noqa: BLE001 — scan nền fail không được giết startup
                logger.warning("warm java discovery failed: %s", err)
        import threading
        threading.Thread(target=warm_java, daemon=True).start()

    # ---------- helper ----------
    def _ok(self, data=None) -> dict:
        return {"ok": True, "data": data or {}}

    def _err(self, exc: Exception) -> dict:
        from core.errors.base import AntaresError
        if isinstance(exc, AntaresError):
            logger.warning("Command error: %s %s", exc.code, exc.message)
            return {"ok": False, "error": exc.to_dict()}
        logger.exception("Unhandled API error")
        return {"ok": False, "error": {
            "code": "INTERNAL_ERROR", "message": str(exc), "recoverable": False}}

    # ---------- app ----------
    def app_get_state(self) -> dict:
        try:
            accounts = self._ctx.get("accounts")
            return self._ok({
                "app": {"name": "Antares Launcher", "version": APP_VERSION},
                "selectedInstance": self._ctx.config.get("selectedInstance"),
                "accountsSummary": [
                    {"id": a["id"], "displayName": a["displayName"], "type": a["type"]}
                    for a in (accounts.list() if accounts else [])
                ],
                "instancesSummary": [
                    {"id": i["id"], "name": i["name"],
                     "loader": i.get("loader"), "version": i.get("minecraftVersion")}
                    for i in (self._ctx.get("instances").list() or [])
                ],
            })
        except Exception as e:
            return self._err(e)

    # ---------- accounts ----------
    def accounts_list(self) -> dict:
        try:
            accounts = self._ctx.get("accounts")
            return self._ok({"accounts": [
                {k: v for k, v in a.items() if k != "token"}
                for a in accounts.list()
            ]})
        except Exception as e:
            return self._err(e)

    def accounts_add_offline(self, display_name: str) -> dict:
        try:
            acc = self._ctx.get("accounts").create_offline(display_name)
            self._publish(ev.ACCOUNTS_CHANGED, {"action": "add", "accountId": acc.get("id")})
            return self._ok({"account": {k: v for k, v in acc.items() if k != "token"}})
        except Exception as e:
            return self._err(e)

    def accounts_login_ely(self, username: str, password: str) -> dict:
        try:
            acc = self._ctx.get("accounts").login_ely(username, password)
            self._publish(ev.AUTH_SUCCESS, {"accountId": acc.get("id"), "type": acc.get("type")})
            self._publish(ev.ACCOUNTS_CHANGED, {"action": "login", "accountId": acc.get("id")})
            return self._ok({"account": {k: v for k, v in acc.items() if k != "token"}})
        except Exception as e:
            return self._err(e)

    def accounts_remove(self, account_id: str) -> dict:
        try:
            removed = self._ctx.get("accounts").remove(account_id)
            self._publish(ev.ACCOUNTS_CHANGED, {"action": "remove", "accountId": account_id})
            return self._ok({"removed": removed})
        except Exception as e:
            return self._err(e)

    def accounts_select(self, account_id: str) -> dict:
        try:
            selected = self._ctx.get("accounts").select(account_id)
            self._publish(ev.SELECTION_CHANGED, {"accountId": account_id})
            return self._ok({"selected": selected})
        except Exception as e:
            return self._err(e)

    # ---------- instances ----------
    def instances_list(self) -> dict:
        try:
            return self._ok({"instances": self._ctx.get("instances").list()})
        except Exception as e:
            return self._err(e)

    def instances_create(self, name: str, minecraft_version: str,
                         loader: str = "vanilla", memory_max_mb: int = 2048) -> dict:
        try:
            inst = self._ctx.get("instances").create(
                name, minecraft_version, loader=loader, memory_max_mb=memory_max_mb)
            self._ctx.config.set("selectedInstance", inst["id"], flush_now=True)
            self._publish(ev.INSTANCE_CREATED, {"instanceId": inst["id"], "name": inst.get("name")})
            self._publish(ev.INSTANCES_CHANGED, {"action": "create", "instanceId": inst["id"]})
            return self._ok({"instance": inst})
        except Exception as e:
            return self._err(e)

    def instances_delete(self, instance_id: str, confirm: bool = False) -> dict:
        try:
            deleted = self._ctx.get("instances").delete(instance_id, confirm=confirm)
            self._publish(ev.INSTANCE_DELETED, {"instanceId": instance_id})
            self._publish(ev.INSTANCES_CHANGED, {"action": "delete", "instanceId": instance_id})
            return self._ok({"deleted": deleted})
        except Exception as e:
            return self._err(e)

    def instances_select(self, instance_id: str) -> dict:
        try:
            self._ctx.config.set("selectedInstance", instance_id, flush_now=True)
            self._publish(ev.SELECTION_CHANGED, {"instanceId": instance_id})
            return self._ok({"selected": instance_id})
        except Exception as e:
            return self._err(e)

    # ---------- minecraft ----------
    def versions_list(self, loader: str = "vanilla") -> dict:
        try:
            from services.loaders.registry import get_loader
            versions = get_loader(loader).list_versions()
            return self._ok({"versions": versions})
        except Exception as e:
            return self._err(e)

    def minecraft_launch(self, instance_id: str) -> dict:
        try:
            task = self._ctx.tasks.create("LAUNCH", owner=f"instance:{instance_id}")
            self._ctx.tasks.start(task)
            import threading
            from services.minecraft.launch.orchestrator import LaunchOrchestrator

            orch = LaunchOrchestrator(
                self._ctx, self._ctx.get("java"), self._ctx.get("accounts"))

            def work():
                try:
                    result = orch.launch(instance_id, task)
                    self._ctx.tasks.complete(task, result)
                except Exception as e:
                    from core.errors.base import AntaresError
                    if isinstance(e, AntaresError):
                        self._ctx.tasks.fail(task, e.to_dict())
                    else:
                        self._ctx.tasks.fail(task, {"code": "INTERNAL_ERROR",
                                                    "message": str(e)})

            threading.Thread(target=work, daemon=True).start()
            return self._ok({"taskId": task.id})
        except Exception as e:
            return self._err(e)

    def tasks_list(self) -> dict:
        try:
            return self._ok({"tasks": [t.to_dict() for t in self._ctx.tasks.list()]})
        except Exception as e:
            return self._err(e)

    def tasks_cancel(self, task_id: str) -> dict:
        try:
            return self._ok({"cancelled": self._ctx.tasks.cancel(task_id)})
        except Exception as e:
            return self._err(e)

    # ---------- mods ----------
    def mods_search(self, query: str, loader: str, mc_version: str) -> dict:
        try:
            hits = self._ctx.get("mods").search(query, loader=loader, mc_version=mc_version)
            return self._ok({"hits": [
                {"projectId": h.get("project_id"), "title": h.get("title"),
                 "author": h.get("author"), "downloads": h.get("downloads", 0)}
                for h in hits]})
        except Exception as e:
            return self._err(e)

    def mods_install(self, project_id: str, instance_id: str,
                     loader: str, mc_version: str) -> dict:
        try:
            task = self._ctx.tasks.create("MOD_INSTALL", owner=f"instance:{instance_id}")
            self._ctx.tasks.start(task)
            fname = self._ctx.get("mods").install(
                project_id, instance_id, loader=loader, mc_version=mc_version, task=task)
            self._ctx.tasks.complete(task, {"filename": fname})
            self._publish(ev.MODS_CHANGED, {"action": "install", "instanceId": instance_id})
            return self._ok({"filename": fname})
        except Exception as e:
            return self._err(e)

    def mods_list_installed(self, instance_id: str) -> dict:
        try:
            return self._ok({"mods": self._ctx.get("mods").list_installed(instance_id)})
        except Exception as e:
            return self._err(e)

    def mods_remove(self, instance_id: str, filename: str) -> dict:
        try:
            removed = self._ctx.get("mods").uninstall(instance_id, filename)
            self._publish(ev.MODS_CHANGED, {"action": "remove", "instanceId": instance_id})
            return self._ok({"removed": removed})
        except Exception as e:
            return self._err(e)

    # ---------- servers ----------
    def servers_list(self) -> dict:
        try:
            return self._ok({"servers": self._ctx.get("servers").list()})
        except Exception as e:
            return self._err(e)

    def servers_create(self, name: str, software: str, version: str, **kw) -> dict:
        try:
            meta = self._ctx.get("servers").create(name, software, version, **kw)
            self._publish(ev.SERVERS_CHANGED, {"action": "create", "serverId": meta.get("id")})
            return self._ok({"server": meta})
        except Exception as e:
            return self._err(e)

    def servers_start(self, server_id: str) -> dict:
        try:
            task = self._ctx.tasks.create("SERVER_START", owner=f"server:{server_id}")
            self._ctx.tasks.start(task)
            import threading

            def work():
                try:
                    result = self._ctx.get("servers").start(server_id, task)
                    self._ctx.tasks.complete(task, result)
                except Exception as e:
                    from core.errors.base import AntaresError
                    err = e.to_dict() if isinstance(e, AntaresError) else {
                        "code": "SERVER_START_FAILED", "message": str(e)}
                    self._ctx.tasks.fail(task, err)

            threading.Thread(target=work, daemon=True).start()
            return self._ok({"taskId": task.id})
        except Exception as e:
            return self._err(e)

    def servers_stop(self, server_id: str) -> dict:
        try:
            self._ctx.get("servers").stop(server_id)
            self._publish(ev.SERVERS_CHANGED, {"action": "stop", "serverId": server_id})
            return self._ok()
        except Exception as e:
            return self._err(e)

    def servers_command(self, server_id: str, command: str) -> dict:
        try:
            self._ctx.get("servers").send_command(server_id, command)
            return self._ok()
        except Exception as e:
            return self._err(e)

    def servers_delete(self, server_id: str, confirm: bool = False) -> dict:
        try:
            deleted = self._ctx.get("servers").delete(server_id, confirm=confirm)
            self._publish(ev.SERVERS_CHANGED, {"action": "delete", "serverId": server_id})
            return self._ok({"deleted": deleted})
        except Exception as e:
            return self._err(e)

    # ---------- game optimization (spec 3.0 mục 3, 70) ----------
    def optimization_scan(self, instance_id: str | None = None) -> dict:
        try:
            return self._ok(self._ctx.get("optimization").scan(instance_id))
        except Exception as e:
            return self._err(e)

    def optimization_plan(self, instance_id: str, profile_id: str) -> dict:
        """Preview diff — không ghi gì (mục 3.1: preview trước apply)."""
        try:
            return self._ok(self._ctx.get("optimization").plan(instance_id, profile_id))
        except Exception as e:
            return self._err(e)

    def optimization_apply(self, instance_id: str, profile_id: str) -> dict:
        try:
            result = self._ctx.get("optimization").apply(instance_id, profile_id)
            self._publish(ev.INSTANCES_CHANGED, {"action": "optimize", "instanceId": instance_id})
            return self._ok(result)
        except Exception as e:
            return self._err(e)

    def optimization_rollback(self, instance_id: str) -> dict:
        try:
            result = self._ctx.get("optimization").rollback(instance_id)
            self._publish(ev.INSTANCES_CHANGED, {"action": "optimize", "instanceId": instance_id})
            return self._ok(result)
        except Exception as e:
            return self._err(e)

    def optimization_snapshot_info(self, instance_id: str) -> dict:
        try:
            return self._ok({"snapshot": self._ctx.get("optimization").snapshot_info(instance_id)})
        except Exception as e:
            return self._err(e)

    # ---------- skin & cape studio (spec 3.0 mục 43) ----------
    def skins_list(self, kind: str | None = None) -> dict:
        try:
            return self._ok(self._ctx.get("skins").list(kind))
        except Exception as e:
            return self._err(e)

    def skins_import(self, data_b64: str, name: str = "", kind: str = "skin") -> dict:
        """Import PNG từ base64 (frontend đọc file — không pass path)."""
        try:
            import base64
            data = base64.b64decode(data_b64)
            svc = self._ctx.get("skins")
            if str(kind) == "cape":
                entry = svc.import_cape(data, name)
            else:
                entry = svc.import_skin(data, name)
            return self._ok({"item": entry})
        except Exception as e:
            return self._err(e)

    def skins_generate(self, kind: str, name: str, base_color: str,
                       accent_color: str = "#3b2a1a") -> dict:
        try:
            return self._ok({"item": self._ctx.get("skins").generate(
                kind, name, base_color, accent_color)})
        except Exception as e:
            return self._err(e)

    def skins_delete(self, kind: str, item_id: str, confirm: bool = False) -> dict:
        try:
            return self._ok({"deleted": self._ctx.get("skins").delete(
                kind, item_id, confirm=confirm)})
        except Exception as e:
            return self._err(e)

    def skins_preview(self, kind: str, item_id: str) -> dict:
        try:
            return self._ok({"preview": self._ctx.get("skins").png_data_uri(kind, item_id)})
        except Exception as e:
            return self._err(e)

    def skins_apply(self, instance_id: str, kind: str, item_id: str | None = None) -> dict:
        try:
            r = self._ctx.get("skins").apply(instance_id, kind, item_id)
            self._publish(ev.INSTANCES_CHANGED, {"action": f"skin-{kind}",
                                                 "instanceId": instance_id})
            return self._ok(r)
        except Exception as e:
            return self._err(e)

    def skins_applied(self, instance_id: str) -> dict:
        try:
            return self._ok({"applied": self._ctx.get("skins").applied(instance_id)})
        except Exception as e:
            return self._err(e)

    # ---------- player profiles (spec 3.0 mục 40) ----------
    def profiles_list(self) -> dict:
        try:
            return self._ok({"profiles": self._ctx.get("profiles").list()})
        except Exception as e:
            return self._err(e)

    def profiles_get(self, profile_id: str) -> dict:
        try:
            return self._ok({"profile": self._ctx.get("profiles").get(profile_id)})
        except Exception as e:
            return self._err(e)

    def profiles_create(self, name: str, spec: dict | None = None) -> dict:
        try:
            return self._ok({"profile": self._ctx.get("profiles").create(name, spec)})
        except Exception as e:
            return self._err(e)

    def profiles_duplicate(self, profile_id: str) -> dict:
        try:
            return self._ok({"profile": self._ctx.get("profiles").duplicate(profile_id)})
        except Exception as e:
            return self._err(e)

    def profiles_update(self, profile_id: str, patch: dict) -> dict:
        try:
            return self._ok({"profile": self._ctx.get("profiles").update(profile_id, patch)})
        except Exception as e:
            return self._err(e)

    def profiles_delete(self, profile_id: str, confirm: bool = False) -> dict:
        try:
            return self._ok({"deleted": self._ctx.get("profiles").delete(profile_id, confirm=confirm)})
        except Exception as e:
            return self._err(e)

    def profiles_capture(self, instance_id: str, name: str = "") -> dict:
        try:
            return self._ok({"profile": self._ctx.get("profiles").capture(
                instance_id, name or None)})
        except Exception as e:
            return self._err(e)

    def profiles_plan(self, profile_id: str) -> dict:
        """Preview diff trước apply — không ghi gì (mục 39 style)."""
        try:
            return self._ok(self._ctx.get("profiles").plan(profile_id))
        except Exception as e:
            return self._err(e)

    def profiles_apply(self, profile_id: str) -> dict:
        try:
            result = self._ctx.get("profiles").apply(profile_id)
            self._publish(ev.INSTANCES_CHANGED, {"action": "profile-apply"})
            return self._ok(result)
        except Exception as e:
            return self._err(e)

    def profiles_revert(self) -> dict:
        try:
            restored = self._ctx.get("profiles").revert()
            self._publish(ev.INSTANCES_CHANGED, {"action": "profile-revert"})
            return self._ok({"restored": restored})
        except Exception as e:
            return self._err(e)

    def profiles_export(self, profile_id: str) -> dict:
        try:
            return self._ok({"data": self._ctx.get("profiles").export_data(profile_id)})
        except Exception as e:
            return self._err(e)

    def profiles_import(self, data: dict) -> dict:
        try:
            return self._ok({"profile": self._ctx.get("profiles").import_data(data)})
        except Exception as e:
            return self._err(e)

    def profiles_validate(self, spec: dict) -> dict:
        try:
            return self._ok({"issues": self._ctx.get("profiles").validate_spec(spec or {})})
        except Exception as e:
            return self._err(e)

    # ---------- resource pack studio (spec 3.0 mục 10, 33) ----------
    def resource_wizard_info(self) -> dict:
        try:
            return self._ok(self._ctx.get("resource_studio").wizard_info())
        except Exception as e:
            return self._err(e)

    def resource_list(self) -> dict:
        try:
            return self._ok({"projects": self._ctx.get("resource_studio").list()})
        except Exception as e:
            return self._err(e)

    # ---------- asset library (RS v2 Batch 5 — mục 12) ----------
    def asset_list(self, query: str = "", category: str = "", tag: str = "",
                   sort: str = "newest") -> dict:
        try:
            return self._ok({"assets": self._ctx.get("assets").list(
                query=query, category=category, tag=tag, sort=sort)})
        except Exception as e:
            return self._err(e)

    def asset_import(self, data_b64: str, name: str = "", category: str = "item",
                     tags: list | None = None) -> dict:
        """Import PNG từ base64 (frontend đọc file -> base64, không pass path)."""
        try:
            import base64
            data = base64.b64decode(data_b64)
            entry = self._ctx.get("assets").import_png(
                data, name=name, category=category, tags=tags)
            return self._ok({"asset": entry})
        except Exception as e:
            return self._err(e)

    def asset_get(self, asset_id: str) -> dict:
        try:
            store = self._ctx.get("assets")
            return self._ok({"asset": store.get(asset_id),
                             "preview": store.png_data_uri(asset_id)})
        except Exception as e:
            return self._err(e)

    def asset_delete(self, asset_id: str) -> dict:
        try:
            return self._ok({"deleted": self._ctx.get("assets").delete(asset_id)})
        except Exception as e:
            return self._err(e)

    def asset_assign(self, asset_id: str, project_id: str, target_rel: str) -> dict:
        try:
            return self._ok(self._ctx.get("assets").assign_to_project(
                project_id, asset_id, target_rel))
        except Exception as e:
            return self._err(e)

    # ---------- zip pack import (RS v2 Batch 6 — mục 13) ----------
    _ZIP_UPLOAD_MAX = 64 * 1024 * 1024       # ZIP file vốn nén — cap 64MB compressed

    def _zip_to_temp(self, data_b64: str) -> Path:
        """base64 -> temp file. Caller phải unlink sau khi dùng."""
        import base64
        import os
        import tempfile
        data = base64.b64decode(data_b64)
        if len(data) > self._ZIP_UPLOAD_MAX:
            raise AntaresError(codes.ZIP_TOO_LARGE,
                               f"ZIP quá lớn: {len(data)} bytes")
        fd, tmp = tempfile.mkstemp(suffix=".zip")
        try:
            with os.fdopen(fd, "wb") as f:
                f.write(data)
        except Exception:
            os.unlink(tmp)
            raise
        return Path(tmp)

    def resource_zip_inspect(self, data_b64: str) -> dict:
        """Frontend gửi ZIP base64 -> temp file -> inspect (không ghi gì)."""
        tmp = None
        try:
            tmp = self._zip_to_temp(data_b64)
            return self._ok(self._ctx.get("resource_studio").zip_inspect(tmp))
        except Exception as e:
            return self._err(e)
        finally:
            if tmp is not None:
                tmp.unlink(missing_ok=True)

    def resource_zip_import(self, project_id: str, data_b64: str,
                            policy: str = "keep",
                            conflict_overrides: dict | None = None) -> dict:
        tmp = None
        try:
            tmp = self._zip_to_temp(data_b64)
            return self._ok(self._ctx.get("resource_studio").zip_import(
                project_id, tmp, policy=policy,
                conflict_overrides=conflict_overrides))
        except Exception as e:
            return self._err(e)
        finally:
            if tmp is not None:
                tmp.unlink(missing_ok=True)

    def resource_get(self, project_id: str) -> dict:
        try:
            return self._ok({"project": self._ctx.get("resource_studio").get(project_id)})
        except Exception as e:
            return self._err(e)

    def resource_create(self, name: str, mc_version: str,
                        template: str = "minimal", description: str = "") -> dict:
        try:
            project = self._ctx.get("resource_studio").create(name, mc_version, template, description)
            return self._ok({"project": project})
        except Exception as e:
            return self._err(e)

    def resource_delete(self, project_id: str) -> dict:
        try:
            return self._ok({"deleted": self._ctx.get("resource_studio").delete(project_id)})
        except Exception as e:
            return self._err(e)

    def resource_validate(self, project_id: str) -> dict:
        try:
            return self._ok(self._ctx.get("resource_studio").validate(project_id))
        except Exception as e:
            return self._err(e)

    def resource_build(self, project_id: str) -> dict:
        try:
            # event RESOURCE_BUILT do service publish (service-layer, mọi entrypoint)
            return self._ok(self._ctx.get("resource_studio").build(project_id))
        except Exception as e:
            return self._err(e)

    def resource_build_task(self, project_id: str) -> dict:
        """Build dưới Task — progress qua task.updated events (mục 17)."""
        try:
            return self._ok(self._ctx.get("resource_studio").build_task(project_id))
        except Exception as e:
            return self._err(e)

    def resource_build_cancel(self, task_id: str) -> dict:
        try:
            return self._ok({"cancelled": self._ctx.tasks.cancel(task_id)})
        except Exception as e:
            return self._err(e)

    def resource_builds(self, project_id: str) -> dict:
        try:
            return self._ok({"builds": self._ctx.get("resource_studio").list_builds(project_id)})
        except Exception as e:
            return self._err(e)

    def resource_install(self, project_id: str, instance_id: str,
                         overwrite: bool = False) -> dict:
        try:
            # event RESOURCE_INSTALLED do service publish
            return self._ok(self._ctx.get("resource_studio").install(
                project_id, instance_id, overwrite=overwrite))
        except Exception as e:
            return self._err(e)

    def resource_installed(self, instance_id: str) -> dict:
        try:
            return self._ok({"packs": self._ctx.get("resource_studio").installed(instance_id)})
        except Exception as e:
            return self._err(e)

    def resource_uninstall(self, instance_id: str, filename: str) -> dict:
        try:
            return self._ok({"removed": self._ctx.get("resource_studio").uninstall(
                instance_id, filename)})
        except Exception as e:
            return self._err(e)

    # ---------- multi-pack layering (RS v2 Batch 7 — mục 25) ----------
    def resource_layer_get(self, instance_id: str) -> dict:
        try:
            rs = self._ctx.get("resource_studio")
            order = rs.layers.get_order(instance_id)
            return self._ok({"order": order, "preview": rs.layers._preview_for(
                instance_id, order)})
        except Exception as e:
            return self._err(e)

    def resource_layer_set(self, instance_id: str, order: list) -> dict:
        try:
            return self._ok({"order": self._ctx.get("resource_studio").layers.set_order(
                instance_id, order)})
        except Exception as e:
            return self._err(e)

    def resource_layer_move(self, instance_id: str, filename: str, delta: int) -> dict:
        try:
            return self._ok({"order": self._ctx.get("resource_studio").layers.move(
                instance_id, filename, delta)})
        except Exception as e:
            return self._err(e)

    def resource_effective_preview(self, instance_id: str) -> dict:
        """Preview effective asset + conflicts (mục 25 gate: deterministic)."""
        try:
            rs = self._ctx.get("resource_studio")
            return self._ok(rs.layers.preview_effective())
        except Exception as e:
            return self._err(e)

    # ---------- runtime companion (spec 3.0 mục 12, 13) ----------
    def runtime_endpoint(self) -> dict:
        """Endpoint + token cho companion mod kết nối."""
        try:
            return self._ok(self._ctx.get("runtime").endpoint())
        except Exception as e:
            return self._err(e)

    def runtime_sessions(self) -> dict:
        try:
            return self._ok({"sessions": self._ctx.get("runtime").sessions()})
        except Exception as e:
            return self._err(e)

    def runtime_metrics(self, session_id: str | None = None) -> dict:
        try:
            return self._ok(self._ctx.get("runtime").metrics(session_id))
        except Exception as e:
            return self._err(e)

    # ---------- repair (spec 3.0 mục 39) ----------
    def repair_actions(self) -> dict:
        try:
            from services.repair.service import ACTIONS
            return self._ok({"actions": list(ACTIONS)})
        except Exception as e:
            return self._err(e)

    def repair_scan(self, action: str, instance_id: str | None = None) -> dict:
        """Dry-run — "What will change?" (mục 39)."""
        try:
            return self._ok(self._ctx.get("repair").scan(action, instance_id))
        except Exception as e:
            return self._err(e)

    def repair_run(self, action: str, instance_id: str | None = None) -> dict:
        try:
            return self._ok(self._ctx.get("repair").run(action, instance_id))
        except Exception as e:
            return self._err(e)

    # ---------- backups (spec 3.0 mục 22) ----------
    def backups_create(self, instance_id: str | None = None,
                       targets=None, label: str = "") -> dict:
        try:
            targets_list = [str(t) for t in targets] if targets else None
            # event backup.created do service publish
            meta = self._ctx.get("backups").create(
                instance_id=instance_id or None, targets=targets_list, label=label or "")
            return self._ok({"snapshot": meta})
        except Exception as e:
            return self._err(e)

    def backups_list(self) -> dict:
        try:
            return self._ok({"snapshots": self._ctx.get("backups").list()})
        except Exception as e:
            return self._err(e)

    def backups_restore(self, snapshot_id: str) -> dict:
        try:
            result = self._ctx.get("backups").restore(snapshot_id)
            self._publish(ev.INSTANCES_CHANGED, {"action": "restore"})
            return self._ok(result)
        except Exception as e:
            return self._err(e)

    def backups_delete(self, snapshot_id: str) -> dict:
        try:
            return self._ok({"deleted": self._ctx.get("backups").delete(snapshot_id)})
        except Exception as e:
            return self._err(e)

    def backups_export_path(self, snapshot_id: str) -> dict:
        """Chuẩn bị ZIP export, trả path để UI mở thư mục (mục 22 Export ZIP)."""
        try:
            p = self._ctx.get("backups").export_zip(snapshot_id)
            return self._ok({"path": str(p), "bytes": p.stat().st_size})
        except Exception as e:
            return self._err(e)

    # ---------- visual studio (spec 3.0 mục 8) ----------
    def visual_presets(self) -> dict:
        try:
            return self._ok(self._ctx.get("visual_studio").presets())
        except Exception as e:
            return self._err(e)

    def visual_render_preview(self, spec: dict) -> dict:
        """PNG data URI cho preview thật (khớp pixel với pack export)."""
        try:
            return self._ok({"preview": self._ctx.get("visual_studio").render_preview_b64(spec)})
        except Exception as e:
            return self._err(e)

    def visual_totem_presets(self) -> dict:
        try:
            return self._ok(self._ctx.get("visual_studio").totem_presets())
        except Exception as e:
            return self._err(e)

    def visual_totem_model_get(self) -> dict:
        """Draft voxel model Totem 3D (mục 38: draft nằm backend store)."""
        try:
            return self._ok(self._ctx.get("visual_studio").totem_model_get())
        except Exception as e:
            return self._err(e)

    def visual_totem_model_save(self, spec: dict) -> dict:
        """Lưu draft voxel model — atomic + validate (mục 62: autosave)."""
        try:
            return self._ok(self._ctx.get("visual_studio").totem_model_save(spec))
        except Exception as e:
            return self._err(e)

    def visual_render_totem_model(self, spec: dict, size: int = 256) -> dict:
        """Static render PNG data URI — fallback khi không có WebGL/thumbnail."""
        try:
            return self._ok({"preview":
                             self._ctx.get("visual_studio").render_totem_model_b64(
                                 spec, size=size)})
        except Exception as e:
            return self._err(e)

    def visual_render_totem(self, spec: dict) -> dict:
        try:
            return self._ok({"preview": self._ctx.get("visual_studio").render_totem_b64(spec)})
        except Exception as e:
            return self._err(e)

    def visual_export_totem_pack(self, name: str, spec: dict, mc_version: str,
                                 install_instance_id: str | None = None,
                                 overwrite: bool = False) -> dict:
        try:
            return self._ok(self._ctx.get("visual_studio").export_totem_pack(
                name, spec, mc_version,
                install_instance_id=install_instance_id or None,
                overwrite=overwrite))
        except Exception as e:
            return self._err(e)

    def visual_hud_widgets(self) -> dict:
        try:
            return self._ok(self._ctx.get("visual_studio").hud_widgets())
        except Exception as e:
            return self._err(e)

    def visual_save_hud_layout(self, project_id: str, layout: list) -> dict:
        try:
            return self._ok(self._ctx.get("visual_studio").save_hud_layout(
                project_id, layout))
        except Exception as e:
            return self._err(e)

    def visual_fx_defaults(self) -> dict:
        """Hit kinds + particle shapes + spec mặc định (mục 52)."""
        try:
            return self._ok(self._ctx.get("visual_studio").fx_defaults())
        except Exception as e:
            return self._err(e)

    def visual_render_hit(self, spec: dict) -> dict:
        try:
            return self._ok({"preview": self._ctx.get("visual_studio").render_hit_b64(spec)})
        except Exception as e:
            return self._err(e)

    def visual_render_particle(self, spec: dict) -> dict:
        try:
            return self._ok({"preview": self._ctx.get("visual_studio").render_particle_b64(spec)})
        except Exception as e:
            return self._err(e)

    def visual_export_fx_pack(self, name: str, hit_spec: dict | None,
                              particle_spec: dict | None, mc_version: str,
                              install_instance_id: str | None = None,
                              overwrite: bool = False) -> dict:
        try:
            return self._ok(self._ctx.get("visual_studio").export_fx_pack(
                name, hit_spec, particle_spec, mc_version,
                install_instance_id=install_instance_id or None,
                overwrite=overwrite))
        except Exception as e:
            return self._err(e)

    def visual_export_pack(self, name: str, spec: dict, mc_version: str,
                           install_instance_id: str | None = None,
                           overwrite: bool = False) -> dict:
        try:
            return self._ok(self._ctx.get("visual_studio").export_pack(
                name, spec, mc_version,
                install_instance_id=install_instance_id or None,
                overwrite=overwrite))
        except Exception as e:
            return self._err(e)

    # ---------- system optimization (spec 3.0 mục 4, 35-37) ----------
    def system_overview(self) -> dict:
        try:
            return self._ok(self._ctx.get("system_opt").overview())
        except Exception as e:
            return self._err(e)

    def system_cleanup_scan(self) -> dict:
        try:
            return self._ok(self._ctx.get("system_opt").cleanup_scan())
        except Exception as e:
            return self._err(e)

    def system_cleanup_clean(self, paths: list) -> dict:
        try:
            if not isinstance(paths, list) or not paths:
                raise AntaresError(codes.VALIDATION_FAILED, "paths must be a non-empty list")
            result = self._ctx.get("system_opt").cleanup_clean([str(p) for p in paths])
            self._publish(ev.SYSTEM_CLEANED, {"bytes": result.get("bytes", 0),
                                              "cleanId": result.get("cleanId")})
            return self._ok(result)
        except Exception as e:
            return self._err(e)

    def system_cleanup_undo(self, clean_id: str) -> dict:
        try:
            return self._ok(self._ctx.get("system_opt").cleanup_undo(clean_id))
        except Exception as e:
            return self._err(e)

    def system_cleanup_empty_trash(self) -> dict:
        try:
            return self._ok({"removed": self._ctx.get("system_opt").cleanup_empty_trash()})
        except Exception as e:
            return self._err(e)

    def system_power_set_plan(self, plan_id: str) -> dict:
        """User-confirmed action (UI confirm trước khi gọi — mục 35 requiresAdmin)."""
        try:
            return self._ok({"applied": self._ctx.get("system_opt").power_set_plan(plan_id)})
        except Exception as e:
            return self._err(e)

    def system_process_priority(self, key: str, level: str) -> dict:
        try:
            return self._ok({"applied": self._ctx.get("system_opt").process_apply_priority(key, level)})
        except Exception as e:
            return self._err(e)

    def system_process_affinity(self, key: str, cores) -> dict:
        try:
            cores_list = [int(c) for c in cores] if cores else None
            return self._ok({
                "applied": self._ctx.get("system_opt").process_apply_affinity(key, cores_list)})
        except Exception as e:
            return self._err(e)

    # ---------- performance (spec 3.0 mục 5, 89) ----------
    def performance_snapshot(self) -> dict:
        """Snapshot hiện tại + lịch sử trong RAM cho vẽ graph ban đầu."""
        try:
            return self._ok(self._ctx.get("telemetry").snapshot())
        except Exception as e:
            return self._err(e)

    def performance_set_enabled(self, enabled: bool) -> dict:
        """Bật/tắt sampler (mục 62: telemetry retention — user có quyền tắt)."""
        try:
            self._ctx.get("telemetry").set_enabled(bool(enabled))
            self._publish(ev.SETTINGS_CHANGED, {"section": "performance"})
            return self._ok({"enabled": bool(enabled)})
        except Exception as e:
            return self._err(e)

    # ---------- notifications (spec 3.0 mục 6) ----------
    def notifications_list(self, limit: int = 100, unread_only: bool = False) -> dict:
        try:
            svc = self._ctx.get("notifications")
            return self._ok({"notifications": svc.list(limit=limit, unread_only=unread_only),
                             "unread": svc.unread_count()})
        except Exception as e:
            return self._err(e)

    def notifications_unread_count(self) -> dict:
        try:
            return self._ok({"unread": self._ctx.get("notifications").unread_count()})
        except Exception as e:
            return self._err(e)

    def notifications_mark_read(self, notification_id: str | None = None) -> dict:
        try:
            n = self._ctx.get("notifications").mark_read(notification_id)
            return self._ok({"updated": n})
        except Exception as e:
            return self._err(e)

    def notifications_clear(self, notification_id: str | None = None) -> dict:
        try:
            n = self._ctx.get("notifications").clear(notification_id)
            return self._ok({"removed": n})
        except Exception as e:
            return self._err(e)

    def notifications_test(self) -> dict:
        """Gửi 1 notification mẫu (demo/debug từ Settings)."""
        try:
            item = self._ctx.get("notifications").info(
                "Hello from Antares", "Notification Center is working.")
            return self._ok({"notification": item})
        except Exception as e:
            return self._err(e)

    # ---------- logs ----------
    # ---------- java (mục 57 Diagnostics) ----------
    def java_list(self) -> dict:
        """Danh sách Java trên máy — port học từ java_utils.find_system_java_versions."""
        try:
            from services.java.discovery import java_info, scan_system_java
            homes = self._ctx.get("java_homes") or scan_system_java()
            infos = [info for h in homes if (info := java_info(h))]
            return self._ok({"javas": infos})
        except Exception as e:
            return self._err(e)

    # ---------- modpack (.mrpack — mục 88, 515) ----------
    def modpack_install(self, mrpack_path: str, instance_id: str) -> dict:
        """Cài .mrpack nền (async) — trả taskId để UI theo dõi qua event."""
        try:
            from pathlib import Path
            from services.mods.mrpack import read_mrpack_info
            path = Path(mrpack_path)
            read_mrpack_info(path)  # validate trước khi trả taskId
            task = self._ctx.tasks.create("MODPACK_INSTALL", owner=f"instance:{instance_id}")
            self._ctx.tasks.start(task)

            import threading

            def work():
                try:
                    from services.mods.mrpack import install_mrpack
                    result = install_mrpack(self._ctx, path, instance_id, task)
                    self._ctx.tasks.complete(task, result)
                except Exception as e:
                    from core.errors.base import AntaresError
                    err = e.to_dict() if isinstance(e, AntaresError) else {
                        "code": "MODPACK_INSTALL_FAILED", "message": str(e)}
                    self._ctx.tasks.fail(task, err)
                finally:
                    self._publish(ev.INSTANCES_CHANGED,
                                  {"action": "modpack", "instanceId": instance_id})

            threading.Thread(target=work, daemon=True).start()
            return self._ok({"taskId": task.id})
        except Exception as e:
            return self._err(e)

    # ---------- settings (i18n persist — mục 575-577) ----------
    def settings_get(self) -> dict:
        try:
            return self._ok({
                "ui": self._ctx.config.section("ui"),
                "performance": self._ctx.config.section("performance"),
                "network": self._ctx.config.section("network"),
            })
        except Exception as e:
            return self._err(e)

    def settings_update(self, section: str, values: dict) -> dict:
        try:
            if section not in _SETTINGS_SECTIONS:
                raise AntaresError(codes.VALIDATION_FAILED,
                                   f"Unknown settings section: {section}")
            if not isinstance(values, dict):
                raise AntaresError(codes.VALIDATION_FAILED, "values must be a dict")
            self._ctx.config.update_section(section, values, flush_now=True)
            self._publish(ev.SETTINGS_CHANGED, {"section": section})
            return self._ok({"saved": True})
        except Exception as e:
            return self._err(e)

    # ---------- mod security & health (mục 306-308, 368) ----------
    def modscan_file(self, path: str) -> dict:
        """Quét 1 jar bất kỳ — phân tích bytecode tìm behavior nguy hiểm."""
        try:
            from pathlib import Path
            return self._ok({"report": self._ctx.get("mod_security").scan_file(Path(path))})
        except Exception as e:
            return self._err(e)

    def modscan_upload(self, filename: str, content_b64: str) -> dict:
        """Scan file upload (base64 content) — phân tích sâu bytecode."""
        try:
            import base64
            content = base64.b64decode(content_b64)
            return self._ok({
                "report": self._ctx.get("mod_security").scan_uploaded(filename, content)})
        except Exception as e:
            return self._err(e)

    def modscan_instance(self, instance_id: str) -> dict:
        """Quét toàn bộ mods dir — tự quarantine mod DANGEROUS."""
        try:
            task = self._ctx.tasks.create("MOD_SCAN", owner=f"instance:{instance_id}")
            self._ctx.tasks.start(task)
            import threading

            def work():
                try:
                    result = self._ctx.get("mod_security").scan_instance(instance_id, task)
                    self._ctx.tasks.complete(task, result)
                except Exception as e:
                    from core.errors.base import AntaresError
                    err = e.to_dict() if isinstance(e, AntaresError) else {
                        "code": "INTERNAL_ERROR", "message": str(e)}
                    self._ctx.tasks.fail(task, err)

            threading.Thread(target=work, daemon=True).start()
            return self._ok({"taskId": task.id})
        except Exception as e:
            return self._err(e)

    def mods_health(self, instance_id: str, include_outdated: bool = False) -> dict:
        """Health check: thiếu deps, wrong loader, outdated (từ Modrinth)."""
        try:
            inst = self._ctx.get("instances").get(instance_id)
            if not inst:
                raise AntaresError(codes.INSTANCE_NOT_FOUND, "Instance not found")
            result = self._ctx.get("mod_security").health_check(
                instance_id, loader=inst.get("loader", "vanilla"),
                mc_version=inst.get("minecraftVersion", ""),
                include_outdated=include_outdated)
            return self._ok(result)
        except Exception as e:
            return self._err(e)

    def mods_autofix(self, instance_id: str) -> dict:
        """Tự tải dependency thiếu (vd Fabric API) từ Modrinth."""
        try:
            inst = self._ctx.get("instances").get(instance_id)
            if not inst:
                raise AntaresError(codes.INSTANCE_NOT_FOUND, "Instance not found")
            task = self._ctx.tasks.create("MOD_AUTOFIX", owner=f"instance:{instance_id}")
            self._ctx.tasks.start(task)
            import threading

            def work():
                try:
                    result = self._ctx.get("mod_security").auto_fix(
                        instance_id, loader=inst.get("loader", "fabric"),
                        mc_version=inst.get("minecraftVersion", "1.21"), task=task)
                    self._ctx.tasks.complete(task, result)
                except Exception as e:
                    from core.errors.base import AntaresError
                    err = e.to_dict() if isinstance(e, AntaresError) else {
                        "code": "INTERNAL_ERROR", "message": str(e)}
                    self._ctx.tasks.fail(task, err)

            threading.Thread(target=work, daemon=True).start()
            return self._ok({"taskId": task.id})
        except Exception as e:
            return self._err(e)

    def quarantine_list(self) -> dict:
        try:
            return self._ok({"items": self._ctx.get("mod_security").quarantine_list()})
        except Exception as e:
            return self._err(e)

    def quarantine_restore(self, quarantine_file: str) -> dict:
        try:
            entry = self._ctx.get("mod_security").quarantine_restore(quarantine_file)
            return self._ok({"restored": entry["originalName"]})
        except Exception as e:
            return self._err(e)

    def quarantine_delete(self, quarantine_file: str) -> dict:
        try:
            return self._ok({
                "deleted": self._ctx.get("mod_security").quarantine_delete(quarantine_file)})
        except Exception as e:
            return self._err(e)

    def logs_recent(self, limit: int = 500, since: int | None = None) -> dict:
        try:
            from core.logging.setup import (
                get_ring, get_ring_cursor_only, get_ring_snapshot,
            )
            if since is not None:
                ring = get_ring()
                if ring is not None:
                    lines, cursor = ring.snapshot_since(int(since))
                    # reset = cursor cũ đã bị buffer cắt mất -> client nạp lại toàn bộ
                    return self._ok({"lines": lines, "cursor": cursor,
                                     "reset": (cursor - len(lines)) > int(since)})
            return self._ok({"lines": get_ring_snapshot(limit),
                             "cursor": get_ring_cursor_only()})
        except Exception as e:
            return self._err(e)

    # ---------- console + diagnostics (spec 3.0 mục 41, 42) ----------
    def console_sources(self, instance_id: str | None = None) -> dict:
        """Danh sách nguồn console: launcher/minecraft/crash + trạng thái."""
        try:
            return self._ok({"sources": self._ctx.get("log_analyzer").sources(instance_id)})
        except Exception as e:
            return self._err(e)

    def console_read(self, source: str, instance_id: str | None = None,
                     limit: int = 5000) -> dict:
        """Đọc 1 nguồn -> [{n, text}] cho console ảo hoá (mục 16)."""
        try:
            return self._ok(self._ctx.get("log_analyzer").read(
                source, instance_id, limit=min(int(limit), 20000)))
        except Exception as e:
            return self._err(e)

    def console_analyze(self, instance_id: str | None = None,
                        source: str | None = None) -> dict:
        """Phân tích lỗi theo blueprint -> insights + khuyến nghị (không LLM)."""
        try:
            return self._ok(self._ctx.get("log_analyzer").analyze(instance_id, source))
        except Exception as e:
            return self._err(e)

    def console_insights(self, instance_id: str | None = None) -> dict:
        """Alias gọn cho analyze — chip insights trong tab Console."""
        try:
            ana = self._ctx.get("log_analyzer").analyze(instance_id)
            return self._ok({"insights": ana["insights"],
                             "errorCount": ana["errorCount"],
                             "warnCount": ana["warnCount"]})
        except Exception as e:
            return self._err(e)

    def net_check(self, host: str | None = None, port: int = 25565,
                  timeout: float = 4.0) -> dict:
        """TCP check endpoint launcher (host=None) hoặc host:port tuỳ ý;
        có hostname MC -> mc_ping (MOTD/players/latency) kèm theo."""
        try:
            from services.diagnostics.net import check_endpoints, mc_ping, tcp_check
            result: dict = {"endpoints": None, "tcp": None, "ping": None}
            if host:
                h = str(host).strip()
                result["tcp"] = tcp_check(h, int(port), timeout=float(timeout))
                result["ping"] = mc_ping(h, int(port), timeout=float(timeout))
            else:
                result["endpoints"] = check_endpoints(timeout=float(timeout))
            return self._ok(result)
        except Exception as e:
            return self._err(e)
