"""Pack builder — Project JSON -> assets -> ZIP + manifest (spec 3.0 mục 33).

Flow mục 33: Project JSON -> asset collector -> version adapter ->
template resolver -> pack builder -> validator -> ZIP writer -> SHA/manifest.
"""
from __future__ import annotations

import hashlib
import json
import re
import time
import zipfile
from pathlib import Path

from core.config.writer import write_json_atomic
from core.errors import codes
from core.errors.base import AntaresError
from core.logging.setup import get_logger
from services.resources import templates, versioning, validator
from services.resources.versioning import default_description as _default_desc

logger = get_logger("resources.builder")


class PackBuilder:
    def __init__(self, project_store) -> None:
        self._store = project_store

    # ------------------------------------------------------------------
    # Generate assets (template resolver + version adapter)
    # ------------------------------------------------------------------

    def generate(self, project_id: str) -> dict:
        """Sinh/regenerate generated/ từ template của project. Idempotent."""
        project = self._store.get(project_id)
        pdir = self._store.dir_of(project_id)
        generated = pdir / "generated"

        import shutil
        if generated.exists():
            shutil.rmtree(generated)
        generated.mkdir(parents=True)

        mc_version = project["minecraft"]["version"]
        files = templates.generate_files(project.get("template", "minimal"), mc_version)

        # pack.mcmeta theo version chọn (mục 6 — version-aware, một chỗ duy nhất)
        meta = versioning.pack_meta(mc_version, _default_desc(mc_version))
        (generated / "pack.mcmeta").write_text(
            json.dumps(meta, indent=2, ensure_ascii=False), encoding="utf-8")

        # pack.png nhỏ 64x64 màu accent template
        tpl = templates.BASE_TEMPLATES.get(project.get("template", "minimal")
                                           ) or templates.BASE_TEMPLATES["minimal"]
        templates.write_png(generated / "pack.png", 64, 64,
                            templates.solid(tpl["accent"], size=64))

        for rel, data in files.items():
            dest = generated / rel
            dest.parent.mkdir(parents=True, exist_ok=True)
            dest.write_bytes(data)

        count = len(files) + 2
        logger.info("Generated %d assets for %s (MC %s)", count, project_id, mc_version)
        return {"files": count, "mcVersion": mc_version,
                "packFormat": versioning.pack_format(mc_version),
                "warnings": versioning.version_warnings(mc_version)}

    # ------------------------------------------------------------------
    # Build ZIP + manifest
    # ------------------------------------------------------------------

    def build(self, project_id: str) -> dict:
        """Validate generated/ -> ZIP -> SHA256 manifest. ERROR thì fail build."""
        project = self._store.get(project_id)
        pdir = self._store.dir_of(project_id)
        generated = pdir / "generated"

        if not (generated / "pack.mcmeta").is_file():
            self.generate(project_id)

        findings = validator.validate_project_dir(generated)
        errors = [f for f in findings if f["severity"] == "ERROR"]
        if errors:
            first = errors[0]
            raise AntaresError(
                codes.VALIDATION_FAILED,
                f"Pack validation failed: {first['code']} at {first['path']}"
                f" ({first.get('detail', '')})")

        builds = pdir / "builds"
        builds.mkdir(exist_ok=True)
        slug = self._slug(project["name"])
        ts = int(time.time())
        zip_name = f"{slug}-{ts}.zip"
        zip_path = builds / zip_name

        file_hashes: dict[str, str] = {}
        with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as zf:
            for f in sorted(generated.rglob("*")):
                if not f.is_file():
                    continue
                rel = f.relative_to(generated).as_posix()
                zf.write(f, rel)
                file_hashes[rel] = hashlib.sha256(f.read_bytes()).hexdigest()

        zip_sha = hashlib.sha256(zip_path.read_bytes()).hexdigest()
        size = zip_path.stat().st_size
        manifest = {
            "project": project_id,
            "name": project["name"],
            "mcVersion": project["minecraft"]["version"],
            "packFormat": versioning.pack_format(project["minecraft"]["version"]),
            "packMetaMode": versioning.mcmeta_mode(project["minecraft"]["version"]),
            "file": zip_name,
            "sha256": zip_sha,
            "bytes": size,
            "files": len(file_hashes),
            "fileHashes": file_hashes,
            "warnings": [f for f in findings if f["severity"] == "WARNING"],
            "builtAt": ts,
        }
        write_json_atomic(builds / f"{zip_name}.manifest.json", manifest)

        # Cập nhật project
        project = self._store.update(project_id, {"buildCount":
                                                  (project.get("buildCount") or 0) + 1})
        logger.info("Built %s (%d files, %d bytes)", zip_name,
                    len(file_hashes), size)
        return manifest

    def list_builds(self, project_id: str) -> list[dict]:
        builds = self._store.dir_of(project_id) / "builds"
        out = []
        for m in sorted(builds.glob("*.manifest.json")):
            try:
                data = json.loads(m.read_text(encoding="utf-8"))
                out.append({k: v for k, v in data.items() if k != "fileHashes"})
            except Exception:
                pass
        return sorted(out, key=lambda b: b.get("builtAt", 0), reverse=True)

    def build_path(self, project_id: str, zip_name: str) -> Path:
        """Path ZIP build — chặn traversal (mục 76)."""
        if "/" in zip_name or "\\" in zip_name or ".." in zip_name:
            raise AntaresError(codes.VALIDATION_FAILED, "Invalid build name")
        p = self._store.dir_of(project_id) / "builds" / zip_name
        if not p.is_file():
            raise AntaresError(codes.FILE_NOT_FOUND, "Build not found")
        return p

    @staticmethod
    def _slug(name: str) -> str:
        return "".join(c if c.isalnum() else "-" for c in name.lower()).strip("-")[:40] or "pack"
