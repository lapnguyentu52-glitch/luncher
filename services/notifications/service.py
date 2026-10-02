"""NotificationService — hub thông báo realtime (spec 3.0 mục 6).

Thiết kế:
- Lắng nghe EventBus (wildcard) -> tự sinh Notification từ event hệ thống
  (task failed, minecraft exited, download failed...). Không poll.
- Store in-memory bounded + dedupe + coalesce update-in-place (mục 80):
  task/download progress KHÔNG tạo notification mới, chỉ cập nhật nếu
  notification tương ứng đã tồn tại.
- Severity: INFO/SUCCESS/WARNING/ERROR/CRITICAL (mục 6.2).
- Push qua EventBus -> EventBridge -> UI (kênh realtime có sẵn).
- History giữ lại trong RAM (session); persistence full là giai đoạn sau.
"""
from __future__ import annotations

import threading
import time
import uuid
from typing import Callable

from core.events import names as ev
from core.events.bus import Envelope, EventBus, WILDCARD
from core.logging.setup import get_logger

logger = get_logger("notifications")

#: Event gộp in-place theo task (không spam notification mới).
IN_PLACE_TASK_EVENTS = {ev.DOWNLOAD_PROGRESS, ev.MINECRAFT_INSTALL_PROGRESS, ev.TASK_UPDATED}

#: Độ ưu tiên tự sinh notification theo event hệ thống.
EVENT_RULES: dict[str, dict] = {
    ev.TASK_UPDATED: {"when": lambda p: p.get("state") == "failed",
                      "severity": "ERROR", "category": "task",
                      "titleKey": "taskFailed"},
    ev.MINECRAFT_STARTED: {"severity": "SUCCESS", "category": "minecraft",
                           "titleKey": "minecraftStarted"},
    ev.MINECRAFT_EXITED: {"when": lambda p: int(p.get("exitCode") or 0) != 0,
                          "severity": "ERROR", "category": "minecraft",
                          "titleKey": "minecraftCrashed"},
    ev.DOWNLOAD_FAILED: {"severity": "ERROR", "category": "download",
                         "titleKey": "downloadFailed"},
    ev.MINECRAFT_ERROR: {"severity": "ERROR", "category": "minecraft",
                         "titleKey": "minecraftError"},
    ev.SERVER_FAILED: {"severity": "ERROR", "category": "server",
                       "titleKey": "serverFailed"},
    ev.AUTH_FAILED: {"severity": "WARNING", "category": "account",
                     "titleKey": "authFailed"},
    ev.RESOURCE_BUILT: {"severity": "SUCCESS", "category": "mods",
                        "titleKey": "resourceBuilt"},
    ev.RESOURCE_INSTALLED: {"severity": "SUCCESS", "category": "mods",
                            "titleKey": "resourceInstalled"},
}

MAX_HISTORY = 200          # mục 80: history giới hạn
DEDUPE_WINDOW = 8.0        # giây — notification giống nhau trong cửa sổ này bị gộp


class Notification:
    """Model notification (mục 6.1)."""

    __slots__ = ("id", "timestamp", "severity", "category", "title",
                 "message", "actions", "read", "sticky")

    def __init__(self, *, severity: str = "INFO", category: str = "launcher",
                 title: str = "", message: str = "", actions: list | None = None,
                 sticky: bool = False) -> None:
        self.id = uuid.uuid4().hex
        self.timestamp = time.time()
        self.severity = severity
        self.category = category
        self.title = title
        self.message = message
        self.actions = actions or []
        self.read = False
        self.sticky = sticky

    def to_dict(self) -> dict:
        return {
            "id": self.id, "timestamp": self.timestamp,
            "severity": self.severity, "category": self.category,
            "title": self.title, "message": self.message,
            "actions": self.actions, "read": self.read, "sticky": self.sticky,
        }


class NotificationService:
    """Điểm duy nhất sinh + lưu notification. UI đọc qua api.bridge."""

    def __init__(self, bus: EventBus, *, max_history: int = MAX_HISTORY) -> None:
        self._bus = bus
        self._items: list[Notification] = []          # mới nhất đứng đầu
        self._lock = threading.Lock()
        self._max = max_history
        self._last_seen: dict[tuple, float] = {}      # dedupe
        self._unsub = bus.subscribe(WILDCARD, self._on_event)

    # ------------------------------------------------------------------
    # Public API
    # ------------------------------------------------------------------

    def push(self, *, severity: str = "INFO", category: str = "launcher",
             title: str, message: str = "", actions: list | None = None,
             sticky: bool = False) -> dict:
        """Tạo notification (thủ công từ service/API)."""
        item = Notification(severity=severity, category=category, title=title,
                            message=message, actions=actions, sticky=sticky)
        with self._lock:
            # Dedupe: cùng title+message trong cửa sổ ngắn -> chỉ đẩy lại timestamp.
            key = (severity, category, title, message)
            now = time.time()
            last = self._last_seen.get(key)
            if last is not None and now - last < DEDUPE_WINDOW:
                for it in self._items:
                    if (it.severity, it.category, it.title, it.message) == key:
                        it.timestamp = now
                        self._last_seen[key] = now
                        return it.to_dict()
            self._last_seen[key] = now
            self._items.insert(0, item)
            del self._items[self._max:]
        self._emit(item)
        return item.to_dict()

    def info(self, title: str, message: str = "", **kw) -> dict:
        return self.push(severity="INFO", title=title, message=message, **kw)

    def success(self, title: str, message: str = "", **kw) -> dict:
        return self.push(severity="SUCCESS", title=title, message=message, **kw)

    def warning(self, title: str, message: str = "", **kw) -> dict:
        return self.push(severity="WARNING", title=title, message=message, **kw)

    def error(self, title: str, message: str = "", **kw) -> dict:
        return self.push(severity="ERROR", title=title, message=message,
                         sticky=kw.pop("sticky", True), **kw)

    def list(self, *, limit: int = 100, unread_only: bool = False) -> list[dict]:
        with self._lock:
            items = [i for i in self._items if not (unread_only and i.read)]
        return [i.to_dict() for i in items[:limit]]

    def unread_count(self) -> int:
        with self._lock:
            return sum(1 for i in self._items if not i.read)

    def mark_read(self, notification_id: str | None = None) -> int:
        """Đọc 1 (hoặc tất cả nếu None). Trả số notification đã đổi."""
        with self._lock:
            n = 0
            for it in self._items:
                if notification_id is None or it.id == notification_id:
                    if not it.read:
                        it.read = True
                        n += 1
                    if notification_id is not None:
                        break
        if n:
            self._bus.publish(ev.NOTIFICATIONS_CHANGED, {"action": "read"})
        return n

    def clear(self, notification_id: str | None = None) -> int:
        """Xoá 1 (hoặc tất cả nếu None)."""
        with self._lock:
            if notification_id is None:
                n = len(self._items)
                self._items.clear()
            else:
                before = len(self._items)
                self._items = [i for i in self._items if i.id != notification_id]
                n = before - len(self._items)
        if n:
            self._bus.publish(ev.NOTIFICATIONS_CHANGED, {"action": "clear"})
        return n

    # ------------------------------------------------------------------
    # Event -> notification (auto)
    # ------------------------------------------------------------------

    def _on_event(self, env: Envelope) -> None:
        try:
            self._handle_event(env)
        except Exception:
            logger.exception("notification auto-rule failed")

    def _handle_event(self, env: Envelope) -> None:
        name = env.name
        payload = env.payload or {}

        # Progress/task update: KHÔNG tạo notification mới (chống spam — mục 80).
        # (Chỉ mức task.failed được rule bên dưới bắt.)
        if name in IN_PLACE_TASK_EVENTS and payload.get("state") != "failed":
            return

        rule = EVENT_RULES.get(name)
        if not rule:
            return
        when = rule.get("when")
        if when is not None and not when(payload):
            return

        self.push(
            severity=rule["severity"],
            category=rule["category"],
            title=rule["titleKey"],            # i18n key — UI dịch khi render
            message=rule.get("messageKey", ""),
            actions=rule.get("actions", []),
        )

    def _emit(self, item: Notification) -> None:
        try:
            self._bus.publish(ev.NOTIFICATION_CREATED, item.to_dict())
        except Exception:
            pass

    def close(self) -> None:
        try:
            self._unsub()
        except Exception:
            pass
