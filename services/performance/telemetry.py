"""Performance telemetry — sampler CPU/RAM/disk (spec 3.0 mục 4.2, 5, 89).

Thiết kế:
- Sampler chạy thread nền, nhịp bình thường 2s, nhịp game-mode 5s (mục 30:
  khi game chạy, launcher phải nhường CPU — giảm tần số, không tăng tải).
- Dùng psutil (đã có trong requirements) — không thêm dependency mới.
- CPU percent dùng interval=None (so với lần mẫu trước) để không block thread.
- Push qua EventBus dạng `performance.telemetry` — EventBridge đã coalesce
  `latest` cho event này (chỉ giữ bản mới nhất), UI không bị spam.
- Lịch sử giữ trong RAM, giới hạn điểm (ring). Không ghi đĩa (mục 62:
  telemetry retention — realtime memory only).
"""
from __future__ import annotations

import threading
import time

from core.events import names as ev
from core.logging.setup import get_logger
from core.scheduler.policy import GameModeGovernor

logger = get_logger("performance.telemetry")

NORMAL_INTERVAL = 2.0     # giây (mục 5.2: hardware 1–2 sec)
GAME_INTERVAL = 5.0       # giây — nhường CPU khi Minecraft đang chạy
MAX_POINTS = 900          # ~30 phút ở nhịp 2s, ring trong RAM


class SystemTelemetryService:
    """Sampler hệ thống + lịch sử ngắn cho Performance Center."""

    def __init__(self, governor: GameModeGovernor, *,
                 interval: float = NORMAL_INTERVAL,
                 enabled: bool = True) -> None:
        self._gov = governor
        self._interval = interval
        self._enabled = enabled
        self._lock = threading.Lock()
        self._history: list[dict] = []
        self._latest: dict | None = None
        self._io_prev: dict | None = None   # bộ đếm raw disk I/O lần mẫu trước
        self._stop = threading.Event()
        self._thread: threading.Thread | None = None

    # ------------------------------------------------------------------
    # Vòng đời
    # ------------------------------------------------------------------

    def start(self) -> None:
        if self._thread and self._thread.is_alive():
            return
        self._stop.clear()
        self._thread = threading.Thread(target=self._loop,
                                        name="antares-telemetry", daemon=True)
        self._thread.start()
        logger.info("Telemetry sampler started (%.1fs)", self._interval)

    def stop(self) -> None:
        self._stop.set()
        thread = self._thread
        if thread and thread.is_alive() and thread is not threading.current_thread():
            thread.join(timeout=2.0)
        self._thread = None

    def set_enabled(self, enabled: bool) -> None:
        self._enabled = bool(enabled)

    # ------------------------------------------------------------------
    # Vòng lặp lấy mẫu
    # ------------------------------------------------------------------

    def _loop(self) -> None:
        # Lần gọi đầu cpu_percent(None) trả 0.0 — làm mẫu "mồi" rồi mới tính.
        try:
            import psutil
            psutil.cpu_percent(None)
        except Exception:
            logger.exception("psutil unavailable — telemetry disabled")
            return

        # Nhịp dọc theo mốc thời gian: nếu sample tốn thời gian, lần ngủ tiếp
        # vẫn bám lịch (tránh trôi khi sample chậm — event-based, không busy-wait).
        next_t = time.monotonic()
        while not self._stop.wait(max(0.0, next_t - time.monotonic())):
            next_t += self._current_interval()
            if next_t < time.monotonic():
                next_t = time.monotonic() + self._current_interval()
            if not self._enabled:
                continue
            try:
                sample = self._sample()
            except Exception:
                logger.exception("telemetry sample failed")
                continue
            if sample is None:
                continue
            with self._lock:
                self._latest = sample
                self._history.append(sample)
                if len(self._history) > MAX_POINTS:
                    del self._history[:len(self._history) - MAX_POINTS]
            # publish ngoài lock — EventBridge tự coalesce latest
            self._publish(sample)

    def _current_interval(self) -> float:
        return GAME_INTERVAL if self._gov.active else self._interval

    # ------------------------------------------------------------------
    # Mẫu dữ liệu
    # ------------------------------------------------------------------

    def _sample(self) -> dict | None:
        import psutil

        vm = psutil.virtual_memory()
        proc = psutil.Process()  # chính launcher

        sample: dict = {
            "ts": time.time(),
            "cpu": round(psutil.cpu_percent(None), 1),
            "ram": {
                "used": vm.used, "total": vm.total,
                "percent": round(vm.percent, 1),
            },
            "launcher": self._proc_info(proc),
        }

        # Disk I/O (giá trị tích luỹ -> tính tốc độ B/s giữa 2 mẫu).
        # Lưu bộ đếm raw riêng — lịch sử/public chỉ giữ giá trị đã tính.
        try:
            io = psutil.disk_io_counters()
            if io is not None:
                now = time.time()
                prev = self._io_prev
                if prev and now > prev["ts"]:
                    dt = now - prev["ts"]
                    read_bps = max(0, (io.read_bytes - prev["read"]) / dt)
                    write_bps = max(0, (io.write_bytes - prev["write"]) / dt)
                    sample["disk"] = {"readBps": int(read_bps), "writeBps": int(write_bps)}
                self._io_prev = {"ts": now, "read": io.read_bytes, "write": io.write_bytes}
        except Exception:
            pass  # một số hệ không có disk_io_counters

        # GPU: psutil không có — bỏ qua (companion phase sau, mục 12).
        return sample

    def _publish(self, sample: dict) -> None:
        """Đẩy mẫu mới qua EventBus (EventBridge coalesce `latest` — không spam UI)."""
        try:
            self._gov.bus.publish(ev.PERFORMANCE_TELEMETRY, sample)
        except Exception:
            pass

    @staticmethod
    def _proc_info(proc) -> dict:
        """Thông tin chính launcher (mục 29 KPI: launcher idle CPU/RAM)."""
        try:
            with proc.oneshot():
                mem = proc.memory_info()
                return {
                    "cpu": round(proc.cpu_percent(None), 1),
                    "rss": mem.rss,
                }
        except Exception:
            return {"cpu": 0.0, "rss": 0}

    # ------------------------------------------------------------------
    # API cho bridge
    # ------------------------------------------------------------------

    def snapshot(self) -> dict:
        with self._lock:
            latest = dict(self._latest) if self._latest else None
            history = list(self._history)
        return {"latest": latest, "history": history, "gameMode": self._gov.active}
