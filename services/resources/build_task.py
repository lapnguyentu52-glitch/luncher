"""Build task — chạy PackBuilder dưới TaskManager với progress (mục 17).

Flow mục 17: UI -> TaskManager -> worker -> progress events -> UI bridge.
Progress 0-100 theo stage: generate(0-20) -> validate(20-40) -> zip(40-90)
-> manifest(90-100). Cancellation được check giữa các stage + từng entry
ZIP; cleanup on cancel (partial ZIP xoá, không publish).

Event RESOURCE_BUILT publish giữ nguyên hành vi sync `build()` (mục 24 —
existing UI dựa vào đó). Build sync `build()` vẫn giữ cho path ngắn —
API task là add-on, không breaking.
"""
from __future__ import annotations

import hashlib
import time
import zipfile
from pathlib import Path

from core.errors import codes
from core.errors.base import AntaresError
from core.logging.setup import get_logger
from core.tasks.manager import TaskState

logger = get_logger("resources.build_task")

#: Logging build task (mục 32) — không log secrets/paths nhạy cảm
_FIELDS = ("task_id", "project_id", "mc_version", "stage", "duration_ms",
           "output_size", "sha256", "result")


class BuildCancelled(AntaresError):
    """User cancel giữa chừng — partial ZIP đã dọn."""

    def __init__(self) -> None:
        super().__init__(codes.DOWNLOAD_CANCELLED, "Build cancelled",
                         recoverable=True, action="RETRY")


def build_with_task(ctx, project_id: str) -> dict:
    """Build project dưới 1 Task — progress + cancellation (mục 17).

    Return manifest giống build() sync. Raise BuildCancelled nếu user cancel
    (partial ZIP đã dọn — không publish partial, mục 16).
    """
    rs: ResourceStudioService = ctx.get("resource_studio")
    rs.projects.dir_of(project_id)                     # validate + tồn tại
    project = rs.get(project_id)
    pdir = rs.projects.dir_of(project_id)
    generated = pdir / "generated"

    tm = ctx.tasks
    task = tm.create("resource_build", owner="resource_studio")
    task_id = task.id
    tm.start(task)
    t0 = time.perf_counter()

    def prog(value: float, stage: str) -> None:
        tm.progress(task, value, stage)

    def check_cancel() -> None:
        if task.cancelled:
            raise BuildCancelled()

    zip_path: Path | None = None
    try:
        # ---------- Stage 1: generate (0-20) ----------
        prog(5, "generate")
        if not (generated / "pack.mcmeta").is_file():
            rs.builder.generate(project_id)
        prog(20, "generate")

        # ---------- Stage 2: validate (20-40) ----------
        prog(25, "validate")
        check_cancel()
        findings = validator_validate(generated)
        errors = [f for f in findings if f["severity"] == "ERROR"]
        if errors:
            first = errors[0]
            raise AntaresError(
                codes.VALIDATION_FAILED,
                f"Pack validation failed: {first['code']} at {first['path']}"
                f" ({first.get('detail', '')})")
        prog(40, "validate")

        # ---------- Stage 3: zip (40-90, per-file progress + cancel check) ----------
        prog(45, "zip")
        builds = pdir / "builds"
        builds.mkdir(exist_ok=True)
        slug = rs.builder._slug(project["name"])
        ts = int(time.time())
        zip_name = f"{slug}-{ts}.zip"
        zip_path = builds / zip_name

        files = sorted(f for f in generated.rglob("*") if f.is_file())
        file_hashes: dict[str, str] = {}
        with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as zf:
            for i, f in enumerate(files):
                check_cancel()
                rel = f.relative_to(generated).as_posix()
                zf.write(f, rel)
                file_hashes[rel] = hashlib.sha256(f.read_bytes()).hexdigest()
                # 40 -> 90 chia theo file
                prog(40 + 50 * (i + 1) // max(1, len(files)), "zip")

        # ---------- Stage 4: manifest (90-100) ----------
        check_cancel()
        prog(92, "manifest")
        zip_sha = hashlib.sha256(zip_path.read_bytes()).hexdigest()
        size = zip_path.stat().st_size
        manifest = {
            "project": project_id,
            "name": project["name"],
            "mcVersion": project["minecraft"]["version"],
            "packFormat": versioning_pack_format(project["minecraft"]["version"]),
            "packMetaMode": packmeta_mode(project["minecraft"]["version"]),
            "file": zip_name,
            "sha256": zip_sha,
            "bytes": size,
            "files": len(file_hashes),
            "fileHashes": file_hashes,
            "warnings": [f for f in findings if f["severity"] == "WARNING"],
            "builtAt": ts,
        }
        write_manifest(builds / f"{zip_name}.manifest.json", manifest)
        prog(100, "manifest")

        # cập nhật buildCount (giống build() sync)
        rs.projects.update(project_id, {
            "buildCount": (project.get("buildCount") or 0) + 1})
        tm.complete(task, {"file": zip_name, "sha256": zip_sha})

        _log_build(task_id=task_id, project_id=project_id,
                   mc_version=project["minecraft"]["version"], stage="done",
                   duration_ms=int((time.perf_counter() - t0) * 1000),
                   output_size=size, sha256=zip_sha, result="ok")
        _publish_built(ctx, project_id, zip_name)
        return manifest

    except BuildCancelled:
        # Cleanup on cancel (mục 17): xoá partial ZIP + manifest dở
        if zip_path is not None and zip_path.exists():
            zip_path.unlink(missing_ok=True)
            manifest_path = zip_path.with_name(zip_path.name + ".manifest.json")
            manifest_path.unlink(missing_ok=True)
        tm.cancel(task_id)
        _log_build(task_id=task_id, project_id=project_id,
                   mc_version=project["minecraft"]["version"], stage="cancelled",
                   duration_ms=int((time.perf_counter() - t0) * 1000),
                   output_size=0, sha256="", result="cancelled")
        raise
    except AntaresError as e:
        # fail: dọn partial ZIP nếu đã tạo
        if zip_path is not None and zip_path.exists():
            zip_path.unlink(missing_ok=True)
        tm.fail(task, {"code": e.code, "message": e.message})
        _log_build(task_id=task_id, project_id=project_id,
                   mc_version=project["minecraft"]["version"], stage="failed",
                   duration_ms=int((time.perf_counter() - t0) * 1000),
                   output_size=0, sha256="", result=e.code)
        raise
    except Exception as e:      # noqa: BLE001 — rìa service phải logger.exception
        logger.exception("Build task failed")
        if zip_path is not None and zip_path.exists():
            zip_path.unlink(missing_ok=True)
        tm.fail(task, {"code": codes.INTERNAL_ERROR, "message": str(e)})
        raise


# ---- helpers (import trễ tránh circular) ----

def validator_validate(generated: Path) -> list[dict]:
    from services.resources import validator
    return validator.validate_project_dir(generated)


def versioning_pack_format(mc_version: str) -> int:
    from services.resources import versioning
    return versioning.pack_format(mc_version)


def packmeta_mode(mc_version: str) -> str:
    from services.resources import versioning
    return versioning.mcmeta_mode(mc_version)


def write_manifest(path: Path, manifest: dict) -> None:
    from core.config.writer import write_json_atomic
    write_json_atomic(path, manifest)


def _publish_built(ctx, project_id: str, zip_name: str) -> None:
    """Event RESOURCE_BUILT — giữ nguyên taxonomy (mục 24)."""
    try:
        from core.events import names as ev
        ctx.events.publish(ev.RESOURCE_BUILT,
                           {"projectId": project_id, "file": zip_name})
    except Exception:
        pass


def _log_build(**kwargs) -> None:
    """Log theo mục 32 — chỉ field an toàn."""
    logger.info("build_task %s", " ".join(f"{k}={v}" for k, v in kwargs.items()))
