"""Mrpack (Modrinth modpack) installer — port từ minecraft_loader/mrpack.py.

Học được từ lib:
 1. modrinth.index.json chứa files[] với "env" (client: required/optional/unsupported)
 2. Path traversal phải check (check_path_inside_minecraft_directory)
 3. overrides/ + client-overrides/ extract thẳng vào game dir
 4. dependencies["minecraft"] + loader phải cài sau khi files xong
"""
from __future__ import annotations

import json
import zipfile
from pathlib import Path

from core.errors import codes
from core.errors.base import AntaresError
from core.tasks.manager import Task
from core.utils.paths import ensure_inside
from infrastructure.fs.atomic import write_text_atomic


def read_mrpack_info(path: Path) -> dict:
    """Đọc thông tin modpack (port get_mrpack_information)."""
    try:
        with zipfile.ZipFile(path, "r") as zf:
            with zf.open("modrinth.index.json", "r") as f:
                index = json.load(f)
    except (zipfile.BadZipFile, KeyError, FileNotFoundError) as e:
        raise AntaresError(codes.VALIDATION_FAILED,
                           f"Invalid .mrpack file: {e}") from e
    return {
        "name": index.get("name", ""),
        "summary": index.get("summary", ""),
        "versionId": index.get("versionId", ""),
        "minecraftVersion": index.get("dependencies", {}).get("minecraft", ""),
        "loader": _detect_loader(index.get("dependencies", {})),
        "loaderVersion": _loader_version(index.get("dependencies", {})),
        "optionalFiles": [
            f["path"] for f in index.get("files", [])
            if f.get("env", {}).get("client") == "optional"
        ],
    }


def _detect_loader(deps: dict) -> str:
    for key, loader in (("fabric-loader", "fabric"), ("quilt-loader", "quilt"),
                        ("neoforge", "neoforge"), ("forge", "forge")):
        if key in deps:
            return loader
    return "vanilla"


def _loader_version(deps: dict) -> str | None:
    for key in ("fabric-loader", "quilt-loader", "neoforge", "forge"):
        if key in deps:
            return deps[key]
    return None


def _filter_files(files: list[dict], optional_selected: list[str]) -> list[dict]:
    """Port _filter_mrpack_files: chỉ lấy required + optional được chọn."""
    out = []
    for f in files:
        env = f.get("env")
        if env is None:
            out.append(f)
            continue
        if env.get("client") == "required":
            out.append(f)
        elif env.get("client") == "optional" and f.get("path") in optional_selected:
            out.append(f)
    return out


def install_mrpack(ctx, mrpack_path: Path, instance_id: str, task: Task, *,
                   optional_selected: list[str] | None = None,
                   skip_dependencies: bool = False) -> dict:
    """Cài modpack vào instance (port install_mrpack).

    Pipeline: read index -> download files (sha1) -> extract overrides -> deps.
    """
    info = read_mrpack_info(mrpack_path)
    game_dir = ctx.paths.instances / instance_id / "game"
    game_dir.mkdir(parents=True, exist_ok=True)

    dm = ctx.get("download_manager")

    with zipfile.ZipFile(mrpack_path, "r") as zf:
        with zf.open("modrinth.index.json", "r") as f:
            index = json.load(f)

        # 1. Download files
        file_list = _filter_files(index.get("files", []), optional_selected or [])
        total = len(file_list)
        for count, file in enumerate(file_list):
            if task.cancelled:
                raise AntaresError(codes.DOWNLOAD_CANCELLED, "Modpack install cancelled")
            target = ensure_inside(game_dir, game_dir / file["path"])
            target.parent.mkdir(parents=True, exist_ok=True)
            task.message = f"Downloading {file['path']} ({count + 1}/{total})"
            task.progress = 100.0 * count / max(1, total)
            dm.download(file["downloads"][0], target,
                        sha1=file.get("hashes", {}).get("sha1"), task=task,
                        overwrite=True)

        # 2. Extract overrides (safe path)
        task.message = "Extracting overrides"
        for zip_name in zf.namelist():
            if not (zip_name.startswith("overrides/") or
                    zip_name.startswith("client-overrides/")):
                continue
            if zf.getinfo(zip_name).file_size == 0:
                continue
            prefix = ("client-overrides/" if zip_name.startswith("client-overrides/")
                      else "overrides/")
            rel = zip_name[len(prefix):]
            target = ensure_inside(game_dir, game_dir / rel)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(zf.read(zip_name))

    # 3. Dependencies (MC + loader) — qua loader registry
    if not skip_dependencies:
        mc_version = index.get("dependencies", {}).get("minecraft")
        loader = _detect_loader(index.get("dependencies", {}))
        task.message = f"Installing Minecraft {mc_version}"
        from services.loaders.registry import get_loader
        get_loader("vanilla").install(mc_version, str(game_dir), task)
        if loader != "vanilla":
            task.message = f"Installing {loader}"
            get_loader(loader).install(mc_version, str(game_dir), task)

    # 4. Update instance metadata
    task.message = "Updating instance"
    instance_service = ctx.get("instances")
    instance_service.update(instance_id, {
        "minecraftVersion": info["minecraftVersion"],
        "loader": info["loader"],
    })

    return {"name": info["name"], "minecraftVersion": info["minecraftVersion"],
            "loader": info["loader"], "filesInstalled": len(file_list)}
