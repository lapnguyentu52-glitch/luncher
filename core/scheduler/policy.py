"""GameMode governor — coalesce policy cho việc nền khi game đang chạy (mục 30, 56).

Nguyên tắc:
- `minecraft.started`  -> BẬT game mode: giảm tần số sampler, trì hoãn công việc IDLE.
- `minecraft.exited`   -> TẮT game mode: trả lại nhịp bình thường + xử lý các việc bị dồn.
- `minecraft.exited` có exitCode != 0 vẫn tắt game mode (crash analyzer chạy riêng).

Dùng bởi telemetry sampler và bất kỳ background worker nào muốn tuân thủ
"Low launcher overhead while gaming".
"""
from __future__ import annotations

import threading

from core.events import names as ev
from core.events.bus import EventBus
from core.logging.setup import get_logger

logger = get_logger("scheduler.policy")


class GameModeGovernor:
    """Trạng thái game-mode dùng chung + đăng ký nghe event launch/exit."""

    def __init__(self, bus: EventBus, *, process_priority_hook=None) -> None:
        self.bus = bus  # public: service khác dùng để publish event có policy-aware
        self._lock = threading.Lock()
        self._active = False
        self._instance_id: str | None = None
        self._priority_hook = process_priority_hook  # callable(instance_id) — mục 36
        self._unsubs = [
            bus.subscribe(ev.MINECRAFT_STARTED, self._on_started),
            bus.subscribe(ev.MINECRAFT_EXITED, self._on_exited),
        ]

    # ------------------------------------------------------------------
    # Truy vấn
    # ------------------------------------------------------------------

    @property
    def active(self) -> bool:
        with self._lock:
            return self._active

    @property
    def instance_id(self) -> str | None:
        with self._lock:
            return self._instance_id

    # ------------------------------------------------------------------
    # Event handlers
    # ------------------------------------------------------------------

    def _on_started(self, env) -> None:
        with self._lock:
            self._active = True
            self._instance_id = (env.payload or {}).get("instanceId")
        logger.info("Game mode ON (instance=%s)", self._instance_id)
        # Mục 36: process priority theo config (default Normal = không làm gì)
        if self._priority_hook and self._instance_id:
            try:
                self._priority_hook(self._instance_id)
            except Exception:
                logger.exception("priority hook failed")

    def _on_exited(self, env) -> None:
        with self._lock:
            self._active = False
            self._instance_id = None
        logger.info("Game mode OFF")

    def close(self) -> None:
        for unsub in self._unsubs:
            try:
                unsub()
            except Exception:
                pass
