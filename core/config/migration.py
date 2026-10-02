"""Config migration: schema cũ -> mới, deterministic, có backup.

Phase 3 của spec: parse settings.json của Spark, map fields, KHÔNG sửa file cũ.
"""
from __future__ import annotations

import json
import shutil
from pathlib import Path
from typing import Any

from core.config.schema import DEFAULTS, SCHEMA_VERSION


def _deep_merge(base: dict, extra: dict) -> dict:
    """Deep merge KHÔNG mutate base — merge vào bản copy sâu.

    Bug đã fix: `dict(base)` chỉ shallow copy; nested dict (vd ui.*) vẫn là
    reference tới DEFAULTS → mọi ConfigManager.set() ô nhiễm module state
    xuyên suốt các instance (thấy khi chạy test tuần tự).
    """
    import copy
    out = copy.deepcopy(base)
    for k, v in extra.items():
        if isinstance(v, dict) and isinstance(out.get(k), dict):
            out[k] = _deep_merge(out[k], v)
        else:
            out[k] = v
    return out


def migrate(data: dict[str, Any]) -> dict[str, Any]:
    """Áp các bước migration tuần tự. Hiện chỉ có 0 -> 3 (từ Spark)."""
    version = data.get("schemaVersion", 0)
    if version >= SCHEMA_VERSION:
        return data
    if version < 3:
        data = _migrate_from_spark(data)
    data["schemaVersion"] = SCHEMA_VERSION
    return data


def _migrate_from_spark(data: dict[str, Any]) -> dict[str, Any]:
    """Map fields từ settings.json của Spark launcher."""
    out = _deep_merge(DEFAULTS, {})

    mc_home = data.get("Minecraft-home")
    if mc_home:
        out["app"]["gameDirectory"] = mc_home

    out["performance"]["defaultMaxMb"] = int(data.get("allocated_ram") or 2048)

    jvm = data.get("jvm-args")
    if jvm:
        out["performance"]["customJvmArgs"] = str(jvm).split()

    accounts = []
    for acc in data.get("User-info") or []:
        if not (isinstance(acc, dict) and acc.get("username")):
            continue
        accounts.append({
            "id": acc.get("UUID") or acc["username"],
            "type": "ely" if acc.get("AUTH_TYPE") == "ely_by login" else "offline",
            "displayName": acc["username"],
            "minecraftUuid": acc.get("UUID"),
            "lastUsed": None,
        })
    out["accounts"] = accounts

    last_type = data.get("last_game_type")
    if last_type:
        out["performance"]["lastGameType"] = str(last_type).lower()
    out["selectedAccount"] = data.get("selected_account")
    return out


def load_and_migrate(path: Path) -> tuple[dict[str, Any], bool]:
    """Đọc file config; nếu là schema cũ thì migrate + backup. Trả (data, migrated)."""
    if not path.exists():
        return _deep_merge(DEFAULTS, {}), False
    raw = json.loads(path.read_text(encoding="utf-8"))
    if raw.get("schemaVersion", 0) >= SCHEMA_VERSION:
        return _deep_merge(DEFAULTS, raw), False
    backup = path.with_suffix(path.suffix + ".bak")
    if not backup.exists():
        shutil.copy2(path, backup)
    migrated = migrate(raw)
    return migrated, True
