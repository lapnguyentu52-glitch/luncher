"""EventBus — typed, sync, thread-safe nhẹ (lock nội bộ).

UI hoặc service subscribe theo tên event. Publisher emit Envelope.
Dùng tên `"*"` để bắt mọi event (EventBridge của webview dùng wildcard này
thay vì phải liệt kê từng tên — spec mục 8.2, 15.2).
"""
from __future__ import annotations

import threading
import time
import uuid
from dataclasses import dataclass, field
from typing import Any, Callable

#: Tên đăng ký đặc biệt — nhận mọi event.
WILDCARD = "*"


@dataclass(frozen=True)
class Envelope:
    name: str
    payload: dict = field(default_factory=dict)
    event_version: int = 1
    timestamp: float = field(default_factory=time.time)
    request_id: str | None = None
    task_id: str | None = None

    def to_dict(self) -> dict:
        return {
            "event": self.name,
            "eventVersion": self.event_version,
            "timestamp": self.timestamp,
            "requestId": self.request_id,
            "taskId": self.task_id,
            "payload": self.payload,
        }


Handler = Callable[[Envelope], None]


class EventBus:
    def __init__(self) -> None:
        self._subs: dict[str, list[Handler]] = {}
        self._lock = threading.Lock()

    def subscribe(self, name: str, handler: Handler) -> Callable[[], None]:
        with self._lock:
            self._subs.setdefault(name, []).append(handler)
            return self._make_unsubscriber(name, handler)

    def _make_unsubscriber(self, name: str, handler: Handler) -> Callable[[], None]:
        def unsub() -> None:
            with self._lock:
                lst = self._subs.get(name)
                if lst and handler in lst:
                    lst.remove(handler)
        return unsub

    def subscriber_count(self, name: str) -> int:
        with self._lock:
            return len(self._subs.get(name, ()))

    def publish(self, name: str, payload: dict | None = None, *,
                request_id: str | None = None, task_id: str | None = None) -> Envelope:
        envelope = Envelope(
            name=name,
            payload=payload or {},
            request_id=request_id or str(uuid.uuid4()),
            task_id=task_id,
        )
        with self._lock:
            handlers = list(self._subs.get(name, ()))
            if name != WILDCARD:  # handler wildcard nhận mọi event
                handlers += list(self._subs.get(WILDCARD, ()))
        for h in handlers:
            try:
                h(envelope)
            except Exception:
                # handler lỗi không được giết publisher — log ở tầng setup
                pass
        return envelope

    def clear(self) -> None:
        with self._lock:
            self._subs.clear()
