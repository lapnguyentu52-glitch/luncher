"""Path & string validation helpers (mục 24 — chống path traversal)."""
from __future__ import annotations

import re
from pathlib import Path

_INVALID_NAME_CHARS = re.compile(r'[\\/:*?"<>|]')


def is_safe_name(name: str) -> bool:
    """Tên instance/server hợp lệ: không traversal, không ký tự cấm."""
    if not name or name in (".", ".."):
        return False
    if name.endswith("."):
        return False
    if _INVALID_NAME_CHARS.search(name):
        return False
    return True


def ensure_inside(parent: Path, child: Path) -> Path:
    """Raise ValueError nếu child nằm ngoài parent — chống ../ traversal."""
    parent_r = parent.resolve()
    child_r = child.resolve()
    # is_relative_to so từng path component — startswith bị bypass bằng
    # anh em cùng tiền tố (parent/game → parent/game_evil).
    if not child_r.is_relative_to(parent_r):
        raise ValueError(f"Path escapes allowed root: {child_r}")
    return child_r


def human_size(num_bytes: float | None) -> str:
    if num_bytes is None or num_bytes < 0:
        return "Unknown"
    size = float(num_bytes)
    for unit in ("B", "KB", "MB", "GB", "TB"):
        if size < 1024:
            return f"{size:.2f} {unit}"
        size /= 1024
    return f"{size:.2f} PB"
