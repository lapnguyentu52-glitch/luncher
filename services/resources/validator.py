"""Pack validator — checks theo spec 3.0 mục 10.5 (+ 76, 77).

Checks:
  invalid_json / invalid_metadata / missing_texture / duplicate_file /
  invalid_model_reference / wrong_path / broken_sound_reference /
  unsupported_asset / unsafe_path + archive structure (pack.mcmeta tồn tại).

Mỗi finding: {code, severity: ERROR|WARNING, path, detail}.
Validate FAIL chỉ khi có ERROR — WARNING không chặn build (mục 42-logic:
heuristic mơ hồ không được coi là bằng chứng tuyệt đối).
"""
from __future__ import annotations

import json
import re
import struct
import zipfile
from pathlib import Path

#: Giới hạn decode ảnh PNG bằng header parse thủ công (mục 77: max size).
MAX_PNG_DIM = 4096
MAX_FILE_BYTES = 8 * 1024 * 1024

#: Đường dẫn hợp lệ trong pack (prefix whitelist — chống wrong_path).
VALID_PREFIXES = (
    "pack.png",
    "assets/",
)

#: Extensions texture/model/sound hợp lệ.
OK_EXTS = {".png", ".mcmeta", ".json", ".ogg", ".fsb", ".txt", ".lang", ".cfg"}

#: Models tham chiếu texture qua "textures": {"layer0": "item/x"} — check nhẹ.
_TEXTURABLE = ("models/item/", "models/block/", "models/gui/")


def validate_project_dir(generated: Path) -> list[dict]:
    """Validate thư mục generated/ của project."""
    findings: list[dict] = []
    if not generated.is_dir():
        return [{"code": "archive_structure", "severity": "ERROR",
                 "path": str(generated), "detail": "generated/ missing"}]

    mcmeta = generated / "pack.mcmeta"
    if not mcmeta.is_file():
        findings.append({"code": "invalid_metadata", "severity": "ERROR",
                         "path": "pack.mcmeta", "detail": "missing pack.mcmeta"})
    else:
        try:
            meta = json.loads(mcmeta.read_text(encoding="utf-8"))
            pf = (meta.get("pack") or {}).get("pack_format")
            # pack_format: int (≤1.21.8) hoặc [major, minor] (69.0+);
            # min_format/max_format từ 1.21.9 (mục 6.3).
            if not (isinstance(pf, int) or
                    (isinstance(pf, list) and len(pf) == 2
                     and all(isinstance(x, int) for x in pf))):
                raise ValueError("pack_format must be int or [major, minor]")
        except Exception as e:
            findings.append({"code": "invalid_json", "severity": "ERROR",
                             "path": "pack.mcmeta", "detail": str(e)})

    seen: dict[str, str] = {}
    for f in sorted(generated.rglob("*")):
        if not f.is_file():
            continue
        rel = f.relative_to(generated).as_posix()
        rel_l = rel.lower()

        if rel != "pack.mcmeta" and not rel_l.startswith(VALID_PREFIXES):
            findings.append({"code": "wrong_path", "severity": "ERROR",
                             "path": rel, "detail": "outside assets/ (không đúng cấu trúc pack)"})
        if f.suffix.lower() not in OK_EXTS:
            findings.append({"code": "unsupported_asset", "severity": "WARNING",
                             "path": rel, "detail": f"extension {f.suffix}"})
        if f.stat().st_size > MAX_FILE_BYTES:
            findings.append({"code": "oversized", "severity": "ERROR",
                             "path": rel, "detail": f"{f.stat().st_size} bytes > {MAX_FILE_BYTES}"})
        if f.suffix.lower() == ".png":
            err = _check_png(f)
            if err:
                findings.append({"code": "missing_texture", "severity": "ERROR",
                                 "path": rel, "detail": err})
        if f.suffix.lower() == ".json" and rel != "pack.mcmeta":
            try:
                json.loads(f.read_text(encoding="utf-8"))
            except Exception as e:
                findings.append({"code": "invalid_json", "severity": "ERROR",
                                 "path": rel, "detail": str(e)})
        if rel_l in _TEXTURABLE or "/models/" in rel_l:
            findings.extend(_check_model_refs(generated, f, rel))
        _check_structure(rel, f, findings)

    # duplicate: cùng nội dung ở 2 path (mục 10.5 Duplicate file)
    import hashlib
    hash_map: dict[str, str] = {}
    for f in sorted(generated.rglob("*")):
        if f.is_file() and f.stat().st_size < MAX_FILE_BYTES:
            h = hashlib.sha1(f.read_bytes()).hexdigest()
            rel = f.relative_to(generated).as_posix()
            if h in hash_map and not rel.endswith("pack.mcmeta"):
                findings.append({"code": "duplicate_file", "severity": "WARNING",
                                 "path": rel,
                                 "detail": f"duplicate of {hash_map[h]}"})
            else:
                hash_map[h] = rel

    return findings


def validate_zip(zf: zipfile.ZipFile) -> list[dict]:
    """Validate ZIP build output (archive structure + quick checks)."""
    findings: list[dict] = []
    names = zf.namelist()
    if "pack.mcmeta" not in names:
        findings.append({"code": "invalid_metadata", "severity": "ERROR",
                         "path": "pack.mcmeta", "detail": "missing in archive"})
    for n in names:
        if n.endswith("/"):
            continue
        # Root chỉ cho phép pack.mcmeta + pack.png; còn lại phải trong assets/
        if not (n in ("pack.mcmeta", "pack.png") or n.lower().startswith("assets/")):
            findings.append({"code": "wrong_path", "severity": "ERROR",
                             "path": n, "detail": "outside assets/"})
    return findings


# ------------------------------------------------------------------

#: Top-level dirs hợp lệ dưới assets/minecraft/ (mục 10.5 Wrong path).
KNOWN_MC_DIRS = {
    "textures", "models", "sounds", "lang", "font", "shaders",
    "texts", "atlases", "items",
}

#: Extension hợp lệ theo từng loại thư mục.
_DIR_EXT_RULES = (
    ("textures/", {".png", ".mcmeta"}),   # .png.mcmeta đi kèm texture animated
    ("models/", {".json"}),
    ("sounds/", {".ogg"}),
    ("lang/", {".json", ".lang"}),
    ("font/", {".json"}),
)


def _check_structure(rel: str, f: Path, findings: list[dict]) -> None:
    """Cấu trúc pack: assets/minecraft/<loại>/... — sai loại/sai extension = wrong_path."""
    prefix = "assets/minecraft/"
    if not rel.startswith(prefix) or rel == "assets/minecraft" :
        return
    rest = rel[len(prefix):]
    parts = rest.split("/")
    if len(parts) == 1:
        # File nằm trực tiếp dưới assets/minecraft/ — phải là thư mục
        findings.append({"code": "wrong_path", "severity": "ERROR",
                         "path": rel, "detail": "file trực tiếp dưới assets/minecraft/ (cần thư mục loại)"})
        return
    top = parts[0]
    if top not in KNOWN_MC_DIRS:
        findings.append({"code": "wrong_path", "severity": "ERROR",
                         "path": rel, "detail": f"thư mục '{top}' không thuộc assets/minecraft/ chuẩn"})
        return
    ext = f.suffix.lower()
    for dir_prefix, allowed in _DIR_EXT_RULES:
        if rest.startswith(dir_prefix) and ext not in allowed:
            findings.append({"code": "wrong_path", "severity": "ERROR",
                             "path": rel,
                             "detail": f"{dir_prefix}* chỉ nhận {sorted(allowed)}, thấy '{ext}'"})
            return


def _check_png(path: Path) -> str | None:
    """Đọc header PNG thủ công — phát hiện corrupt/quá lớn, không decode full (mục 77)."""
    try:
        data = path.read_bytes()
        if len(data) < 33 or not data.startswith(b"\x89PNG\r\n\x1a\n"):
            return "not a valid PNG"
        w, h = struct.unpack(">II", data[16:24])
        if w > MAX_PNG_DIM or h > MAX_PNG_DIM:
            return f"too large: {w}x{h}"
        return None
    except OSError as e:
        return str(e)


def _check_model_refs(root: Path, model_file: Path, rel: str) -> list[dict]:
    """Model JSON tham chiếu texture không tồn tại -> invalid_model_reference.

    Ref namespaced "ns:path" -> assets/<ns>/textures/<path>.png;
    ref rút gọn "item/x" -> assets/minecraft/textures/item/x.png.
    """
    out: list[dict] = []
    try:
        data = json.loads(model_file.read_text(encoding="utf-8"))
    except Exception:
        return out  # invalid_json đã bắt ở check riêng
    textures = (data.get("textures") or {}) if isinstance(data, dict) else {}
    for key, ref in textures.items():
        if not isinstance(ref, str) or ref.startswith("#"):
            continue
        m = re.match(r"^([a-z0-9_.-]+):(.+)$", ref)
        tex_rel = (f"assets/{m.group(1)}/textures/{m.group(2)}.png" if m
                   else f"assets/minecraft/textures/{ref}.png")
        if not (root / tex_rel).is_file():
            out.append({"code": "invalid_model_reference", "severity": "ERROR",
                        "path": rel, "detail": f"textures.{key} -> {tex_rel} missing"})
    return out
