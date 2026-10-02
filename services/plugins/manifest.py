"""Manifest + permissions — plugin security (spec 3.0 mục 85).

Plugin tự khai báo trong `plugin.json`:

```json
{
  "id": "com.example.tool",
  "name": "Example Tool",
  "version": "1.0.0",
  "entry": "main.py",
  "permissions": ["ui", "minecraft.telemetry"]
}
```

Mô hình quyền:
- **Deny-by-default**: không khai báo thì không có quyền nào (mục 85:
  "Không cho plugin full access mặc định").
- Quyền **scoped**: `filesystem`, `filesystem.read`, `network`, `process`,
  `minecraft`, `minecraft.telemetry`, `ui`.
- Quyền **không nhận diện được** -> manifest không hợp lệ (fail-fast, không
  đoán ý plugin tác giả).
"""
from __future__ import annotations

import json
from pathlib import Path

#: Toàn bộ quyền hợp lệ. `x` cho phép mọi scope con `x.*`.
KNOWN_PERMISSIONS = {
    "filesystem", "filesystem.read",
    "network", "process", "minecraft", "minecraft.telemetry", "ui",
}


class ManifestError(Exception):
    """plugin.json thiếu/không hợp lệ — plugin bị từ chối load."""


def load_manifest(plugin_dir: Path) -> dict:
    """Đọc + validate plugin.json. Raise ManifestError nếu sai."""
    mf_path = plugin_dir / "plugin.json"
    try:
        mf = json.loads(mf_path.read_text(encoding="utf-8"))
    except FileNotFoundError as e:
        raise ManifestError(f"thiếu plugin.json trong {plugin_dir.name}") from e
    except json.JSONDecodeError as e:
        raise ManifestError(f"plugin.json hỏng (JSON): {e}") from e

    if not isinstance(mf, dict):
        raise ManifestError("plugin.json phải là object")
    pid = mf.get("id")
    if not isinstance(pid, str) or not pid.strip():
        raise ManifestError("thiếu 'id'")
    if not is_safe_id(pid):
        raise ManifestError(f"id không hợp lệ: {pid!r} (chỉ a-z 0-9 . - _)")
    for field in ("name", "version", "entry"):
        if not isinstance(mf.get(field), str) or not mf[field].strip():
            raise ManifestError(f"thiếu/sai trường '{field}'")

    perms = mf.get("permissions", [])
    if not isinstance(perms, list) or not all(isinstance(p, str) for p in perms):
        raise ManifestError("'permissions' phải là list string")
    unknown = [p for p in perms if p not in KNOWN_PERMISSIONS]
    if unknown:
        raise ManifestError(f"quyền không nhận diện được: {', '.join(unknown)}")
    return mf


def is_safe_id(pid: str) -> bool:
    """ID dùng làm tên thư mục log/key API — chặn path traversal (mục 76)."""
    allowed = set("abcdefghijklmnopqrstuvwxyz0123456789.-_")
    return 1 <= len(pid) <= 64 and set(pid) <= allowed and pid[0] not in ".-"


def has_permission(manifest: dict, required: str) -> bool:
    """Kiểm quyền scoped: 'minecraft.telemetry' cần manifest khai báo
    'minecraft.telemetry' hoặc 'minecraft' (quyền cha)."""
    perms = set(manifest.get("permissions") or [])
    if required in perms:
        return True
    if "." in required:
        return required.split(".", 1)[0] in perms
    return False
