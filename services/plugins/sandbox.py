"""PluginSandbox + PluginContext — API bề mặt cho plugin (mục 84).

Sandbox là **màng lọc quyền**: plugin chỉ thấy những gì manifest cho phép.

- `PluginContext` — object được truyền vào `setup(ctx)` của plugin backend.
  Mọi phương thức đều qua `has_permission()`; thiếu quyền -> `PermissionError`
  (exception rõ ràng, plugin tự bắt).
- Không cung cấp filesystem/process/network tuỳ tiện: scope đúng spec mục 84
  (`filesystem`, `network`, `process`, `minecraft`, `ui`).
- `register_*` chỉ **ghi nhận ý định** vào `PluginRegistry` — code plugin
  KHÔNG được thực thi trực tiếp khi đăng ký; các hook được registry gọi có
  kiểm soát và mọi exception bị bọc thành event `plugin.error` (mục 53:
  app không chết vì plugin).

Ghi chú ngôn ngữ: backend plugin load qua importlib (Python). Plugin ngôn
ngữ khác dùng IPC client (như companion) — ngoài phạm vi batch này.
"""
from __future__ import annotations

import time
from pathlib import Path
from typing import Any, Callable

from core.events import names as ev
from core.events.bus import EventBus
from services.plugins.manifest import has_permission


class PermissionDenied(PermissionError):
    """Plugin gọi API ngoài quyền manifest khai báo."""


class PluginContext:
    """API object cho 1 plugin — mọi route đều kiểm tra quyền trước."""

    def __init__(self, manifest: dict, registry: "PluginRegistry",
                 events: EventBus, data_dir: Path,
                 notifications=None, http=None, network_allowlist=None) -> None:
        self._manifest = manifest
        self._registry = registry
        self._events = events
        self._notifications = notifications
        self._http = http
        self._network_allowlist = list(network_allowlist or [])
        self.id = manifest["id"]
        #: Thư mục data RIÊNG của plugin — plugin không cần quyền filesystem
        #: vẫn được ghi ở đây ( isolate, mục 85 spirit ).
        self.data_dir = data_dir
        #: Unsubscriber của event plugin đã subscribe — để unload sạch (mục 85).
        self._unsubs: list[Callable[[], None]] = []

    # -- kiểm quyền ----------------------------------------------------

    def _require(self, perm: str) -> None:
        if not has_permission(self._manifest, perm):
            raise PermissionDenied(
                f"plugin {self.id} thiếu quyền '{perm}'")

    # -- register API (mục 84) ------------------------------------------

    def register_command(self, command_id: str, handler: Callable) -> None:
        """Lệnh hiện trong command palette: `plugin.<id>.<command_id>`."""
        self._require("ui")
        self._registry.register_command(self.id, command_id, handler)

    def register_event(self, event_name: str, handler: Callable) -> None:
        """Đăng ký handler cho event launcher (chỉ event thuộc taxonomy)."""
        self._require("ui")
        if not hasattr(ev, event_name):
            raise ValueError(f"event không thuộc taxonomy: {event_name}")
        unsub = self._events.subscribe(event_name, handler)
        if unsub:
            self._unsubs.append(unsub)

    def register_tab(self, tab_id: str, *, title_key: str) -> None:
        """Khai báo tab frontend (frontend part của plugin tự render)."""
        self._require("ui")
        self._registry.register_tab(self.id, tab_id, title_key)

    def register_panel(self, panel_id: str, host: str, handler: Callable) -> None:
        """Panel nhúng vào tab có sẵn: host = 'dashboard' | 'play' | 'performance'."""
        allowed_hosts = {"dashboard", "play", "performance"}
        if host not in allowed_hosts:
            raise ValueError(f"host không hợp lệ: {host} (cho phép: {sorted(allowed_hosts)})")
        self._require("ui")
        self._registry.register_panel(self.id, panel_id, host, handler)

    def register_optimizer(self, optimizer_id: str, handler: Callable) -> None:
        """Gợi ý tối ưu tuỳ chỉnh hiện trong Game Optimization."""
        self._require("minecraft")
        self._registry.register_optimizer(self.id, optimizer_id, handler)

    def register_resource_template(self, template_id: str, spec: dict) -> None:
        """Template dự án Resource Studio (mục 84)."""
        self._require("ui")
        self._registry.register_resource_template(self.id, template_id, spec)

    def register_visual_preset(self, kind: str, preset_id: str, spec: dict) -> None:
        """Preset crosshair/totem tuỳ chỉnh (mục 84). kind: crosshair|totem."""
        if kind not in ("crosshair", "totem"):
            raise ValueError("kind phải là 'crosshair' hoặc 'totem'")
        self._require("ui")
        self._registry.register_visual_preset(self.id, kind, preset_id, spec)

    # -- scoped data API -------------------------------------------------

    def on_minecraft_telemetry(self, handler: Callable) -> None:
        """Nhận telemetry companion (mục 12) — cần 'minecraft.telemetry'."""
        self._require("minecraft.telemetry")
        unsub = self._events.subscribe(ev.RUNTIME_PERFORMANCE, handler)
        if unsub:
            self._unsubs.append(unsub)

    def close(self) -> None:
        """Hủy mọi event subscription của plugin (loader gọi lúc unload)."""
        for unsub in self._unsubs:
            try:
                unsub()
            except Exception:
                pass
        self._unsubs.clear()

    # ------------------------------------------------------------------
    # Scoped network — CHỈ qua allowlist config `plugins.network.hosts`
    # (mục 85: không plugin nào tự do gọi internet). Domain phải khớp_exact
    # hoặc là subdomain của 1 entry (vd "api.example.com" khớp entry đó,
    # không khớp "example.com" trừ khi entry ghi .example.com).
    # ------------------------------------------------------------------

    def _check_host(self, url: str) -> str:
        from urllib.parse import urlparse
        if self._http is None:
            raise PermissionDenied("network chưa khả dụng (thiếu HttpClient)")
        host = (urlparse(url).hostname or "").lower()
        if not host:
            raise ValueError(f"URL không hợp lệ: {url!r}")
        scheme = urlparse(url).scheme.lower()
        if scheme != "https":
            raise PermissionDenied("plugin chỉ được gọi HTTPS")
        for entry in self._network_allowlist:
            e = entry.lower().lstrip(".")
            if host == e or host.endswith("." + e):
                return host
        raise PermissionDenied(
            f"plugin {self.id}: host '{host}' không trong allowlist")

    def http_get_json(self, url: str, *, timeout: float = 10.0):
        """GET JSON từ host trong allowlist — cần 'network'."""
        self._require("network")
        self._check_host(url)
        return self._http.get_json(url, timeout=(timeout, timeout))

    def http_post_json(self, url: str, payload: dict, *, timeout: float = 10.0):
        """POST JSON tới host trong allowlist — cần 'network'."""
        self._require("network")
        self._check_host(url)
        return self._http.post_json(url, json=payload, timeout=(timeout, timeout))

    def read_config(self) -> dict:
        """Config riêng của plugin trong data_dir (không cần quyền FS)."""
        import json
        path = self.data_dir / "config.json"
        if not path.exists():
            return {}
        try:
            return json.loads(path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, OSError):
            return {}

    def write_config(self, data: dict) -> None:
        from core.config.writer import write_json_atomic
        self.data_dir.mkdir(parents=True, exist_ok=True)
        write_json_atomic(self.data_dir / "config.json", data)

    def log(self, message: str) -> None:
        """Log có nhãn plugin — đi qua logging chuẩn, không đụng file trực tiếp."""
        from core.logging.setup import get_logger
        get_logger(f"plugin.{self.id}").info("%s", message)

    # -- scoped filesystem (quyền 'filesystem' / 'filesystem.read') ------

    def _scoped_path(self, relative: str, *, write: bool) -> Path:
        """Resolve đường dẫn trong data_dir của plugin, chặn escape (mục 76)."""
        self._require("filesystem" if write else "filesystem.read")
        if not isinstance(relative, str) or not relative.strip():
            raise ValueError("đường dẫn phải là string không rỗng")
        root = self.data_dir.resolve()
        target = (root / relative).resolve()
        if target != root and root not in target.parents:
            raise PermissionDenied(
                f"plugin {self.id}: đường dẫn ngoài data dir ({relative})")
        return target

    def read_file(self, relative: str) -> bytes:
        """Đọc file trong data_dir riêng — cần 'filesystem.read'."""
        return self._scoped_path(relative, write=False).read_bytes()

    def write_file(self, relative: str, data: bytes) -> dict:
        """Ghi file trong data_dir riêng — cần 'filesystem' (RW)."""
        if not isinstance(data, (bytes, bytearray)):
            raise ValueError("data phải là bytes")
        if len(data) > 8 * 1024 * 1024:
            raise ValueError("file quá lớn (giới hạn 8MB)")
        p = self._scoped_path(relative, write=True)
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes(bytes(data))
        return {"path": str(p), "bytes": len(data)}

    def list_files(self, relative: str = ".") -> list[str]:
        """Liệt kê file trong data_dir (đệ quy, đường dẫn tương đối)."""
        root = self._scoped_path(relative, write=False)
        base = self.data_dir.resolve()
        if not root.exists():
            return []
        return sorted(
            str(f.relative_to(base)) for f in root.rglob("*") if f.is_file())

    def delete_file(self, relative: str) -> dict:
        """Xoá 1 file trong data_dir — cần 'filesystem' (RW)."""
        p = self._scoped_path(relative, write=True)
        if p.is_file():
            p.unlink()
            return {"deleted": str(p)}
        return {"deleted": None}

    # -- scoped network (quyền 'network' + allowlist config) -------------

    def notify(self, title: str, message: str = "", severity: str = "INFO") -> None:
        """Tạo notification — plugin có quyền 'ui'.

        Ưu tiên NotificationService (có history + dedupe, mục 80); nếu service
        chưa sẵn sàng (test headless) thì publish event để UI vẫn thấy toast.
        """
        self._require("ui")
        if self._notifications is not None:
            self._notifications.push(severity=severity, category=f"plugin:{self.id}",
                                     title=title, message=message)
            return
        self._events.publish(ev.NOTIFICATION_CREATED, {
            "id": f"plugin-{self.id}-{int(time.time() * 1000)}",
            "timestamp": time.time(),
            "severity": severity, "category": f"plugin:{self.id}",
            "title": title, "message": message,
        })


class PluginSandbox:
    """Sandbox cho 1 plugin: giữ manifest + context, chỉ expose những gì được phép."""

    def __init__(self, manifest: dict, registry: "PluginRegistry",
                 events: EventBus, data_root: Path, notifications=None,
                 http=None, network_allowlist=None) -> None:
        self.manifest = manifest
        self.id = manifest["id"]
        self.loaded_at = time.time()
        data_dir = data_root / self.id
        data_dir.mkdir(parents=True, exist_ok=True)
        self.context = PluginContext(manifest, registry, events, data_dir,
                                     notifications=notifications, http=http,
                                     network_allowlist=network_allowlist)

    def permissions(self) -> list[str]:
        return list(self._manifest_permissions())

    def _manifest_permissions(self) -> list[str]:
        return self.manifest.get("permissions") or []

    def allows(self, perm: str) -> bool:
        return has_permission(self.manifest, perm)

    # delegate getattr xuống context nhưng vẫn qua permission check
    def api(self) -> PluginContext:
        return self.context
