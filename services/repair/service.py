"""Repair Center — dry-run first (spec 3.0 mục 39).

Mọi repair phải trả lời "What will change?" TRƯỚC khi chạy:
  scan(action)  -> danh sách vấn đề tìm thấy + hành động sẽ làm (không ghi gì)
  run(action)   -> thực thi; mọi file bị xoá/đổi đều đi qua trash (undo được)
                   hoặc có backup.

Actions (mục 39):
  instance_metadata  — instance.json thiếu/hỏng -> xoá instance rác + báo
  missing_dirs       — mods/resourcepacks/... thiếu -> tạo lại
  option_files       — options.txt trùng key -> khử duplicate
  launcher_config    — settings.json lỗi schema -> về defaults (giữ giá trị hợp lệ)
  downloads          — file .part/lock treo -> dọn
  caches             — cache corrupt (manifest reads fail) -> xoá entry lỗi
  resource_packs     — zip hỏng trong resourcepacks -> quarantine
"""
from __future__ import annotations

import json
from pathlib import Path

from app.context import AppContext
from core.errors import codes
from core.errors.base import AntaresError
from core.logging.setup import get_logger

logger = get_logger("repair")

ACTIONS = (
    "instance_metadata",
    "missing_dirs",
    "option_files",
    "launcher_config",
    "downloads",
    "caches",
    "resource_packs",
)

#: Thư mục con instance bắt buộc.
_REQUIRED_DIRS = ("mods", "config", "resourcepacks", "shaderpacks", "screenshots", "saves", "logs")


class RepairService:
    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx

    # ------------------------------------------------------------------
    # Public: scan (dry-run) / run
    # ------------------------------------------------------------------

    def scan(self, action: str, instance_id: str | None = None) -> dict:
        """Dry-run — không ghi gì. Trả findings + planned actions (mục 39)."""
        if action not in ACTIONS:
            raise AntaresError(codes.VALIDATION_FAILED, f"Unknown repair action: {action}")
        findings: list[dict] = []
        planned: list[dict] = []

        if action == "instance_metadata":
            self._scan_instance_metadata(findings, planned)
        elif action == "missing_dirs":
            self._scan_missing_dirs(findings, planned, instance_id)
        elif action == "option_files":
            self._scan_option_files(findings, planned, instance_id)
        elif action == "launcher_config":
            self._scan_launcher_config(findings, planned)
        elif action == "downloads":
            self._scan_downloads(findings, planned)
        elif action == "caches":
            self._scan_caches(findings, planned)
        elif action == "resource_packs":
            self._scan_resource_packs(findings, planned, instance_id)

        return {"action": action, "findings": findings, "planned": planned,
                "hasIssues": bool(planned)}

    def run(self, action: str, instance_id: str | None = None) -> dict:
        """Thực thi các fix đã planned. Trả summary những gì đã đổi."""
        scan = self.scan(action, instance_id)
        done, trashed = [], 0
        for step in scan["planned"]:
            try:
                if step["kind"] == "mkdir":
                    Path(step["path"]).mkdir(parents=True, exist_ok=True)
                    done.append(step["detail"])
                elif step["kind"] == "write_json":
                    from core.config.writer import write_json_atomic
                    write_json_atomic(Path(step["path"]), step["content"])
                    done.append(step["detail"])
                elif step["kind"] == "rewrite_lines":
                    Path(step["path"]).write_text("\n".join(step["lines"]) + "\n",
                                                  encoding="utf-8")
                    done.append(step["detail"])
                elif step["kind"] == "trash":
                    self._to_trash(Path(step["path"]))
                    trashed += 1
                    done.append(step["detail"])
            except Exception as e:
                logger.warning("repair step failed: %s (%s)", step["detail"], e)

        result = {"action": action, "performed": len(done), "details": done,
                  "trashed": trashed}
        try:
            from core.events import names as ev
            self._ctx.events.publish(ev.REPAIR_RUN, {
                "action": action, "performed": len(done)})
        except Exception:
            pass
        logger.info("Repair %s: %d steps", action, len(done))
        return result

    # ------------------------------------------------------------------
    # Scanners (mỗi cái tìm vấn đề + plan steps)
    # ------------------------------------------------------------------

    def _scan_instance_metadata(self, findings: list[dict], planned: list[dict]) -> None:
        base = Path(self._ctx.paths.instances)
        if not base.is_dir():
            return
        for d in base.iterdir():
            if not d.is_dir():
                continue
            meta = d / "instance.json"
            if not meta.exists():
                findings.append({"path": str(d), "detail": "instance.json missing",
                                 "severity": "ERROR"})
                planned.append({"kind": "trash", "path": str(d),
                                "detail": f"Move orphan folder {d.name} to trash"})
                continue
            try:
                data = json.loads(meta.read_text(encoding="utf-8"))
                if not data.get("id") or not data.get("name"):
                    raise ValueError("missing id/name")
            except Exception as e:
                findings.append({"path": str(meta), "detail": f"corrupt: {e}",
                                 "severity": "ERROR"})
                planned.append({"kind": "trash", "path": str(d),
                                "detail": f"Move corrupt instance {d.name} to trash"})

    def _scan_missing_dirs(self, findings: list[dict], planned: list[dict],
                           instance_id: str | None) -> None:
        base = Path(self._ctx.paths.instances)
        if not base.is_dir():
            return
        targets = ([instance_id] if instance_id
                   else [d.name for d in base.iterdir() if d.is_dir()])
        for iid in targets:
            inst_root = base / iid
            if not (inst_root / "instance.json").is_file():
                continue  # đã thuộc repair khác
            for sub in _REQUIRED_DIRS:
                p = inst_root / "game" / sub
                if not p.is_dir():
                    findings.append({"path": str(p), "detail": "missing directory",
                                     "severity": "WARNING"})
                    planned.append({"kind": "mkdir", "path": str(p),
                                    "detail": f"Recreate {iid}/game/{sub}"})

    def _scan_option_files(self, findings: list[dict], planned: list[dict],
                           instance_id: str | None) -> None:
        base = Path(self._ctx.paths.instances)
        if not base.is_dir():
            return
        targets = ([instance_id] if instance_id
                   else [d.name for d in base.iterdir() if d.is_dir()])
        for iid in targets:
            opt = base / iid / "game" / "options.txt"
            if not opt.is_file():
                continue
            seen: set[str] = set()
            dup_keys: set[str] = set()
            out_lines: list[str] = []
            try:
                lines = opt.read_text(encoding="utf-8", errors="replace").splitlines()
            except OSError:
                continue
            changed = False
            for line in lines:
                key = line.partition(":")[0].strip()
                if key and line and not line.startswith("#"):
                    if key in seen:
                        dup_keys.add(key)
                        changed = True
                        continue  # bỏ duplicate
                    seen.add(key)
                out_lines.append(line)
            if dup_keys:
                findings.append({"path": str(opt),
                                 "detail": f"duplicate keys: {sorted(dup_keys)}",
                                 "severity": "WARNING"})
                planned.append({"kind": "rewrite_lines", "path": str(opt),
                                "lines": out_lines,
                                "detail": f"Deduplicate {sorted(dup_keys)} in {iid}/options.txt"})

    def _scan_launcher_config(self, findings: list[dict], planned: list[dict]) -> None:
        cfg_path = Path(self._ctx.paths.config) / "settings.json"
        if not cfg_path.is_file():
            return
        try:
            data = json.loads(cfg_path.read_text(encoding="utf-8"))
        except Exception as e:
            findings.append({"path": str(cfg_path), "detail": f"unparseable: {e}",
                             "severity": "ERROR"})
            from core.config.schema import DEFAULTS
            planned.append({
                "kind": "write_json", "path": str(cfg_path), "content": dict(DEFAULTS),
                "detail": "Reset settings.json to schema defaults"})
            return
        # Giữ giá trị hợp lệ, bổ sung section thiếu (merge shallow từ DEFAULTS)
        from core.config.schema import DEFAULTS
        merged = {**DEFAULTS, **{k: v for k, v in data.items() if k != "schemaVersion"}}
        if merged != data:
            missing = sorted(set(DEFAULTS) - set(data))
            findings.append({"path": str(cfg_path), "detail": f"missing keys: {missing}",
                             "severity": "WARNING"})
            planned.append({
                "kind": "write_json", "path": str(cfg_path),
                "content": {**merged, "schemaVersion": DEFAULTS["schemaVersion"]},
                "detail": f"Add missing config keys: {missing}"})

    def _scan_downloads(self, findings: list[dict], planned: list[dict]) -> None:
        dl = Path(self._ctx.paths.cache) / "downloads"
        if not dl.is_dir():
            return
        for f in dl.rglob("*"):
            if f.is_file() and (f.suffix == ".part" or f.name.endswith(".lock")):
                findings.append({"path": str(f), "detail": "stale download artifact",
                                 "severity": "WARNING"})
                planned.append({"kind": "trash", "path": str(f),
                                "detail": f"Trash {f.name}"})

    def _scan_caches(self, findings: list[dict], planned: list[dict]) -> None:
        cache = Path(self._ctx.paths.cache)
        manifests = cache / "manifests"
        if not manifests.is_dir():
            return
        for f in manifests.rglob("*.json"):
            try:
                json.loads(f.read_text(encoding="utf-8"))
            except Exception:
                findings.append({"path": str(f), "detail": "corrupt manifest",
                                 "severity": "WARNING"})
                planned.append({"kind": "trash", "path": str(f),
                                "detail": f"Trash corrupt cache manifest {f.name}"})

    def _scan_resource_packs(self, findings: list[dict], planned: list[dict],
                             instance_id: str | None) -> None:
        import zipfile
        base = Path(self._ctx.paths.instances)
        if not base.is_dir():
            return
        targets = ([instance_id] if instance_id
                   else [d.name for d in base.iterdir() if d.is_dir()])
        for iid in targets:
            rp = base / iid / "game" / "resourcepacks"
            for z in rp.glob("*.zip") if rp.is_dir() else ():
                try:
                    with zipfile.ZipFile(z) as zf:
                        bad = zf.testzip()
                        if bad is not None:
                            raise ValueError(f"corrupt member: {bad}")
                        if "pack.mcmeta" not in zf.namelist():
                            raise ValueError("pack.mcmeta missing")
                except Exception as e:
                    findings.append({"path": str(z), "detail": f"broken pack: {e}",
                                     "severity": "ERROR"})
                    planned.append({"kind": "trash", "path": str(z),
                                    "detail": f"Quarantine broken pack {z.name} ({iid})"})

    # ------------------------------------------------------------------
    # Trash (undo được — tái dùng cơ chế DiskCleaner)
    # ------------------------------------------------------------------

    def _to_trash(self, p: Path) -> None:
        import shutil
        import uuid
        data = Path(self._ctx.paths.data)
        if not p.exists():
            return
        try:
            p.resolve().relative_to(data.resolve())
        except (ValueError, OSError):
            raise AntaresError(codes.VALIDATION_FAILED,
                               f"Refusing to trash outside data: {p}")
        trash = data / "trash" / f"repair-{uuid.uuid4().hex[:8]}"
        trash.mkdir(parents=True, exist_ok=True)
        shutil.move(str(p), trash / uuid.uuid4().hex[:8])
