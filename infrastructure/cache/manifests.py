"""Manifest/metadata cache — TTL-based disk cache (mục 10.2, 30, 298)."""
from __future__ import annotations

import json
import time
from pathlib import Path
from typing import Any

from core.logging.setup import get_logger

logger = get_logger("cache")

TTL_MANIFEST = 30 * 60        # 5-30 min
TTL_LOADER = 30 * 60          # 10-60 min
TTL_SEARCH = 5 * 60           # 1-10 min
TTL_STATIC = 12 * 3600        # 6-24h


class DiskCache:
    """Cache JSON với TTL, graceful khi offline (dùng stale data — mục 60)."""

    def __init__(self, cache_dir: Path) -> None:
        self._dir = cache_dir / "manifests"
        self._dir.mkdir(parents=True, exist_ok=True)

    def _path(self, key: str) -> Path:
        safe = key.replace("/", "_").replace(":", "_")
        return self._dir / f"{safe}.json"

    def get(self, key: str, ttl: int = TTL_MANIFEST, *, allow_stale: bool = False) -> Any | None:
        path = self._path(key)
        if not path.exists():
            return None
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
        except Exception:
            return None
        age = time.time() - data.get("ts", 0)
        if age <= ttl or allow_stale:
            if age > ttl:
                logger.debug("Using STALE cache: %s", key)
            return data.get("value")
        return None

    def put(self, key: str, value: Any) -> None:
        path = self._path(key)
        try:
            path.write_text(json.dumps({"ts": time.time(), "value": value}),
                            encoding="utf-8")
        except Exception:
            logger.warning("Cache write failed: %s", key)

    def clear(self) -> int:
        count = 0
        for f in self._dir.glob("*.json"):
            f.unlink(missing_ok=True)
            count += 1
        return count
