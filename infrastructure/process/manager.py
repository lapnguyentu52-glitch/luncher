"""Process manager — spawn, stream output, graceful stop (mục 12, 287, 444).

Không console window trên Windows; capture stdout/stderr; UTF-8 fallback.
"""
from __future__ import annotations

import subprocess
import threading
import time
from dataclasses import dataclass, field
from enum import Enum
from typing import Callable

from core.logging.setup import get_logger

logger = get_logger("process")


class ProcessState(str, Enum):
    CREATED = "created"
    STARTING = "starting"
    RUNNING = "running"
    STOPPING = "stopping"
    EXITED = "exited"
    CRASHED = "crashed"
    KILLED = "killed"


@dataclass
class ManagedProcess:
    cmd: list[str]
    cwd: str | None
    state: ProcessState = ProcessState.CREATED
    pid: int | None = None
    exit_code: int | None = None
    started_at: float | None = None
    _proc: subprocess.Popen | None = field(default=None, repr=False)

    def is_running(self) -> bool:
        return self._proc is not None and self._proc.poll() is None


class ProcessManager:
    """Quản lý process do launcher spawn — không đụng process ngoài (mục 357)."""

    def __init__(self) -> None:
        self._processes: dict[str, ManagedProcess] = {}

    def spawn(self, key: str, cmd: list[str], *, cwd: str | None = None,
              on_line: Callable[[str], None] | None = None,
              on_exit: Callable[[int], None] | None = None) -> ManagedProcess:
        if key in self._processes and self._processes[key].is_running():
            raise RuntimeError(f"Process already running: {key}")

        creationflags = getattr(subprocess, "CREATE_NO_WINDOW", 0) if _is_windows() else 0
        mp = ManagedProcess(cmd=cmd, cwd=cwd, state=ProcessState.STARTING)
        try:
            mp._proc = subprocess.Popen(
                cmd, cwd=cwd,
                stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                text=True, errors="replace", bufsize=1,
                creationflags=creationflags,
            )
        except FileNotFoundError as e:
            mp.state = ProcessState.CRASHED
            raise RuntimeError(f"Executable not found: {cmd[0]}") from e
        mp.pid = mp._proc.pid
        mp.state = ProcessState.RUNNING
        mp.started_at = time.time()
        self._processes[key] = mp

        threading.Thread(target=self._pump, args=(mp, on_line, on_exit),
                         daemon=True).start()
        return mp

    def _pump(self, mp: ManagedProcess,
              on_line: Callable[[str], None] | None,
              on_exit: Callable[[int], None] | None) -> None:
        proc = mp._proc
        assert proc is not None and proc.stdout is not None
        for line in proc.stdout:
            line = line.rstrip("\r\n")
            if on_line:
                try:
                    on_line(line)
                except Exception:
                    logger.exception("on_line handler failed")
        code = proc.wait()
        mp.exit_code = code
        running_before = mp.state == ProcessState.RUNNING
        mp.state = ProcessState.EXITED if code == 0 else ProcessState.CRASHED
        if not running_before and code != 0:
            mp.state = ProcessState.KILLED
        if on_exit:
            try:
                on_exit(code)
            except Exception:
                logger.exception("on_exit handler failed")

    def stop(self, key: str, *, graceful_stdin: str | None = None,
             timeout: float = 10.0) -> None:
        mp = self._processes.get(key)
        if not mp or not mp.is_running():
            return
        proc = mp._proc
        assert proc is not None
        mp.state = ProcessState.STOPPING
        if graceful_stdin and proc.stdin:
            try:
                proc.stdin.write(graceful_stdin + "\n")
                proc.stdin.flush()
            except Exception:
                pass
        try:
            proc.wait(timeout=timeout)
            return
        except subprocess.TimeoutExpired:
            pass
        try:
            proc.kill()
            mp.state = ProcessState.KILLED
        except Exception:
            pass

    def get(self, key: str) -> ManagedProcess | None:
        return self._processes.get(key)

    def is_running(self, key: str) -> bool:
        mp = self._processes.get(key)
        return bool(mp and mp.is_running())


def _is_windows() -> bool:
    import os
    return os.name == "nt"
