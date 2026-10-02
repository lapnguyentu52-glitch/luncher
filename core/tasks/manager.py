"""Task manager — registry các task dài hạn, cancellation + progress (mục 93, 209).

Mọi thao tác download/install/launch/server phải chạy dưới một Task để UI
track, cancel và có progress events.
"""
from __future__ import annotations

import threading
import time
import uuid
from dataclasses import dataclass, field
from enum import Enum
from typing import Any, Callable


class TaskState(str, Enum):
    PENDING = "pending"
    RUNNING = "running"
    COMPLETED = "completed"
    FAILED = "failed"
    CANCELLED = "cancelled"


@dataclass
class Task:
    id: str
    type: str
    owner: str
    state: TaskState = TaskState.PENDING
    created_at: float = field(default_factory=time.time)
    started_at: float | None = None
    finished_at: float | None = None
    progress: float = 0.0  # 0..100
    message: str = ""
    result: Any = None
    error: dict | None = None
    _cancel_event: threading.Event = field(default_factory=threading.Event, repr=False)

    @property
    def cancelled(self) -> bool:
        return self._cancel_event.is_set()

    def request_cancel(self) -> None:
        self._cancel_event.set()

    def to_dict(self) -> dict:
        return {
            "id": self.id,
            "type": self.type,
            "owner": self.owner,
            "state": self.state.value,
            "progress": self.progress,
            "message": self.message,
            "error": self.error,
        }


class TaskManager:
    def __init__(self) -> None:
        self._tasks: dict[str, Task] = {}
        self._lock = threading.Lock()
        self._on_update: Callable[[Task], None] | None = None

    def set_update_listener(self, listener: Callable[[Task], None] | None) -> None:
        self._on_update = listener

    def create(self, type_: str, owner: str = "") -> Task:
        task = Task(id=uuid.uuid4().hex, type=type_, owner=owner)
        with self._lock:
            self._tasks[task.id] = task
        self._notify(task)
        return task

    def get(self, task_id: str) -> Task | None:
        return self._tasks.get(task_id)

    def list(self, *, active_only: bool = False) -> list[Task]:
        with self._lock:
            tasks = list(self._tasks.values())
        if active_only:
            tasks = [t for t in tasks if t.state in (TaskState.PENDING, TaskState.RUNNING)]
        return sorted(tasks, key=lambda t: t.created_at, reverse=True)

    def start(self, task: Task) -> None:
        task.state = TaskState.RUNNING
        task.started_at = time.time()
        self._notify(task)

    def progress(self, task: Task, value: float, message: str = "") -> None:
        task.progress = max(0.0, min(100.0, value))
        if message:
            task.message = message
        self._notify(task)

    def complete(self, task: Task, result: Any = None) -> None:
        task.state = TaskState.COMPLETED
        task.progress = 100.0
        task.finished_at = time.time()
        task.result = result
        self._notify(task)

    def fail(self, task: Task, error: dict) -> None:
        task.state = TaskState.FAILED
        task.finished_at = time.time()
        task.error = error
        self._notify(task)

    def cancel(self, task_id: str) -> bool:
        task = self._tasks.get(task_id)
        if not task:
            return False
        task.request_cancel()
        if task.state in (TaskState.PENDING, TaskState.RUNNING):
            task.state = TaskState.CANCELLED
            task.finished_at = time.time()
            self._notify(task)
        return True

    def cleanup(self, *, keep_recent: int = 50) -> None:
        with self._lock:
            finished = [t for t in self._tasks.values()
                        if t.state in (TaskState.COMPLETED, TaskState.FAILED, TaskState.CANCELLED)]
            finished.sort(key=lambda t: t.finished_at or 0)
            for t in finished[:-keep_recent]:
                self._tasks.pop(t.id, None)

    def _notify(self, task: Task) -> None:
        if self._on_update:
            try:
                self._on_update(task)
            except Exception:
                pass
