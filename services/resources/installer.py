"""Pack installer — install ZIP vào instance (spec 3.0 mục 75, 33 step cuối).

An toàn (mục 75): không overwrite silent — file trùng tên trong resourcepacks
của instance được backup vào data/backups/resourcepacks/ trước khi ghi.
"""
from __future__ import annotations

import hashlib
import shutil
import time
import zipfile
from pathlib import Path

from app.context import AppContext
from core.errors import codes
from core.errors.base import AntaresError
from core.logging.setup import get_logger

logger = get_logger("resources.installer")


class PackInstaller:
    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx

    def install_zip(self, zip_path: Path, instance_id: str,
                    *, overwrite: bool = False) -> dict:
        """Copy ZIP vào <instance>/game/resourcepacks/. Trả thông tin cài."""
        inst = self._ctx.get("instances").get(instance_id)
        if not inst:
            raise AntaresError(codes.INSTANCE_NOT_FOUND, "Instance not found")

        rp_dir = Path(self._ctx.paths.instances) / instance_id / "game" / "resourcepacks"
        rp_dir.mkdir(parents=True, exist_ok=True)

        zip_path = Path(zip_path)
        if not zip_path.is_file():
            raise AntaresError(codes.FILE_NOT_FOUND, "ZIP not found")

        # Validate ZIP trước khi copy (mục 75: validate -> install)
        with zipfile.ZipFile(zip_path) as zf:
            findings = validator_quick(zf)
            errors = [f for f in findings if f["severity"] == "ERROR"]
            if errors:
                raise AntaresError(codes.VALIDATION_FAILED,
                                   f"Invalid resource pack: {errors[0]['detail']}")

        dest = rp_dir / zip_path.name
        backed_up = None
        if dest.exists():
            if not overwrite:
                raise AntaresError(
                    codes.VALIDATION_FAILED,
                    f"'{dest.name}' already exists (overwrite=True to replace)")
            backed_up = self._backup(dest)

        shutil.copy2(zip_path, dest)
        sha = hashlib.sha256(dest.read_bytes()).hexdigest()
        logger.info("Installed %s -> %s (backup=%s)", zip_path.name, instance_id,
                    bool(backed_up))
        return {
            "file": dest.name, "bytes": dest.stat().st_size,
            "sha256": sha, "backup": backed_up,
            "installedAt": time.time(),
        }

    def _backup(self, dest: Path) -> str:
        backup_dir = Path(self._ctx.paths.data) / "backups" / "resourcepacks"
        backup_dir.mkdir(parents=True, exist_ok=True)
        name = f"{dest.name}.{int(time.time())}.bak"
        shutil.copy2(dest, backup_dir / name)
        return name

    def list_installed(self, instance_id: str) -> list[dict]:
        rp_dir = Path(self._ctx.paths.instances) / instance_id / "game" / "resourcepacks"
        out = []
        for p in rp_dir.glob("*.zip") if rp_dir.exists() else ():
            out.append({"file": p.name, "bytes": p.stat().st_size,
                        "mtime": p.stat().st_mtime})
        return sorted(out, key=lambda x: x["file"])

    def remove(self, instance_id: str, filename: str) -> bool:
        if "/" in filename or "\\" in filename or ".." in filename:
            raise AntaresError(codes.VALIDATION_FAILED, "Invalid filename")
        p = Path(self._ctx.paths.instances) / instance_id / "game" / "resourcepacks" / filename
        if p.is_file():
            p.unlink()
            return True
        return False


def validator_quick(zf: zipfile.ZipFile) -> list[dict]:
    from services.resources.validator import validate_zip
    return validate_zip(zf)
