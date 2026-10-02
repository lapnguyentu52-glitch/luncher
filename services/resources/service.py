"""ResourceStudioService — facade hợp nhất cho Resource Pack Studio (mục 10, 33)."""
from __future__ import annotations

from pathlib import Path

from app.context import AppContext
from services.resources.builder import PackBuilder
from services.resources.installer import PackInstaller
from services.resources.layering import LayerStore
from services.resources.project import ProjectStore
from services.resources import importer, templates, validator, versioning
from services.resources import build_task as _build_task


class ResourceStudioService:
    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx
        self.projects = ProjectStore(ctx)
        self.builder = PackBuilder(self.projects)
        self.installer = PackInstaller(ctx)
        self._layers: LayerStore | None = None

    # ------------------------------------------------------------------
    # Wizard metadata (mục 10.2: chọn version + template + modules)
    # ------------------------------------------------------------------

    def wizard_info(self) -> dict:
        versions = versioning.supported_versions()
        return {
            "templates": [
                {"id": tid, "labelKey": tpl["labelKey"], "descKey": tpl["descKey"],
                 "modules": tpl["modules"]}
                for tid, tpl in templates.BASE_TEMPLATES.items()
            ],
            "versions": versions,
            "defaultVersion": versions[0] if versions else "1.21.4",
        }

    # ------------------------------------------------------------------
    # Delegates + composite ops
    # ------------------------------------------------------------------

    def create(self, name: str, mc_version: str, template: str = "minimal",
               description: str = "") -> dict:
        project = self.projects.create(name, mc_version, template, description)
        self.builder.generate(project["id"])   # sinh generated/ ngay để preview được
        return self.projects.get(project["id"])

    def get(self, project_id: str) -> dict:
        return self.projects.get(project_id)

    def list(self) -> list[dict]:
        return self.projects.list()

    def update(self, project_id: str, patch: dict) -> dict:
        return self.projects.update(project_id, patch)

    def delete(self, project_id: str) -> bool:
        return self.projects.delete(project_id)

    def generate(self, project_id: str) -> dict:
        return self.builder.generate(project_id)

    def validate(self, project_id: str) -> dict:
        project = self.projects.get(project_id)
        generated = self.projects.dir_of(project_id) / "generated"
        findings = validator.validate_project_dir(generated)
        return {
            "ok": not any(f["severity"] == "ERROR" for f in findings),
            "findings": findings,
            "packFormat": versioning.pack_format(project["minecraft"]["version"]),
        }

    def build(self, project_id: str) -> dict:
        result = self.builder.build(project_id)
        try:
            from core.events import names as ev
            self._ctx.events.publish(ev.RESOURCE_BUILT,
                                     {"projectId": project_id, "file": result.get("file")})
        except Exception:
            pass
        return result

    def build_task(self, project_id: str) -> dict:
        """Build dưới TaskManager — progress + cancel (mục 17, Batch 9.5).

        Return {task: task_dict, manifest: build_manifest}.
        Raise BuildCancelled (DOWNLOAD_CANCELLED) nếu user cancel.
        """
        manifest = _build_task.build_with_task(self._ctx, project_id)
        return {"manifest": manifest}

    def list_builds(self, project_id: str) -> list[dict]:
        return self.builder.list_builds(project_id)

    def build_path(self, project_id: str, zip_name: str) -> Path:
        return self.builder.build_path(project_id, zip_name)

    # ------------------------------------------------------------------
    # ZIP import (RS v2 Batch 6 — mục 13)
    # ------------------------------------------------------------------

    def zip_inspect(self, zip_path: Path) -> dict:
        """Quét ZIP an toàn — không ghi gì (mục 13.1: inspect trước import)."""
        import zipfile
        zip_path = Path(zip_path)
        if not zip_path.is_file():
            raise AntaresError(codes.FILE_NOT_FOUND, "ZIP not found")
        try:
            with zipfile.ZipFile(zip_path) as zf:
                return importer.inspect_zip(zf)
        except zipfile.BadZipFile as e:
            raise importer.ZipInvalidError(f"ZIP hỏng: {e}") from e

    def zip_import(self, project_id: str, zip_path: Path, policy: str = "keep",
                   conflict_overrides: dict | None = None) -> dict:
        return importer.import_zip(self._ctx, project_id, zip_path,
                                   policy=policy,
                                   conflict_overrides=conflict_overrides)

    # ------------------------------------------------------------------
    # Multi-pack layering (RS v2 Batch 7 — mục 25)
    # ------------------------------------------------------------------

    @property
    def layers(self) -> LayerStore:
        if self._layers is None:
            self._layers = LayerStore(self._ctx)
        return self._layers

    def install(self, project_id: str, instance_id: str,
                *, overwrite: bool = False) -> dict:
        """Build (nếu chưa có build nào) rồi install vào instance."""
        builds = self.builder.list_builds(project_id)
        if not builds:
            self.build(project_id)
            builds = self.builder.list_builds(project_id)
        latest = builds[0]
        zip_path = self.builder.build_path(project_id, latest["file"])
        result = self.installer.install_zip(zip_path, instance_id, overwrite=overwrite)
        try:
            from core.events import names as ev
            self._ctx.events.publish(ev.RESOURCE_INSTALLED,
                                     {"projectId": project_id, "instanceId": instance_id,
                                      "file": result.get("file")})
        except Exception:
            pass
        return result

    def installed(self, instance_id: str) -> list[dict]:
        return self.installer.list_installed(instance_id)

    def uninstall(self, instance_id: str, filename: str) -> bool:
        removed = self.installer.remove(instance_id, filename)
        if removed:
            # Batch 10 fix: sync lại options.txt sau khi uninstall — pack bị
            # xoá không còn nằm trong resourcePacks (mục 90 install UX)
            try:
                self.layers.set_order(instance_id, self.layers.get_order(instance_id))
            except AntaresError:
                pass     # instance không tồn tại — bỏ qua sync
        return removed
