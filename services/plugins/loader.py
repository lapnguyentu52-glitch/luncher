"""PluginLoader + PluginRegistry — vòng đời plugin (spec 3.0 mục 84).

Registry là **một nguồn sự thật** cho mọi thứ plugin đã đăng ký. Loader là
**cổng vào duy nhất**: quét thư mục plugins, validate manifest, import entry,
gọi `setup(ctx)` trong sandbox, giữ vòng đời load/unload.

Nguyên tắc an toàn (mục 53, 85):
- Plugin lỗi khi load -> bị skip + event PLUGIN_ERROR, launcher vẫn chạy.
- Plugin lỗi khi chạy hook -> exception bọc lại, KHÔNG giết thread publisher.
- Unload = gỡ đăng ký + xoá sandbox khỏi registry (không cố gc module —
  importlib unload Python là nửa vời, chấp nhận cho plugin đơn giản).
"""
from __future__ import annotations

import importlib.util
import threading
from pathlib import Path

from core.events import names as ev
from core.events.bus import EventBus
from core.logging.setup import get_logger
from services.plugins.manifest import ManifestError, load_manifest
from services.plugins.sandbox import PermissionDenied, PluginSandbox

logger = get_logger("plugins")

#: Vòng đời plugin: module entry phải expose hàm này.
SETUP_FUNC = "setup"
#: Tuỳ chọn — gọi khi plugin bị disable/launcher shutdown.
TEARDOWN_FUNC = "teardown"


class PluginRegistry:
    """Ghi nhận đăng ký của plugin: command/tab/panel/optimizer/template/preset."""

    def __init__(self) -> None:
        self._lock = threading.Lock()
        self._commands: dict[str, dict] = {}     # key -> {"plugin", "handler"}
        self._tabs: list[dict] = []
        self._panels: dict[str, list[dict]] = {}  # host -> [panel]
        self._optimizers: list[dict] = []
        self._templates: list[dict] = []
        self._presets: list[dict] = []

    def _record(self, plugin_id: str, kind: str, key, **entry) -> None:
        with self._lock:
            entry["plugin"] = plugin_id
            if kind == "command":
                self._commands[key] = entry
            elif kind == "tab":
                self._tabs.append(entry)
            elif kind == "panel":
                self._panels.setdefault(key, []).append(entry)
            elif kind == "optimizer":
                self._optimizers.append(entry)
            elif kind == "template":
                self._templates.append(entry)
            elif kind == "preset":
                self._presets.append(entry)

    # -- đăng ký (gọi từ PluginContext) ---------------------------------

    def register_command(self, plugin_id: str, command_id: str, handler) -> None:
        key = f"plugin.{plugin_id}.{command_id}"
        self._record(plugin_id, "command", key, id=key, handler=handler)

    def register_tab(self, plugin_id: str, tab_id: str, title_key: str) -> None:
        key = f"plugin.{plugin_id}.{tab_id}"
        self._record(plugin_id, "tab", key, id=key, titleKey=title_key)

    def register_panel(self, plugin_id: str, panel_id: str, host: str, handler) -> None:
        key = host
        self._record(plugin_id, "panel", key,
                     id=f"plugin.{plugin_id}.{panel_id}", host=host, handler=handler)

    def register_optimizer(self, plugin_id: str, optimizer_id: str, handler) -> None:
        self._record(plugin_id, "optimizer",
                     f"plugin.{plugin_id}.{optimizer_id}", handler=handler)

    def register_resource_template(self, plugin_id: str, template_id: str, spec: dict) -> None:
        self._record(plugin_id, "template",
                     f"plugin.{plugin_id}.{template_id}", spec=spec)

    def register_visual_preset(self, plugin_id: str, kind: str, preset_id: str, spec: dict) -> None:
        self._record(plugin_id, "preset",
                     f"plugin.{plugin_id}.{kind}.{preset_id}", kind=kind, spec=spec)

    # -- đọc (cho bridge) -------------------------------------------------

    def tabs(self) -> list[dict]:
        with self._lock:
            return [dict(t) for t in self._tabs]

    def panels_for(self, host: str) -> list[dict]:
        with self._lock:
            return [dict(p) for p in self._panels.get(host, ())]

    def commands(self) -> list[dict]:
        with self._lock:
            return [dict(c) for c in self._commands.values()]

    def run_command(self, key: str) -> dict:
        """Thực thi command của plugin — exception bọc thành lỗi, không giết app."""
        with self._lock:
            entry = self._commands.get(key)
        if entry is None:
            return {"ok": False, "error": {"code": "NOT_FOUND", "message": f"không có command {key}"}}
        try:
            result = entry["handler"]()
            return {"ok": True, "data": result if isinstance(result, dict) else {"result": result}}
        except Exception as e:  # noqa: BLE001 — ranh giới plugin
            return {"ok": False, "error": {"code": "PLUGIN_ERROR", "message": str(e)}}

    # -- unload ------------------------------------------------------------

    def forget_plugin(self, plugin_id: str) -> None:
        with self._lock:
            self._commands = {k: v for k, v in self._commands.items()
                              if v.get("plugin") != plugin_id}
            self._tabs = [t for t in self._tabs if t.get("plugin") != plugin_id]
            for host, panels in self._panels.items():
                self._panels[host] = [p for p in panels if p.get("plugin") != plugin_id]
            self._optimizers = [o for o in self._optimizers if o.get("plugin") != plugin_id]
            self._templates = [t for t in self._templates if t.get("plugin") != plugin_id]
            self._presets = [p for p in self._presets if p.get("plugin") != plugin_id]


class PluginLoader:
    """Quét + load/unload plugin từ `<data>/plugins/` (mục 84)."""

    def __init__(self, ctx) -> None:
        self._ctx = ctx
        self._plugins_dir = ctx.paths.plugins
        self._registry = PluginRegistry()
        self._sandboxes: dict[str, PluginSandbox] = {}
        self._modules: dict[str, object] = {}
        self._lock = threading.Lock()
        #: disabled lưu ở config `plugins.disabled`: [id] — không xoá code user
        disabled = ctx.config.get("plugins.disabled", []) or []
        self._disabled: set[str] = set(disabled)

    # ------------------------------------------------------------------

    def scan(self) -> list[dict]:
        """Danh sách plugin tìm thấy (kể cả disabled) + trạng thái + lỗi."""
        result = []
        if not self._plugins_dir.exists():
            return result
        for d in sorted(self._plugins_dir.iterdir()):
            if not d.is_dir():
                continue
            if d.name.startswith("_"):
                continue  # thư mục reserved (_data = sandbox data), không phải plugin
            info: dict = {"id": d.name, "dir": str(d), "enabled": d.name not in self._disabled}
            try:
                mf = load_manifest(d)
                info.update({
                    "id": mf["id"], "name": mf["name"], "version": mf["version"],
                    "permissions": mf.get("permissions", []),
                    "loaded": mf["id"] in self._sandboxes,
                })
            except ManifestError as e:
                info.update({"invalid": True, "error": str(e), "loaded": False})
            result.append(info)
        return result

    def load_all(self) -> dict:
        """Load mọi plugin hợp lệ + enabled. Trả summary; lỗi từng plugin không fatal."""
        loaded, skipped = [], []
        for info in self.scan():
            if info.get("invalid"):
                skipped.append({"id": info["id"], "reason": info["error"]})
            elif info["enabled"] and not info["loaded"]:
                try:
                    self._load_one(Path(info["dir"]))
                    loaded.append(info["id"])
                except Exception as e:  # noqa: BLE001 — ranh giới plugin
                    skipped.append({"id": info["id"], "reason": str(e)})
        return {"loaded": loaded, "skipped": skipped}

    def _load_one(self, plugin_dir: Path) -> dict:
        mf = load_manifest(plugin_dir)
        pid = mf["id"]
        with self._lock:
            if pid in self._sandboxes:
                return {"id": pid, "alreadyLoaded": True}

        sandbox = PluginSandbox(mf, self._registry, self._ctx.events,
                                self._plugins_dir / "_data",
                                notifications=self._ctx.get("notifications"),
                                http=self._ctx.get("http_client"),
                                network_allowlist=self._ctx.config.get(
                                    "plugins.network.hosts", []) or [])

        # Import entry module (Python plugin — mục 84 backend/)
        entry = plugin_dir / mf["entry"]
        if not entry.exists():
            raise ManifestError(f"entry '{mf['entry']}' không tồn tại")
        spec = importlib.util.spec_from_file_location(f"antares_plugin_{pid.replace('.', '_').replace('-', '_')}", entry)
        if spec is None or spec.loader is None:
            raise ManifestError(f"không import được entry: {mf['entry']}")
        module = importlib.util.module_from_spec(spec)
        try:
            spec.loader.exec_module(module)
        except Exception as e:
            raise ManifestError(f"entry chạy lỗi khi import: {e}") from e

        setup = getattr(module, SETUP_FUNC, None)
        if not callable(setup):
            raise ManifestError(f"entry thiếu hàm {SETUP_FUNC}(ctx)")

        # setup(ctx) chạy trong sandbox — exception -> plugin bị loại
        try:
            setup(sandbox.api())
        except PermissionDenied:
            raise
        except Exception as e:  # noqa: BLE001 — ranh giới plugin
            raise ManifestError(f"setup() lỗi: {e}") from e

        with self._lock:
            self._sandboxes[pid] = sandbox
            self._modules[pid] = module
        self._ctx.events.publish(ev.PLUGIN_LOADED, {
            "pluginId": pid, "name": mf["name"], "version": mf["version"],
        })
        logger.info("Plugin loaded: %s %s", pid, mf["version"])
        return {"id": pid}

    def unload(self, plugin_id: str) -> dict:
        """Disable + unload 1 plugin: teardown() nếu có, gỡ đăng ký, lưu config."""
        with self._lock:
            sandbox = self._sandboxes.pop(plugin_id, None)
            module = self._modules.pop(plugin_id, None)
        if sandbox is None:
            return {"ok": False, "error": {"code": "NOT_LOADED", "message": f"{plugin_id} chưa load"}}
        teardown = getattr(module, TEARDOWN_FUNC, None) if module else None
        if callable(teardown):
            try:
                teardown(sandbox.api())
            except Exception:  # noqa: BLE001
                logger.exception("teardown lỗi trong %s", plugin_id)
        sandbox.context.close()                    # hủy event subscriptions
        self._registry.forget_plugin(plugin_id)
        self._disabled.add(plugin_id)
        self._save_disabled()
        self._ctx.events.publish(ev.PLUGIN_UNLOADED, {"pluginId": plugin_id})
        return {"ok": True}

    def enable(self, plugin_id: str) -> dict:
        self._disabled.discard(plugin_id)
        self._save_disabled()
        d = self._plugins_dir / plugin_id
        if d.is_dir():
            try:
                self._load_one(d)
            except ManifestError as e:
                return {"ok": False, "error": {"code": "MANIFEST", "message": str(e)}}
        return {"ok": True}

    def set_enabled(self, plugin_id: str, enabled: bool) -> dict:
        """UI toggle: enabled=False -> unload + lưu; True -> load lại."""
        if enabled:
            return self.enable(plugin_id)
        return self.unload(plugin_id)

    def _save_disabled(self) -> None:
        self._ctx.config.set("plugins.disabled", sorted(self._disabled), flush_now=True)

    def registry(self) -> PluginRegistry:
        return self._registry

    def loaded_ids(self) -> list[str]:
        with self._lock:
            return list(self._sandboxes)
