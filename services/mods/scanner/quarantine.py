"""Quarantine — cô lập mod nguy hiểm (mục 368 safe-disable pattern).

Jar nguy hiểm được move vào cache/quarantine/ kèm manifest ghi lý do.
Không delete — user có thể restore nếu là false positive.
"""
from __future__ import annotations

import json
import shutil
import time
from pathlib import Path

from core.errors import codes
from core.errors.base import AntaresError
from core.utils.paths import is_safe_name
from infrastructure.fs.atomic import write_json_atomic


class Quarantine:
    def __init__(self, quarantine_dir: Path) -> None:
        self._dir = quarantine_dir
        self._dir.mkdir(parents=True, exist_ok=True)
        self._manifest = self._dir / "manifest.json"

    def _load_manifest(self) -> dict:
        if self._manifest.exists():
            try:
                return json.loads(self._manifest.read_text(encoding="utf-8"))
            except Exception:
                pass
        return {"items": {}}

    def _save_manifest(self, data: dict) -> None:
        write_json_atomic(self._manifest, data)

    def quarantine(self, jar_path: Path, *, instance_id: str, verdict: str,
                   score: int, findings: list[dict]) -> dict:
        """Move jar vào vault + ghi manifest. Trả entry."""
        if not jar_path.exists():
            raise AntaresError(codes.INSTANCE_NOT_FOUND, f"File not found: {jar_path.name}")

        # Tên file unique: timestamp_name
        safe_name = jar_path.name if is_safe_name(jar_path.name) else "unnamed.jar"
        dest_name = f"{int(time.time())}_{safe_name}"
        dest = self._dir / dest_name

        entry = {
            "originalName": jar_path.name,
            "originalPath": str(jar_path),
            "instanceId": instance_id,
            "verdict": verdict,
            "score": score,
            "findings": findings,
            "quarantinedAt": time.time(),
            "quarantineFile": dest_name,
        }

        shutil.move(str(jar_path), str(dest))
        manifest = self._load_manifest()
        manifest["items"][dest_name] = entry
        self._save_manifest(manifest)
        return entry

    def restore(self, quarantine_file: str) -> dict:
        """Restore mod về chỗ cũ (false positive)."""
        manifest = self._load_manifest()
        entry = manifest["items"].get(quarantine_file)
        if not entry:
            raise AntaresError(codes.INSTANCE_NOT_FOUND,
                               f"Quarantine entry not found: {quarantine_file}")
        src = self._dir / quarantine_file
        if not src.exists():
            raise AntaresError(codes.INSTANCE_NOT_FOUND,
                               f"Quarantined file missing: {quarantine_file}")
        original = Path(entry["originalPath"])
        original.parent.mkdir(parents=True, exist_ok=True)
        shutil.move(str(src), str(original))
        del manifest["items"][quarantine_file]
        self._save_manifest(manifest)
        return entry

    def delete(self, quarantine_file: str) -> bool:
        """Xoá vĩnh viễn mod nguy hiểm."""
        manifest = self._load_manifest()
        if quarantine_file not in manifest["items"]:
            return False
        (self._dir / quarantine_file).unlink(missing_ok=True)
        del manifest["items"][quarantine_file]
        self._save_manifest(manifest)
        return True

    def list(self) -> list[dict]:
        manifest = self._load_manifest()
        out = []
        for name, entry in manifest.get("items", {}).items():
            item = dict(entry)
            item["exists"] = (self._dir / name).exists()
            out.append(item)
        return sorted(out, key=lambda x: x.get("quarantinedAt", 0), reverse=True)

    def get(self, quarantine_file: str) -> dict | None:
        return self._load_manifest()["items"].get(quarantine_file)
