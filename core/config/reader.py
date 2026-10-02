"""Config reader."""
from __future__ import annotations

import json
from pathlib import Path
from typing import Any

import copy

from core.config.migration import load_and_migrate
from core.config.schema import DEFAULTS


def read_config(path: Path) -> tuple[dict[str, Any], bool]:
    """Trả (config dict, migrated?)."""
    try:
        return load_and_migrate(path)
    except Exception:
        return copy.deepcopy(DEFAULTS), False


def read_json(path: Path, default: Any = None) -> Any:
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception:
        return default
