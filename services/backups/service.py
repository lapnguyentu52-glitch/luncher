"""Backup & Restore Center (spec 3.0 mục 22).

Snapshot layout (mục 22):
  data/backups/<snapshotId>/
    metadata.json   — id, label, targets, timestamps, file count/bytes
    config/         — settings.json launcher (target: config)
    instances/      — <id>/instance.json + mods/ + resourcepacks/ + saves/...

Nguyên tắc:
- Restore KHÔNG xoá đè mù quáng: toàn bộ state hiện tại được snapshot
  "pre-restore" tự động trước (mục 74: backup trước destructive operation).
- Export/Import ZIP để chia sẻ/đưa sang máy khác.
"""
from __future__ import annotations

import shutil
import time
import uuid
import zipfile
from pathlib import Path

from app.context import AppContext
from core.config.writer import write_json_atomic
from core.errors import codes
from core.errors.base import AntaresError
from core.logging.setup import get_logger

logger = get_logger("backups")

#: Target hợp lệ (mục 22).
TARGETS = ("instance", "config", "mods", "resourcepacks", "saves")

#: Thư mục con của instance tương ứng target.
_TARGET_SUBDIR = {
    "mods": "mods",
    "resourcepacks": "resourcepacks",
    "saves": "saves",
}


class BackupService:
    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx

    # ------------------------------------------------------------------
    # Paths
    # ------------------------------------------------------------------

    @property
    def _backups(self) -> Path:
        d = Path(self._ctx.paths.data) / "backups" / "snapshots"
        d.mkdir(parents=True, exist_ok=True)
        return d

    def _snap_dir(self, snapshot_id: str) -> Path:
        if not snapshot_id or any(c in snapshot_id for c in "/\\..:"):
            raise AntaresError(codes.VALIDATION_FAILED, "Invalid snapshot id")
        d = self._backups / snapshot_id
        if not d.is_dir():
            raise AntaresError(codes.FILE_NOT_FOUND, "Snapshot not found")
        return d

    # ------------------------------------------------------------------
    # Create
    # ------------------------------------------------------------------

    def create(self, *, instance_id: str | None = None, targets: list[str] | None = None,
               label: str = "") -> dict:
        """Tạo snapshot. instance_id=None + targets=['config'] = backup launcher config."""
        targets = [t for t in (targets or ["config"]) if t in TARGETS]
        if not targets:
            raise AntaresError(codes.VALIDATION_FAILED, "No valid backup targets")

        if instance_id and "config" not in targets and not any(
                t in targets for t in ("instance", "mods", "resourcepacks", "saves")):
            raise AntaresError(codes.VALIDATION_FAILED, "Instance backup needs a data target")
        if instance_id and not self._ctx.get("instances").get(instance_id):
            raise AntaresError(codes.INSTANCE_NOT_FOUND, "Instance not found")

        snap_id = f"{time.strftime('%Y%m%d-%H%M%S')}-{uuid.uuid4().hex[:6]}"
        root = self._backups / snap_id
        files = 0
        bytes_total = 0

        def copy_into(src: Path, dest_dir: Path) -> tuple[int, int]:
            n, b = 0, 0
            if not src.exists():
                return n, b
            if src.is_file():
                dest_dir.mkdir(parents=True, exist_ok=True)
                shutil.copy2(src, dest_dir / src.name)
                return 1, src.stat().st_size
            for f in src.rglob("*"):
                if f.is_file():
                    rel = f.relative_to(src)
                    (dest_dir / rel).parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(f, dest_dir / rel)
                    n += 1
                    b += f.stat().st_size
            return n, b

        for target in targets:
            if target == "config":
                n, b = copy_into(Path(self._ctx.paths.config), root / "config")
                files += n
                bytes_total += b
                continue
            if not instance_id:
                continue
            inst_root = Path(self._ctx.paths.instances) / instance_id
            if target == "instance":
                # metadata + 3 thư mục dữ liệu chính (KHÔNG copy game dir nặng:
                n, b = copy_into(inst_root / "instance.json", root / "instances" / instance_id)
                files += n
                bytes_total += b
                # version jars, libraries, assets — launcher dùng chung cache)
                for sub in ("mods", "resourcepacks", "saves"):
                    n, b = copy_into(inst_root / "game" / sub,
                                     root / "instances" / instance_id / sub)
                    files += n
                    bytes_total += b
            else:
                sub = _TARGET_SUBDIR[target]
                n, b = copy_into(inst_root / "game" / sub,
                                 root / "instances" / instance_id / sub)
                files += n
                bytes_total += b

        meta = {
            "id": snap_id,
            "label": label or "",
            "targets": targets,
            "instanceId": instance_id,
            "createdAt": time.time(),
            "files": files,
            "bytes": bytes_total,
        }
        write_json_atomic(root / "metadata.json", meta)
        self._publish("backup.created", {"snapshotId": snap_id})
        logger.info("Snapshot %s: %d files, %.1f KB (%s)",
                    snap_id, files, bytes_total / 1024, ",".join(targets))
        return meta

    def _publish(self, name: str, payload: dict) -> None:
        try:
            self._ctx.events.publish(name, payload)
        except Exception:
            pass

    # ------------------------------------------------------------------
    # List / Delete
    # ------------------------------------------------------------------

    def list(self) -> list[dict]:
        out = []
        for d in self._backups.iterdir() if self._backups.exists() else ():
            meta = d / "metadata.json"
            if d.is_dir() and meta.is_file():
                try:
                    import json
                    out.append(json.loads(meta.read_text(encoding="utf-8")))
                except Exception:
                    pass
        return sorted(out, key=lambda m: m.get("createdAt", 0), reverse=True)

    def delete(self, snapshot_id: str) -> bool:
        shutil.rmtree(self._snap_dir(snapshot_id), ignore_errors=True)
        return True

    # ------------------------------------------------------------------
    # Restore (có pre-restore backup — mục 74)
    # ------------------------------------------------------------------

    def restore(self, snapshot_id: str) -> dict:
        meta = self._metadata(snapshot_id)

        # 1. Backup state HIỆN TẠI trước khi ghi đè (mục 74)
        pre = self.create(
            instance_id=meta.get("instanceId"),
            targets=meta.get("targets") or ["config"],
            label=f"pre-restore of {snapshot_id}")

        # 2. Đè từ snapshot
        root = self._snap_dir(snapshot_id)
        restored = 0

        config_src = root / "config"
        if config_src.is_dir():
            for f in config_src.rglob("*"):
                if f.is_file():
                    dest = Path(self._ctx.paths.config) / f.relative_to(config_src)
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(f, dest)
                    restored += 1
            # ConfigManager đang cache in-memory -> đọc lại từ đĩa
            try:
                self._ctx.config.reload()
            except Exception:
                logger.exception("config reload after restore failed")

        inst_root_src = root / "instances"
        if inst_root_src.is_dir():
            for snap_inst in inst_root_src.iterdir():
                if not snap_inst.is_dir():
                    continue
                dest_base = Path(self._ctx.paths.instances) / snap_inst.name
                # instance.json
                meta_file = snap_inst / "instance.json"
                if meta_file.is_file():
                    (dest_base / "instance.json").parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(meta_file, dest_base / "instance.json")
                    restored += 1
                for sub in ("mods", "resourcepacks", "saves"):
                    src = snap_inst / sub
                    if not src.is_dir():
                        continue
                    dest = dest_base / "game" / sub
                    # Đồng bộ: xoá file hiện có không có trong snapshot (restore thật sự)
                    if dest.is_dir():
                        for cur in dest.rglob("*"):
                            if cur.is_file() and not (src / cur.relative_to(dest)).exists():
                                cur.unlink()
                    for f in src.rglob("*"):
                        if f.is_file():
                            d = dest / f.relative_to(src)
                            d.parent.mkdir(parents=True, exist_ok=True)
                            shutil.copy2(f, d)
                            restored += 1

        self._publish("backup.restored", {"snapshotId": snapshot_id})
        logger.info("Restored %s (%d files). Pre-restore snapshot: %s",
                    snapshot_id, restored, pre["id"])
        return {"restored": restored, "preRestoreSnapshotId": pre["id"]}

    def _metadata(self, snapshot_id: str) -> dict:
        import json
        path = self._snap_dir(snapshot_id) / "metadata.json"
        try:
            return json.loads(path.read_text(encoding="utf-8"))
        except Exception as e:
            raise AntaresError(codes.FILE_NOT_FOUND, "Corrupt snapshot metadata") from e

    # ------------------------------------------------------------------
    # Export / Import ZIP
    # ------------------------------------------------------------------

    def export_zip(self, snapshot_id: str) -> Path:
        root = self._snap_dir(snapshot_id)
        zip_path = self._backups / f"{snapshot_id}.zip"
        if zip_path.exists():
            return zip_path
        with zipfile.ZipFile(zip_path, "w", zipfile.ZIP_DEFLATED) as zf:
            for f in root.rglob("*"):
                if f.is_file():
                    zf.write(f, f.relative_to(root))
        return zip_path

    def import_zip(self, zip_path: str) -> dict:
        src = Path(zip_path)
        if not src.is_file():
            raise AntaresError(codes.FILE_NOT_FOUND, "ZIP not found")
        with zipfile.ZipFile(src) as zf:
            names = zf.namelist()
            if "metadata.json" not in names:
                raise AntaresError(codes.VALIDATION_FAILED,
                                   "Not an Antares snapshot (metadata.json missing)")
            import json
            meta = json.loads(zf.read("metadata.json").decode("utf-8"))
            snap_id = str(meta.get("id") or f"imported-{uuid.uuid4().hex[:6]}")
            if any(c in snap_id for c in "/\\..:"):
                raise AntaresError(codes.VALIDATION_FAILED, "Invalid snapshot id in archive")
            dest = self._backups / snap_id
            if dest.exists():
                shutil.rmtree(dest)
            zf.extractall(dest)
        return self._metadata(snap_id)
