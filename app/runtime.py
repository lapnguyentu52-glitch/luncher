"""Runtime — quản lý vòng đời start/run/stop (spec 126, 42)."""
from __future__ import annotations

import threading

from app.context import AppContext
from app.version import APP_NAME
from core.events import names as ev
from core.logging.setup import get_logger

logger = get_logger("runtime")


class AppRuntime:
    """Điều phối lifecycle: BOOTING -> READY -> SHUTTING_DOWN -> CLOSED."""

    def __init__(self, ctx: AppContext) -> None:
        self.ctx = ctx
        self._shutdown_requested = threading.Event()

    @property
    def shutting_down(self) -> bool:
        return self._shutdown_requested.is_set()

    def start_background(self) -> None:
        """Khởi tạo nền sau khi UI paint — không network nặng (spec 6, 39)."""
        # Companion IPC (mục 12-13): loopback server nhận telemetry từ game.
        try:
            runtime = self.ctx.get("runtime")
            if runtime:
                runtime.start()
        except Exception:
            logger.exception("IPC start failed (companion telemetry sẽ không có)")
        self.ctx.events.publish(ev.APP_READY, {"app": APP_NAME})

    def request_shutdown(self) -> None:
        """Shutdown sequence (spec 42, 114): cancel tasks -> flush -> events."""
        if self.shutting_down:
            return
        self._shutdown_requested.set()
        logger.info("Shutdown requested")
        self.ctx.events.publish(ev.APP_SHUTDOWN, {})
        for task in self.ctx.tasks.list(active_only=True):
            self.ctx.tasks.cancel(task.id)
        # Stop IPC trước khi flush (mục 68: stop runtime IPC trước background workers)
        try:
            runtime = self.ctx.get("runtime")
            if runtime:
                runtime.stop()
        except Exception:
            pass
        self.ctx.config.flush()
        logger.info("Shutdown complete")

    def run_headless(self) -> None:
        """Chạy không GUI (dùng cho test/smoke)."""
        self.start_background()
        try:
            while not self._shutdown_requested.wait(timeout=0.1):
                pass
        except KeyboardInterrupt:
            pass
        finally:
            self.request_shutdown()
