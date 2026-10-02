"""InstanceService — create/list/get/delete instances (mục 17, 273).

Instance = runtime environment: metadata (instance.json) + game dir.
"""
from __future__ import annotations

import shutil
import uuid
from pathlib import Path

from app.context import AppContext
from core.errors import codes
from core.errors.base import InstanceError
from core.utils.paths import is_safe_name


class InstanceService:
    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx

    def _root(self, instance_id: str) -> Path:
        return self._ctx.paths.instances / instance_id

    def list(self) -> list[dict]:
        out = []
        for p in self._ctx.paths.instances.iterdir() if self._ctx.paths.instances.exists() else ():
            meta = p / "instance.json"
            if p.is_dir() and meta.exists():
                import json
                try:
                    data = json.loads(meta.read_text(encoding="utf-8"))
                    out.append(data)
                except Exception:
                    pass
        return out

    def get(self, instance_id: str) -> dict | None:
        import json
        meta = self._root(instance_id) / "instance.json"
        if not meta.exists():
            return None
        try:
            return json.loads(meta.read_text(encoding="utf-8"))
        except Exception:
            return None

    def create(self, name: str, minecraft_version: str, *,
               loader: str = "vanilla", memory_max_mb: int = 2048,
               memory_min_mb: int = 512) -> dict:
        if not is_safe_name(name):
            raise InstanceError(codes.INSTANCE_NAME_INVALID, f"Invalid instance name: {name}")
        instance_id = uuid.uuid4().hex[:12]
        root = self._root(instance_id)
        game = root / "game"
        for sub in ("mods", "config", "resourcepacks", "shaderpacks",
                    "screenshots", "saves", "logs"):
            (game / sub).mkdir(parents=True, exist_ok=True)

        instance = {
            "id": instance_id,
            "name": name,
            "minecraftVersion": minecraft_version,
            "loader": loader,
            "directory": str(root),
            "memory": {"minMb": memory_min_mb, "maxMb": memory_max_mb},
            "jvmArgs": [],
            "jvmPreset": "auto",
            "createdAt": instance_id,
            "lastPlayedAt": None,
            "launchCount": 0,
        }
        from core.config.writer import write_json_atomic
        write_json_atomic(root / "instance.json", instance)
        return instance

    def update(self, instance_id: str, patch: dict) -> dict | None:
        instance = self.get(instance_id)
        if not instance:
            return None
        instance.update(patch)
        from core.config.writer import write_json_atomic
        write_json_atomic(self._root(instance_id) / "instance.json", instance)
        return instance

    def delete(self, instance_id: str, *, confirm: bool = False) -> bool:
        """Xoá instance — bắt buộc confirm=True (mục 460)."""
        if not confirm:
            raise InstanceError(codes.VALIDATION_FAILED,
                                "Delete requires explicit confirmation")
        root = self._root(instance_id)
        if not root.exists():
            return False
        shutil.rmtree(root, ignore_errors=True)
        return True

    def open_folder(self, instance_id: str) -> str:
        game = self._root(instance_id) / "game"
        game.mkdir(parents=True, exist_ok=True)
        return str(game)
