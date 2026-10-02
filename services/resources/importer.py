"""ZIP pack importer (master plan v3 mục 13 — Batch 6).

Import resource pack .zip vào project bằng safe extraction:

- KHÔNG dùng extractall (mục 13.1) — mọi entry qua canonical path validation.
- 2-phase: inspect (đọc metadata, không ghi gì) -> import (staging dir, rồi
  merge vào project; lỗi/staging dở -> rollback, generated/ không hỏng).
- Threat model mục 13.2: zip-slip (../, absolute, Windows drive, UNC, backslash),
  nested archive bomb, duplicate filename, huge file/count/total, invalid
  UTF-8 names, malformed ZIP, symlink-like entry, overwrite policy.

Chỉ IMPORT texture/model/sound/lang asset vào project — KHÔNG copy pack.mcmeta
(project tự sinh theo version — mục 6).

Conflicts (mục 93): entry trùng với file sẵn có của project — policy
"keep" (giữ cũ) | "replace" (ghi đè) | "keep_both" (đổi tên -imported).
Không tự replace âm thầm. Per-file overrides qua conflict_overrides dict.
"""
from __future__ import annotations

import re
import shutil
import time
import uuid
import zipfile
from pathlib import Path, PurePosixPath
from typing import Any

from app.context import AppContext
from core.errors import codes
from core.errors.base import AntaresError
from core.logging.setup import get_logger

logger = get_logger("resources.importer")

#: Limits (decision Batch 0 — mục 34)
MAX_TOTAL_UNCOMPRESSED = 256 * 1024 * 1024     # 256 MB
MAX_ENTRY_BYTES = 64 * 1024 * 1024             # 64 MB / file
MAX_ENTRIES = 4096

#: Extensions được import vào pack (khớp validator OK_EXTS + phần mở rộng pack)
OK_EXTS = {".png", ".json", ".mcmeta", ".ogg", ".lang", ".txt", ".cfg"}

#: Chỉ nhận asset paths — pack.mcmeta/pack.png loại bỏ (project tự sinh)
_SKIP_NAMES = {"pack.mcmeta", "pack.png"}

_CONFLICT_POLICIES = ("keep", "replace", "keep_both")


class ZipSecurityError(AntaresError):
    """Entry ZIP bị chặn vì security — code ZIP_SECURITY_REJECTED."""

    def __init__(self, message: str) -> None:
        super().__init__(codes.ZIP_SECURITY_REJECTED, message)


class ZipInvalidError(AntaresError):
    def __init__(self, message: str) -> None:
        super().__init__(codes.ZIP_INVALID, message)


# ----------------------------------------------------------------------
# Path validation (mục 13.1: normalize -> reject -> resolve -> contain)
# ----------------------------------------------------------------------

_WINDOWS_DRIVE = re.compile(r"^[A-Za-z]:")
_UNC = re.compile(r"^(\\\\|//)")


def sanitize_entry_name(name: str) -> str:
    """Entry name -> canonical POSIX rel path. Raise ZipSecurityError nếu nguy hiểm.

    Trả về path chuẩn hoá (forward slash, không leading ./) hoặc raise.
    """
    if not name or not isinstance(name, str):
        raise ZipSecurityError("Entry không có tên")
    # Backslash -> slash trước (Windows-style entry trong zip)
    n = name.replace("\\", "/").strip()

    # UNC path
    if _UNC.match(n):
        raise ZipSecurityError(f"UNC path bị chặn: {name!r}")
    # Windows drive (C:/...)
    if _WINDOWS_DRIVE.match(n):
        raise ZipSecurityError(f"Absolute path bị chặn: {name!r}")
    # Absolute POSIX
    if n.startswith("/"):
        raise ZipSecurityError(f"Absolute path bị chặn: {name!r}")

    parts = []
    for seg in n.split("/"):
        if seg in ("", "."):
            continue
        if seg == "..":
            raise ZipSecurityError(f"Path traversal bị chặn: {name!r}")
        # Ký tự điều khiển / không phải UTF-8 hợp lý -> chặn
        if any(ord(c) < 32 for c in seg):
            raise ZipSecurityError(f"Tên entry có ký tự không hợp lệ: {name!r}")
        parts.append(seg)
    if not parts:
        raise ZipSecurityError(f"Entry tên rỗng: {name!r}")
    # Symlink-like: tên chứa dạng link tuyệt đối không thể xảy ra sau các
    # check trên; entry directory (kết thúc /) đã được zip ra rỗng -> skip ở
    # mức inspect. Colon chỉ chặn ở vị trí drive (đã check) — giữ : cho tên.
    return "/".join(parts)


def _safe_ext(name: str) -> bool:
    dot = name.rfind(".")
    if dot < 0:
        return False
    return name[dot:].lower() in OK_EXTS


# ----------------------------------------------------------------------
# Inspect — đọc metadata, không ghi gì (mục 51: Inspect -> Validate)
# ----------------------------------------------------------------------

def inspect_zip(zf: zipfile.ZipFile) -> dict:
    """Quét ZIP -> summary + danh sách entry hợp lệ + findings.

    Return:
    {
      "ok": bool,                  # True nếu không ERROR
      "findings": [{code, severity, path, detail}],
      "entries": [{path, bytes}],  # entry sẽ import (đã lọc dir + skip)
      "totals": {"files": N, "bytes": N},
    }
    """
    findings: list[dict] = []
    entries: list[dict] = []
    seen: set[str] = set()
    total_bytes = 0

    infos = zf.infolist()
    if len(infos) > MAX_ENTRIES:
        findings.append({"code": "zip_too_many_entries", "severity": "ERROR",
                         "path": "", "detail": f"{len(infos)} > {MAX_ENTRIES}"})
        return {"ok": False, "findings": findings, "entries": [],
                "totals": {"files": 0, "bytes": 0}}

    for info in infos:
        name = info.filename
        # Directory entry — bỏ qua
        if name.endswith("/") or info.is_dir():
            continue

        # 1. Canonical path validation (zip-slip/absolute/UNC/..)
        try:
            rel = sanitize_entry_name(name)
        except ZipSecurityError as e:
            findings.append({"code": "zip_slip", "severity": "ERROR",
                             "path": name, "detail": str(e)})
            continue

        # 2. Duplicate filename (sau normalize — 2 tên khác cách viết cùng path)
        if rel in seen:
            findings.append({"code": "zip_duplicate_entry", "severity": "ERROR",
                             "path": rel, "detail": "trùng path sau normalize"})
            continue
        seen.add(rel)

        # 3. Nested archive — chặn zip trong zip (bomb vector, TRƯỚC ext check)
        if rel.lower().endswith((".zip", ".jar", ".tar", ".gz", ".7z")):
            findings.append({"code": "zip_nested_archive", "severity": "ERROR",
                             "path": rel, "detail": "archive lồng nhau bị chặn"})
            continue

        # 4. Extension whitelist
        if not _safe_ext(rel):
            findings.append({"code": "zip_unsupported_entry", "severity": "WARNING",
                             "path": rel,
                             "detail": f"bỏ qua (extension không thuộc pack asset)"})
            continue

        # 5. Size limits — dùng file_size từ central directory (không giải nén)
        size = info.file_size
        if size > MAX_ENTRY_BYTES:
            findings.append({"code": "zip_entry_too_large", "severity": "ERROR",
                             "path": rel, "detail": f"{size} > {MAX_ENTRY_BYTES}"})
            continue
        total_bytes += size
        if total_bytes > MAX_TOTAL_UNCOMPRESSED:
            findings.append({"code": "zip_bomb", "severity": "ERROR",
                             "path": rel,
                             "detail": f"tổng giải nén > {MAX_TOTAL_UNCOMPRESSED}"})
            continue

        # 6. Symlink-like entry: external_attr high bits -> mode symlink
        if (info.external_attr >> 16) & 0o170000 == 0o120000:
            findings.append({"code": "zip_symlink", "severity": "ERROR",
                             "path": rel, "detail": "symlink entry bị chặn"})
            continue

        # 7. Chỉ nhận asset dưới assets/ (pack cấu trúc chuẩn)
        if rel in _SKIP_NAMES or not rel.startswith("assets/"):
            findings.append({"code": "zip_skipped_entry", "severity": "WARNING",
                             "path": rel,
                             "detail": "bỏ qua (ngoài assets/ — pack.mcmeta/pack.png do project tự sinh)"})
            continue

        entries.append({"path": rel, "bytes": size})

    # Malformed ZIP — test toàn bộ CRC khi inspect (mục 13.2 malformed)
    # testzip đọc + kiểm CRC không ghi gì ra disk.
    try:
        bad = zf.testzip()
        if bad is not None:
            findings.append({"code": "zip_malformed", "severity": "ERROR",
                             "path": bad, "detail": "CRC không khớp"})
    except Exception as e:
        findings.append({"code": "zip_malformed", "severity": "ERROR",
                         "path": "", "detail": f"ZIP hỏng: {e}"})

    errors = [f for f in findings if f["severity"] == "ERROR"]
    return {"ok": not errors, "findings": findings, "entries": entries,
            "totals": {"files": len(entries), "bytes": total_bytes}}


# ----------------------------------------------------------------------
# Import — staging + merge + rollback (mục 13.1, 14)
# ----------------------------------------------------------------------

def import_zip(ctx: AppContext, project_id: str, zip_path: Path,
               policy: str = "keep",
               conflict_overrides: dict[str, str] | None = None) -> dict:
    """Import ZIP vào project. Staging dir trước, merge sau — rollback an toàn.

    Args:
        policy: "keep" | "replace" | "keep_both" (mục 93).
        conflict_overrides: {rel_path: policy} — per-file wins over global.
    """
    if policy not in _CONFLICT_POLICIES:
        raise ZipInvalidError(f"Policy không hợp lệ: {policy!r}")
    overrides = conflict_overrides or {}

    rs = ctx.get("resource_studio")
    rs.projects.dir_of(project_id)          # validate + tồn tại
    pdir = rs.projects.dir_of(project_id)
    generated = pdir / "generated"

    zip_path = Path(zip_path)
    if not zip_path.is_file():
        raise AntaresError(codes.FILE_NOT_FOUND, "ZIP not found")

    # Phase A — inspect (không ghi gì)
    with zipfile.ZipFile(zip_path) as zf:
        report = inspect_zip(zf)
        if not report["ok"]:
            errors = [f for f in report["findings"] if f["severity"] == "ERROR"]
            raise ZipSecurityError(
                f"ZIP bị chặn: {errors[0]['code']} tại {errors[0]['path'] or '?'} "
                f"({errors[0]['detail']})")

        # Phase B — extract vào staging (data/tmp, KHÔNG đụng generated/)
        staging = ctx.paths.data / "tmp" / f"zip-import-{uuid.uuid4().hex[:12]}"
        staging.mkdir(parents=True, exist_ok=True)
        imported: list[str] = []
        conflicts: list[dict] = []
        applied_policy: dict[str, str] = {}
        try:
            for entry in report["entries"]:
                rel = entry["path"]
                data = zf.read(rel)     # bây giờ mới giải nén entry này
                # re-check size thực tế (central dir có thể nói dối)
                if len(data) != entry["bytes"]:
                    raise ZipSecurityError(
                        f"Kích thước thực khác metadata: {rel}")
                dest = staging / rel
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_bytes(data)
        except Exception:
            shutil.rmtree(staging, ignore_errors=True)
            raise

        # Phase C — merge staging -> generated với conflict policy
        replaced_old: list[tuple[str, Path]] = []   # (rel, backup cũ)
        try:
            for entry in report["entries"]:
                rel = entry["path"]
                src = staging / rel
                dest = generated / rel
                exists = dest.is_file()
                if exists:
                    # cùng nội dung -> không phải conflict
                    import hashlib
                    same = (hashlib.sha256(src.read_bytes()).hexdigest()
                            == hashlib.sha256(dest.read_bytes()).hexdigest())
                    if same:
                        imported.append(rel)
                        continue
                    per_file = overrides.get(rel, policy)
                    if per_file == "keep":
                        conflicts.append({"path": rel, "resolution": "kept_existing"})
                        continue
                    applied_policy[rel] = per_file
                    if per_file == "keep_both":
                        # rename -imported (mục 93: keep both)
                        alt = rel.replace(".png", "-imported.png") \
                            if rel.endswith(".png") else \
                            rel.replace(".json", "-imported.json")
                        dest = generated / alt
                        conflicts.append({"path": rel, "resolution": "kept_both",
                                          "newPath": alt})
                    else:
                        # replace: backup file cũ để rollback được (mục 13.2)
                        backup = staging / ".old" / rel
                        backup.parent.mkdir(parents=True, exist_ok=True)
                        shutil.copy2(dest, backup)
                        replaced_old.append((rel, backup))
                        conflicts.append({"path": rel, "resolution": "replaced"})
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.move(str(src), str(dest))
                imported.append(dest.relative_to(generated).as_posix())

            # Phase D — ghi nhận vào project.assets (mục 5.1)
            import json as _json
            from core.config.writer import write_json_atomic
            pj_path = pdir / "project.json"
            project = _json.loads(pj_path.read_text(encoding="utf-8"))
            existing = {a.get("path"): a for a in (project.get("assets") or [])}
            for rel in imported:
                p = generated / rel
                if p.is_file():
                    import hashlib
                    existing[rel] = {
                        "assetId": None,
                        "sha256": hashlib.sha256(p.read_bytes()).hexdigest(),
                        "path": rel, "source": "zip-import",
                        "assignedAt": time.time(),
                    }
            project["assets"] = list(existing.values())
            project["updatedAt"] = time.time()
            write_json_atomic(pj_path, project)
        except Exception:
            # Rollback: xoá file mới merge + phục hồi file bị replace
            _rollback_imported(generated, imported, replaced_old)
            raise
        finally:
            shutil.rmtree(staging, ignore_errors=True)

    logger.info("Imported %d entries từ %s vào %s (%d conflicts)",
                len(imported), zip_path.name, project_id, len(conflicts))
    return {"imported": imported, "conflicts": conflicts,
            "findings": report["findings"],
            "totals": report["totals"],
            "overridden": applied_policy}


def _rollback_imported(generated: Path, imported: list[str],
                       replaced_old: list[tuple[str, Path]]) -> None:
    """Hoàn tác merge: xoá file mới, phục hồi file bị replace từ backup."""
    for rel in imported:
        # rel có thể là path keep_both (đã đổi tên) — chỉ xoá nếu tồn tại
        candidate = generated / rel
        try:
            if candidate.is_file():
                candidate.unlink()
        except OSError:
            pass
    for rel, backup in replaced_old:
        try:
            if backup.is_file():
                dest = generated / rel
                dest.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(backup, dest)
        except OSError:
            pass
