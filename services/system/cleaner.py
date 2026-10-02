"""Disk cleanup — Safe-first, có Undo (spec 3.0 mục 4.3 Disk, 37).

Nguyên tắc mục 37:
- Chỉ quét/đụng thư mục **launcher sở hữu** (cache, logs, downloads .part).
- `saves/screenshots/mods/resourcepacks/accounts` = PROTECTED — chỉ hiển thị
  thông tin kích thước, KHÔNG có action xoá.
- Clean KHÔNG delete vĩnh viễn: move vào `data/trash/<cleanId>/` + manifest ->
  Undo 1 click; xoá hẳn là action riêng có confirm (UI).
"""
from __future__ import annotations

import shutil
import time
import uuid
from pathlib import Path

from app.context import AppContext
from core.errors import codes
from core.errors.base import AntaresError
from core.logging.setup import get_logger

logger = get_logger("system.cleaner")

#: Rule quét "Safe" — chỉ file launcher-owned quá hạn hoặc phầnDownload dở.
SAFE_RULES = (
    {"id": "download_parts", "root": "cache/downloads", "glob": "**/*.part",
     "maxAgeDays": 1, "labelKey": "downloadParts"},
    {"id": "old_logs", "root": "logs", "glob": "**/*",
     "maxAgeDays": 7, "labelKey": "oldLogs"},
    {"id": "old_snapshots", "root": "instances", "glob": "*/optimization-snapshots/opt-*.json",
     "maxAgeDays": 30, "labelKey": "oldSnapshots"},
    {"id": "trash", "root": "trash", "glob": "*/*",
     "maxAgeDays": 7, "labelKey": "trash"},
)

#: Chỉ hiển thị kích thước — không bao giờ xoá (mục 37).
PROTECTED_DIRS = (
    {"id": "saves", "path": "instances/*/game/saves", "labelKey": "saves"},
    {"id": "screenshots", "path": "instances/*/game/screenshots", "labelKey": "screenshots"},
    {"id": "mods", "path": "instances/*/game/mods", "labelKey": "mods"},
    {"id": "resourcepacks", "path": "instances/*/game/resourcepacks", "labelKey": "resourcepacks"},
    {"id": "accounts", "path": "accounts", "labelKey": "accounts"},
)

DU_DIR = Path("data") / "trash"


class DiskCleaner:
    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx

    # ------------------------------------------------------------------
    # Scan
    # ------------------------------------------------------------------

    def scan(self) -> dict:
        safe = []
        for rule in SAFE_RULES:
            items, total = self._collect(self._data() / rule["root"], rule["glob"],
                                         rule["maxAgeDays"])
            if rule["id"] == "old_snapshots":
                items, total = [], 0
                base = self._data() / "instances"
                for p in base.glob(rule["glob"]) if base.exists() else ():
                    if self._age_days(p) >= rule["maxAgeDays"]:
                        items.append(self._item(p))
                        total += p.stat().st_size if p.is_file() else 0
            safe.append({
                "id": rule["id"], "labelKey": rule["labelKey"],
                "count": len(items), "bytes": total, "items": items,
                "risk": "LOW", "reversible": True,
            })
        return {
            "safe": safe,
            "protected": self._protected(),
            "totalCleanableBytes": sum(g["bytes"] for g in safe),
        }

    def _collect(self, root: Path, pattern: str, max_age_days: int):
        items, total = [], 0
        for p in root.glob(pattern) if root.exists() else ():
            if not p.is_file() or p.name == "manifest.json":
                continue
            if self._age_days(p) < max_age_days:
                continue
            items.append(self._item(p))
            total += p.stat().st_size
        return items, total

    @staticmethod
    def _item(p: Path) -> dict:
        return {"path": str(p), "bytes": p.stat().st_size,
                "ageDays": round(DiskCleaner._age_days(p), 1)}

    @staticmethod
    def _age_days(p: Path) -> float:
        try:
            return max(0.0, (time.time() - p.stat().st_mtime) / 86400)
        except OSError:
            return 0.0

    def _protected(self) -> list[dict]:
        out = []
        data = self._data()
        for rule in PROTECTED_DIRS:
            base = data / rule["path"].split("/*")[0]
            total = 0
            for p in data.glob(rule["path"]) if data.exists() else ():
                if p.is_dir():
                    for f in p.rglob("*"):
                        if f.is_file():
                            total += f.stat().st_size
                elif p.is_file():
                    total += p.stat().st_size
            out.append({"id": rule["id"], "labelKey": rule["labelKey"],
                        "bytes": total if base.exists() else 0})
        return out

    # ------------------------------------------------------------------
    # Clean (move to trash) + Undo + Empty
    # ------------------------------------------------------------------

    def clean(self, paths: list[str]) -> dict:
        """Move các file user chọn vào trash + manifest (mục 37: không xoá silent)."""
        clean_id = uuid.uuid4().hex[:12]
        trash = self._data() / "trash" / clean_id
        trash.mkdir(parents=True, exist_ok=True)
        moved, freed = [], 0
        for raw in paths:
            src = Path(raw)
            if not self._owned(src):
                raise AntaresError(codes.VALIDATION_FAILED,
                                   f"Refusing to clean outside launcher data: {src}")
            if not src.is_file():
                continue
            size = src.stat().st_size   # chụp TRƯỚC khi move (src mất sau move)
            dest = trash / uuid.uuid4().hex[:8]
            shutil.move(str(src), dest)
            moved.append({"src": str(src), "dest": str(dest)})
            freed += size

        from core.config.writer import write_json_atomic
        write_json_atomic(trash / "manifest.json", {
            "id": clean_id, "ts": time.time(),
            "items": moved, "bytes": freed,
        })
        logger.info("Cleanup %s: %d files, %.1f KB", clean_id, len(moved), freed / 1024)
        return {"cleanId": clean_id, "moved": len(moved), "bytes": freed}

    def undo(self, clean_id: str) -> dict:
        trash = self._data() / "trash" / clean_id
        manifest = trash / "manifest.json"
        if not manifest.is_file():
            raise AntaresError(codes.FILE_NOT_FOUND, "Cleanup manifest not found")
        from core.config.reader import read_json
        items = (read_json(manifest) or {}).get("items", [])
        restored = 0
        for it in items:
            src, dest = Path(it["dest"]), Path(it["src"])
            if src.is_file():
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.move(str(src), str(dest))
                restored += 1
        shutil.rmtree(trash, ignore_errors=True)
        return {"restored": restored}

    def empty_trash(self) -> int:
        trash = self._data() / "trash"
        n = 0
        for d in trash.iterdir() if trash.exists() else ():
            if d.is_dir():
                shutil.rmtree(d, ignore_errors=True)
                n += 1
        return n

    # ------------------------------------------------------------------

    def _data(self) -> Path:
        return Path(self._ctx.paths.data)

    def _owned(self, p: Path) -> bool:
        """File phải nằm trong data dir của launcher (chống path escape — mục 76)."""
        try:
            p.resolve().relative_to(self._data().resolve())
            return True
        except (ValueError, OSError):
            return False
