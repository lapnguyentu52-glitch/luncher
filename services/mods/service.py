"""ModService — search/install/uninstall mods cho instance."""
from __future__ import annotations

from pathlib import Path

from app.context import AppContext
from core.errors import codes
from core.errors.base import AntaresError
from core.tasks.manager import Task
from infrastructure.http.client import HttpClient
from services.mods.modrinth import ModrinthClient


class ModService:
    def __init__(self, ctx: AppContext, http: HttpClient) -> None:
        self._ctx = ctx
        self._client = ModrinthClient(http)

    def search(self, query: str, *, loader: str, mc_version: str) -> list[dict]:
        return self._client.search(query, loader=loader, mc_version=mc_version)

    def install(self, project_id: str, instance_id: str, *, loader: str,
                mc_version: str, task: Task | None = None) -> str:
        """Cài mod vào instance. Trả filename. Fail -> không để nửa chừng."""
        versions = self._client.get_versions(project_id, loader=loader,
                                             mc_version=mc_version)
        if not versions:
            raise AntaresError(codes.VALIDATION_FAILED,
                               f"No {loader} build for {mc_version}")
        file = self._client.pick_file(versions[0])
        if not file:
            raise AntaresError(codes.VALIDATION_FAILED, "No downloadable file")

        mods_dir = self._ctx.paths.instances / instance_id / "game" / "mods"
        mods_dir.mkdir(parents=True, exist_ok=True)
        target = mods_dir / file["filename"]

        dm = self._ctx.get("download_manager")
        dm.download(file["url"], target, task=task,
                    sha1=file.get("hashes", {}).get("sha1"))
        return file["filename"]

    def list_installed(self, instance_id: str) -> list[str]:
        mods_dir = self._ctx.paths.instances / instance_id / "game" / "mods"
        if not mods_dir.exists():
            return []
        return sorted(f.name for f in mods_dir.glob("*.jar"))

    def uninstall(self, instance_id: str, filename: str) -> bool:
        target = self._ctx.paths.instances / instance_id / "game" / "mods" / filename
        if not target.exists():
            return False
        target.unlink()
        return True
