"""EventBridge — đẩy event từ EventBus xuống WebView (spec mục 8.2, 15.2).

Thay cho việc UI poll định kỳ (`setInterval -> tasks_list / logs_recent`), backend
đẩy event khi state đổi. Ba cơ chế chính:

1. **Gộp (coalescing)** — output/progress nhiều dòng được gộp thành 1 event:
   - `append`: gom nhiều dòng text (minecraft/server output, log) thành 1 batch.
   - `latest`: chỉ giữ trạng thái mới nhất cho mỗi task/instance/server.
   - còn lại: đi nguyên (event đổi state không được mất).
2. **Throttle** — flush tối đa `interval_ms` một lần (~12 nhịp/giây) để không
   nhấn chìm UI, đúng ngân sách 10–20 UI updates/sec/task.
3. **Fallback drain** — mọi thứ đi qua `flush()`; nếu WebView chưa sẵn sàng
   (hoặc `evaluate_js` lỗi) thì event nằm lại trong outbox cho command
   `events_drain()`. Nhờ vậy UI không mất event và không cần poll.

Bridge chạy được cả khi không có WebView (test/headless): không có cửa sổ thì
không có thread, `drain()` là đường duy nhất.
"""
from __future__ import annotations

import json
import threading
import time
from typing import Any, Callable

from core.events import names as ev
from core.events.bus import WILDCARD, Envelope, EventBus
from core.logging.setup import get_logger, get_ring_cursor, get_ring_since_meta

logger = get_logger("events.bridge")

DEFAULT_INTERVAL_MS = 80        # ~12 nhịp/giây
DEFAULT_MAX_QUEUE = 500         # event rời rạc giữ tối đa
MAX_LINES_PER_BATCH = 400       # dòng text gộp trong 1 event

#: Event gom theo dòng text — payload chứa key này là 1 dòng.
APPEND_EVENTS = {
    ev.MINECRAFT_OUTPUT: "line",
    ev.SERVER_OUTPUT: "line",
}

#: Event chỉ giữ bản mới nhất cho mỗi khoá (task/instance/server).
LATEST_EVENTS = {
    ev.DOWNLOAD_PROGRESS,
    ev.MINECRAFT_INSTALL_PROGRESS,
    ev.SERVER_METRICS,
    ev.TASK_UPDATED,
    ev.PERFORMANCE_TELEMETRY,   # telemetry: chỉ mẫu mới nhất -> không spam UI (mục 5.2)
    ev.RUNTIME_PERFORMANCE,     # companion FPS/frametime — cùng policy
}


def key_for(name: str, env: Envelope) -> str:
    """Khoá gộp 'latest' — tách theo task/instance/server để không đè nhau."""
    payload = env.payload or {}
    if name == ev.TASK_UPDATED:
        return str(payload.get("id") or env.task_id or "global")
    return str(
        env.task_id
        or payload.get("taskId")
        or payload.get("instanceId")
        or payload.get("serverId")
        or "global"
    )


class EventBridge:
    """Cầu nối EventBus -> frontend."""

    def __init__(
        self,
        bus: EventBus,
        *,
        interval_ms: int = DEFAULT_INTERVAL_MS,
        max_queue: int = DEFAULT_MAX_QUEUE,
        log_reader: Callable[[int], tuple[list[str], int]] | None = None,
        log_cursor: int | None = None,
    ) -> None:
        self._bus = bus
        self._interval = max(16, int(interval_ms)) / 1000.0
        self._max_queue = max(16, int(max_queue))
        self._log_reader = log_reader or get_ring_since_meta
        self._log_cursor = get_ring_cursor() if log_cursor is None else log_cursor

        self._outbox: list[dict] = []
        self._pending: dict[str, dict] = {}
        self._latest: dict[tuple[str, str], dict] = {}
        self._lock = threading.Lock()

        self._thread: threading.Thread | None = None
        self._stop = threading.Event()
        self._window: Any | None = None
        self._ready = False

        self.stats: dict[str, int] = {
            "batches": 0, "emitted": 0, "pushed": 0,
            "pushFailures": 0, "dropped": 0, "logLines": 0,
        }

        self._unsub = bus.subscribe(WILDCARD, self._on_event)

    # ------------------------------------------------------------------
    # Nhận event (chạy trên thread của publisher -> chỉ ghi buffer)
    # ------------------------------------------------------------------

    def _on_event(self, env: Envelope) -> None:
        name = env.name
        record = env.to_dict()

        with self._lock:
            if name in APPEND_EVENTS:
                line_key = APPEND_EVENTS[name]
                buf = self._pending.get(name)
                if buf is None:
                    buf = {"event": record, "meta": {}, "lines": []}
                    self._pending[name] = buf
                buf["event"] = record
                payload = record.get("payload") or {}
                line = payload.get(line_key)
                if line is not None:
                    buf["lines"].append(line)
                    if len(buf["lines"]) > MAX_LINES_PER_BATCH:
                        buf["lines"].pop(0)
                buf["meta"].update({k: v for k, v in payload.items() if k != line_key})
                return

            if name in LATEST_EVENTS:
                self._latest[(name, key_for(name, env))] = record
                return

            self._outbox.append(record)
            if len(self._outbox) > self._max_queue:
                self._outbox.pop(0)
                self.stats["dropped"] += 1

    # ------------------------------------------------------------------
    # Gom batch
    # ------------------------------------------------------------------

    def _collect_log_lines(self) -> dict | None:
        try:
            entries, cursor = self._log_reader(self._log_cursor)
        except Exception:
            return None
        if cursor <= self._log_cursor:
            return None
        # Ring buffer đã cắt mất phần trước cursor cũ (buffer đầy) -> client
        # phải nạp lại toàn bộ thay vì nối tiếp (payload.reset = true).
        reset = (cursor - len(entries)) > self._log_cursor
        self._log_cursor = cursor
        if not entries:
            return None
        self.stats["logLines"] += len(entries)
        # entries: [{text, level, logger, ts}] — mang luôn metadata để UI lọc
        # realtime theo mức/nguồn không phải re-parse (mục 41).
        payload: dict = {"lines": entries[-MAX_LINES_PER_BATCH:],
                         "count": len(entries)}
        if reset:
            payload["reset"] = True
        return {
            "event": ev.LOG_LINES,
            "eventVersion": 2,
            "timestamp": time.time(),
            "payload": payload,
        }

    def flush(self) -> list[dict]:
        """Lấy toàn bộ event đang chờ dưới dạng 1 batch (rỗng nếu không có gì)."""
        with self._lock:
            batch = list(self._outbox)
            self._outbox.clear()

            for name, buf in self._pending.items():
                item = dict(buf["event"])
                item["payload"] = {**buf["meta"], "lines": list(buf["lines"]), "count": len(buf["lines"])}
                batch.append(item)
            self._pending.clear()

            for _, record in self._latest.items():
                batch.append(record)
            self._latest.clear()

        log_batch = self._collect_log_lines()
        if log_batch is not None:
            batch.append(log_batch)

        if batch:
            self.stats["batches"] += 1
            self.stats["emitted"] += len(batch)
        return batch

    def pending(self) -> int:
        with self._lock:
            return (
                len(self._outbox)
                + len(self._latest)
                + len(self._pending)
            )

    def sync_log_cursor(self, cursor: int) -> None:
        """Nhảy con trỏ log tới `cursor` của client.

        Dùng khi UI vừa nạp snapshot đầy đủ (logs_recent trả cursor): các dòng
        ≤ cursor đã nằm trong snapshot, đẩy tiếp sẽ gây trùng dòng.
        """
        cursor = int(cursor)
        # Không lùi lại — client cũ hơn bridge thì giữ cursor bridge.
        if cursor > self._log_cursor:
            self._log_cursor = cursor

    # ------------------------------------------------------------------
    # Đẩy xuống WebView
    # ------------------------------------------------------------------

    @staticmethod
    def script(batch: list[dict]) -> str:
        """JS gọi hàm nhận batch ở frontend (an toàn với U+2028/U+2029)."""
        payload = json.dumps(batch, ensure_ascii=False, default=str)
        payload = payload.replace("\u2028", "\\u2028").replace("\u2029", "\\u2029")
        return f"window.__antaresEvent && window.__antaresEvent({payload});"

    def _requeue(self, batch: list[dict]) -> None:
        with self._lock:
            self._outbox[:0] = batch
            overflow = len(self._outbox) - self._max_queue
            if overflow > 0:
                del self._outbox[-overflow:]
                self.stats["dropped"] += overflow

    def push_once(self) -> int:
        """Flush + đẩy 1 lần. Trả số event đã đẩy (0 nếu không đẩy được)."""
        if not self.can_push():
            return 0
        batch = self.flush()
        if not batch:
            return 0
        try:
            self._window.evaluate_js(self.script(batch))
        except Exception as exc:                      # pragma: no cover - GUI path
            self.stats["pushFailures"] += 1
            logger.debug("evaluate_js failed: %s", exc)
            self._requeue(batch)
            return 0
        self.stats["pushed"] += len(batch)
        return len(batch)

    def can_push(self) -> bool:
        return self._ready and self._window is not None and not self._stop.is_set()

    # ------------------------------------------------------------------
    # Vòng đời
    # ------------------------------------------------------------------

    def attach_window(self, window: Any) -> None:
        """Gắn WebView + bắt đầu thread flush.

        Chỉ push khi cửa sổ đã load xong (`events.loaded`), trước đó event nằm
        lại trong outbox cho `events_drain()`.
        """
        self._window = window
        loaded = getattr(getattr(window, "events", None), "loaded", None)
        if loaded is not None:
            try:
                loaded += lambda *_, **__: self._mark_ready()
            except Exception:
                self._mark_ready()
        else:
            self._mark_ready()
        self.start()

    def _mark_ready(self) -> None:
        self._ready = True
        logger.debug("EventBridge: webview ready")

    def start(self) -> None:
        if self._thread and self._thread.is_alive():
            return
        self._stop.clear()
        self._thread = threading.Thread(target=self._loop, name="antares-events", daemon=True)
        self._thread.start()

    def _loop(self) -> None:
        while not self._stop.wait(self._interval):
            try:
                self.push_once()
            except Exception:                          # pragma: no cover
                logger.exception("EventBridge loop error")

    def stop(self) -> None:
        self._stop.set()
        thread = self._thread
        if thread and thread.is_alive() and thread is not threading.current_thread():
            thread.join(timeout=1.0)
        self._thread = None

    def close(self) -> None:
        self.stop()
        try:
            self._unsub()
        except Exception:
            pass
