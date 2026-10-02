"""Resource locks — instance/server lock file với PID (mục 446-449).

Lock order: InstanceLock -> artifact locks. Không đảo chiều.
"""
from __future__ import annotations

import json
import os
import time
from pathlib import Path

from core.errors.base import AntaresError
from core.errors import codes


class LockError(AntaresError):
    def __init__(self, message: str) -> None:
        super().__init__(codes.INSTANCE_LOCKED, message)


class ResourceLock:
    """Lock file chứa pid+timestamp. Stale lock = pid không còn sống."""

    def __init__(self, path: Path) -> None:
        self.path = path
        self._acquired = False

    def acquire(self, *, steal_stale: bool = True) -> bool:
        self.path.parent.mkdir(parents=True, exist_ok=True)
        if self.path.exists():
            if steal_stale and self._is_stale():
                self.path.unlink(missing_ok=True)
            else:
                return False
        payload = {"pid": os.getpid(), "timestamp": time.time()}
        self.path.write_text(json.dumps(payload), encoding="utf-8")
        self._acquired = True
        return True

    def release(self) -> None:
        if self._acquired:
            self.path.unlink(missing_ok=True)
            self._acquired = False

    def _is_stale(self) -> bool:
        try:
            data = json.loads(self.path.read_text(encoding="utf-8"))
            pid = int(data.get("pid", 0))
        except Exception:
            return True  # hỏng định dạng -> coi là stale
        if pid == os.getpid():
            return False
        try:
            os.kill(pid, 0)
            return False  # pid còn sống
        except ProcessLookupError:
            return True
        except PermissionError:
            return False

    def __enter__(self) -> "ResourceLock":
        if not self.acquire():
            raise LockError(f"Resource is locked: {self.path}")
        return self

    def __exit__(self, *exc) -> None:
        self.release()
