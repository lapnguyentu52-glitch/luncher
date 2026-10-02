"""Resource pack projects — project.json + CRUD (spec 3.0 mục 10.3).

Layout trên đĩa (data/resource-studio/<projectId>/):
  project.json      — nguồn sự thật (schema mục 34, KHÔNG chứa binary)
  generated/        — assets đã sinh từ template (regenerate được)
  builds/           — output ZIP + manifest mỗi lần build
"""
from __future__ import annotations

import time
import uuid
from pathlib import Path

from app.context import AppContext
from core.config.writer import write_json_atomic
from core.errors import codes
from core.errors.base import AntaresError
from core.logging.setup import get_logger

logger = get_logger("resources.project")

SCHEMA = 1


class ProjectStore:
    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx

    def root(self) -> Path:
        d = Path(self._ctx.paths.resource_studio)
        d.mkdir(parents=True, exist_ok=True)
        return d

    def dir_of(self, project_id: str) -> Path:
        # Chặn path traversal (mục 76): id chỉ được là hex/slug an toàn
        if not project_id or any(c in project_id for c in "/\\..:"):
            raise AntaresError(codes.VALIDATION_FAILED, "Invalid project id")
        d = self.root() / project_id
        if not d.is_dir():
            raise AntaresError(codes.FILE_NOT_FOUND, f"Project not found: {project_id}")
        return d

    # ------------------------------------------------------------------

    def create(self, name: str, mc_version: str, template: str = "minimal",
               description: str = "") -> dict:
        if not name or not name.strip():
            raise AntaresError(codes.VALIDATION_FAILED, "Project name required")
        if not mc_version:
            raise AntaresError(codes.VALIDATION_FAILED, "Minecraft version required")

        project_id = uuid.uuid4().hex[:12]
        project = {
            "schema": SCHEMA,
            "id": project_id,
            "name": name.strip(),
            "description": description or "",
            "minecraft": {"version": mc_version},
            "template": template,
            "visuals": {"crosshair": {}, "totem": {}, "hud": {}},   # mục 34
            "resourcePack": {"enabled": True},
            "createdAt": time.time(),
            "updatedAt": time.time(),
            "buildCount": 0,
        }
        pdir = self.root() / project_id
        (pdir / "generated").mkdir(parents=True, exist_ok=True)
        (pdir / "builds").mkdir(parents=True, exist_ok=True)
        write_json_atomic(pdir / "project.json", project)
        logger.info("Created project %s (%s)", project_id, name)
        return project

    def get(self, project_id: str) -> dict:
        path = self.dir_of(project_id) / "project.json"
        import json
        try:
            return json.loads(path.read_text(encoding="utf-8"))
        except Exception as e:
            raise AntaresError(codes.FILE_NOT_FOUND, f"Corrupt project: {e}") from e

    def list(self) -> list[dict]:
        out = []
        for d in self.root().iterdir() if self.root().exists() else ():
            if d.is_dir() and (d / "project.json").is_file():
                try:
                    import json
                    out.append(json.loads((d / "project.json").read_text(encoding="utf-8")))
                except Exception:
                    pass
        return sorted(out, key=lambda p: p.get("createdAt", 0), reverse=True)

    def update(self, project_id: str, patch: dict) -> dict:
        project = self.get(project_id)
        allowed = ("name", "description", "visuals", "resourcePack", "template",
                   "assets")   # assets: RS v2 Batch 5 (mục 5.1)
        for k, v in patch.items():
            if k in allowed:
                project[k] = v
        project["updatedAt"] = time.time()
        write_json_atomic(self.dir_of(project_id) / "project.json", project)
        return project

    def delete(self, project_id: str) -> bool:
        import shutil
        d = self.dir_of(project_id)
        shutil.rmtree(d, ignore_errors=True)
        return True
