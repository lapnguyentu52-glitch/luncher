"""Atomic config writer — write temp -> fsync -> replace (mục 91)."""
from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Any


def write_json_atomic(path: Path, data: Any, *, fsync: bool = True) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(data, f, indent=2, ensure_ascii=False)
        if fsync:
            f.flush()
            os.fsync(f.fileno())
    os.replace(tmp, path)


def write_text_atomic(path: Path, text: str, *, fsync: bool = True) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    tmp = path.with_suffix(path.suffix + ".tmp")
    with open(tmp, "w", encoding="utf-8") as f:
        f.write(text)
        if fsync:
            f.flush()
            os.fsync(f.fileno())
    os.replace(tmp, path)
