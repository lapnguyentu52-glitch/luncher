"""Process optimization — priority/affinity cho process launcher spawn (mục 36).

Ràng buộc an toàn:
- CHỈ đụng process do launcher spawn (Minecraft/server qua ProcessManager) —
  không bao giờ process ngoài (mục 35: "identify competing heavy processes
  only as information, not forced-kill").
- Priority: Normal (default) / Above Normal / High. KHÔNG realtime (mục 36).
- Affinity: Auto (không set) hoặc mask tùy chọn; có reset.
"""
from __future__ import annotations

import os

from infrastructure.process.manager import ProcessManager
from core.logging.setup import get_logger

logger = get_logger("system.process")

#: Đúng mục 36: không realtime.
ALLOWED_PRIORITY = {"normal", "above_normal", "high"}

_PS_PRIORITY = {
    "normal":   0x00000020,   # NORMAL_PRIORITY_CLASS
    "above_normal": 0x00008000,   # ABOVE_NORMAL_PRIORITY_CLASS
    "high":     0x00000080,   # HIGH_PRIORITY_CLASS
}


class ProcessService:
    def __init__(self, processes: ProcessManager) -> None:
        self._processes = processes

    def apply_priority(self, key: str, level: str) -> bool:
        """Đặt priority cho process đang chạy (vd key='minecraft:<id>')."""
        level = (level or "normal").lower()
        if level not in ALLOWED_PRIORITY:
            return False
        mp = self._processes.get(key)
        if not mp or not mp.is_running() or mp._proc is None:
            return False
        try:
            import psutil
            proc = psutil.Process(mp.pid)
            if os.name == "nt":
                proc.nice(_PS_PRIORITY[level])
            else:
                proc.nice({"normal": 0, "above_normal": -5, "high": -10}[level])
            logger.info("Priority %s -> %s", key, level)
            return True
        except Exception as e:
            logger.warning("apply_priority(%s, %s) failed: %s", key, level, e)
            return False

    def apply_affinity(self, key: str, cores: list[int] | None) -> bool:
        """Set CPU affinity. `cores=None`/[] = reset về Auto (tất cả core)."""
        mp = self._processes.get(key)
        if not mp or not mp.is_running() or mp.pid is None:
            return False
        try:
            import psutil
            proc = psutil.Process(mp.pid)
            if not cores:
                proc.cpu_affinity(list(range(os.cpu_count() or 1)))
            else:
                n = os.cpu_count() or 1
                valid = [c for c in cores if 0 <= c < n]
                if not valid:
                    return False
                proc.cpu_affinity(valid)
            logger.info("Affinity %s -> %s", key, cores or "auto")
            return True
        except Exception as e:
            # Windows: cpu_affinity dùng win32 API qua psutil — hoạt động ổn;
            # một số hệ cần quyền đặc biệt -> log + trả False, không crash.
            logger.warning("apply_affinity(%s) failed: %s", key, e)
            return False

    @staticmethod
    def info(pid: int) -> dict | None:
        """Thông tin 1 process launcher-owned (informational — mục 4.3 Memory)."""
        try:
            import psutil
            proc = psutil.Process(pid)
            with proc.oneshot():
                return {
                    "pid": pid,
                    "name": proc.name(),
                    "cpu": proc.cpu_percent(None),
                    "rss": proc.memory_info().rss,
                    "nice": proc.nice(),
                    "affinity": len(proc.cpu_affinity()),
                }
        except Exception:
            return None
