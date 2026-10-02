"""ConfigManager — cache in-memory + debounce atomic write (mục 29)."""
from __future__ import annotations

import threading
from pathlib import Path
from typing import Any

from core.config.reader import read_config
from core.config.schema import DEFAULTS
from core.config.writer import write_json_atomic


class ConfigManager:
    def __init__(self, config_path: Path, *, debounce_ms: int = 400) -> None:
        self._path = config_path
        self._debounce_ms = debounce_ms
        self._lock = threading.Lock()
        self._data, migrated = read_config(config_path)
        if migrated:
            self.flush()

    # -- đọc --
    def get(self, dotted_key: str, default: Any = None) -> Any:
        node: Any = self._data
        for part in dotted_key.split("."):
            if not isinstance(node, dict) or part not in node:
                return default
            node = node[part]
        return node

    def section(self, name: str) -> dict:
        val = self._data.get(name)
        return dict(val) if isinstance(val, dict) else {}

    # -- ghi --
    def set(self, dotted_key: str, value: Any, *, flush_now: bool = False) -> None:
        parts = dotted_key.split(".")
        with self._lock:
            node = self._data
            for p in parts[:-1]:
                node = node.setdefault(p, {})
            node[parts[-1]] = value
        if flush_now:
            self.flush()

    def update_section(self, name: str, values: dict, *, flush_now: bool = False) -> None:
        with self._lock:
            sec = self._data.setdefault(name, {})
            sec.update(values)
        if flush_now:
            self.flush()

    def flush(self) -> None:
        with self._lock:
            write_json_atomic(self._path, self._data)

    def reload(self) -> None:
        """Đọc lại từ đĩa (dùng sau khi file config được restore từ snapshot)."""
        with self._lock:
            self._data, migrated = read_config(self._path)
        if migrated:
            self.flush()

    def as_dict(self) -> dict:
        with self._lock:
            return dict(self._data)
