"""Asset Library (master plan v3 mục 12 — Batch 5).

Nguồn asset chính cho Resource Studio + Totem 3D (mục 40): import PNG, hash,
catalog, search/filter, face assignment cho model 3D.

Security (mục 12.2 + 23):
- Không trust extension — check MIME + PNG signature + IHDR (decode fail-safe
  từ atlas.decode_png_rgba, mục 12.2 malformed PNG).
- Giới hạn: 8 MB/file, 4096px (decision Batch 0; khớp validator pack).
- Không ghi ra path tùy ý: filename chỉ [a-z0-9_-]; asset files nằm trong
  data/assets/<sha256><.png> — content-addressed, traversal không thể xảy ra
  vì filename không bao giờ chạm filesystem raw.
- Duplicate import cùng content -> reuse (hash trùng), không ghi 2 lần.
- Catalog metadata: data/asset-catalog.json (atomic write; mục 14).
"""
from __future__ import annotations

import hashlib
import json
import re
import struct
import time
import uuid
from pathlib import Path
from typing import Any

from app.context import AppContext
from core.config.writer import write_json_atomic
from core.errors import codes
from core.errors.base import AntaresError
from core.logging.setup import get_logger
from services.visuals.atlas import AtlasError, decode_png_rgba

logger = get_logger("resources.assets")

#: Giới hạn (decision Batch 0 — mục 34)
MAX_FILE_BYTES = 8 * 1024 * 1024
MAX_DIM = 4096

_CATEGORIES = ("block", "item", "entity", "gui", "particle", "font", "sound", "lang")

_NAME_RE = re.compile(r"^[a-z0-9][a-z0-9_-]{0,63}$")

#: PNG signature + tối thiểu header
_PNG_SIG = b"\x89PNG\r\n\x1a\n"


def categories() -> tuple[str, ...]:
    """8 category chuẩn mục 12.3."""
    return _CATEGORIES


class AssetStore:
    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx

    # ------------------------------------------------------------------
    # Paths
    # ------------------------------------------------------------------

    @property
    def _catalog_path(self) -> Path:
        return self._ctx.paths.data / "asset-catalog.json"

    def _files_dir(self) -> Path:
        d = self._ctx.paths.data / "assets"
        d.mkdir(parents=True, exist_ok=True)
        return d

    # ------------------------------------------------------------------
    # Catalog IO
    # ------------------------------------------------------------------

    def _load_catalog(self) -> dict:
        p = self._catalog_path
        if not p.is_file():
            return {"schemaVersion": 1, "assets": []}
        try:
            data = json.loads(p.read_text(encoding="utf-8"))
        except Exception:
            logger.warning("Corrupt asset catalog — backup + reset")
            try:
                (p.with_suffix(".json.corrupt")).write_bytes(p.read_bytes())
            except Exception:
                pass
            return {"schemaVersion": 1, "assets": []}
        if not isinstance(data, dict) or not isinstance(data.get("assets"), list):
            return {"schemaVersion": 1, "assets": []}
        return data

    def _save_catalog(self, catalog: dict) -> None:
        write_json_atomic(self._catalog_path, catalog)

    # ------------------------------------------------------------------
    # Import (mục 12.1 flow)
    # ------------------------------------------------------------------

    def import_png(self, data: bytes, name: str | None = None,
                   category: str = "item", tags: list[str] | None = None) -> dict:
        """Import 1 PNG -> validate -> hash -> lưu file + metadata (mục 12.1).

        Raise AntaresError với code rõ ràng cho mọi validation fail.
        """
        # 1. Size check (trước cả decode — mục 12.2 huge file)
        if not isinstance(data, (bytes, bytearray)) or len(data) == 0:
            raise AntaresError(codes.VALIDATION_FAILED, "Asset data trống")
        if len(data) > MAX_FILE_BYTES:
            raise AntaresError(codes.ASSET_TOO_LARGE,
                               f"Asset quá lớn: {len(data)} > {MAX_FILE_BYTES} bytes")

        # 2. MIME/signature check (mục 12.2: không trust extension)
        if not data.startswith(_PNG_SIG):
            raise AntaresError(codes.VALIDATION_FAILED,
                               "Không phải PNG hợp lệ (signature sai)")

        # 3. Dimension check qua header thủ công (nhanh, trước decode đầy đủ)
        #    Layout PNG: sig(8) + len(4) + 'IHDR'(4) + IHDR body(13)...
        #    Header khai báo dims lớn -> chặn TRƯỚC decode để không tốn RAM.
        if len(data) < 33 or data[12:16] != b"IHDR":
            raise AntaresError(codes.ASSET_INVALID, "PNG header không hợp lệ")
        w, h = struct.unpack(">II", data[16:24])
        if w > MAX_DIM or h > MAX_DIM or w == 0 or h == 0:
            raise AntaresError(codes.VALIDATION_FAILED,
                               f"Kích thước {w}x{h} ngoài giới hạn (max {MAX_DIM}px)")

        # 4. Decode fail-safe (malformed IDAT/filter -> lỗi rõ ràng)
        try:
            decode_png_rgba(data)
        except AtlasError as e:
            raise AntaresError(codes.VALIDATION_FAILED,
                               f"PNG decode thất bại: {e}") from e

        # 5. SHA256 + content-addressed file (mục 12.1)
        sha = hashlib.sha256(data).hexdigest()
        files = self._files_dir()
        asset_path = files / f"{sha}.png"
        if not asset_path.exists():
            tmp = files / f".tmp-{uuid.uuid4().hex}"
            tmp.write_bytes(data)
            tmp.replace(asset_path)     # atomic move trên cùng filesystem

        # 6. Metadata record
        name_clean = self._safe_name(name) or f"asset-{sha[:8]}"
        catalog = self._load_catalog()
        # Duplicate content: reuse — không tạo record thứ 2 cùng hash trỏ
        # file đích (vẫn cho phép nhiều entry khác path/target mục đích dùng)
        asset_id = uuid.uuid4().hex[:12]
        now = time.time()
        entry = {
            "id": asset_id,
            "name": name_clean,
            "sha256": sha,
            "mime": "image/png",
            "width": w,
            "height": h,
            "bytes": len(data),
            "category": self._safe_category(category),
            "tags": [self._safe_tag(tg) for tg in (tags or []) if self._safe_tag(tg)][:16],
            "source": "imported",
            "createdAt": now,
            "updatedAt": now,
        }
        catalog["assets"].append(entry)
        self._save_catalog(catalog)
        logger.info("Imported asset %s (%dx%d, %d bytes)", name_clean, w, h, len(data))
        return entry

    # ------------------------------------------------------------------
    # Query (mục 12.4 search/filter/sort)
    # ------------------------------------------------------------------

    def list(self, *, query: str = "", category: str = "", tag: str = "",
             sort: str = "newest") -> list[dict]:
        catalog = self._load_catalog()
        assets = catalog.get("assets", [])
        q = (query or "").strip().lower()
        out = []
        for a in assets:
            if q and q not in a.get("name", "").lower() \
                    and q not in a.get("sha256", "")[:12] \
                    and not any(q in tg.lower() for tg in a.get("tags", [])):
                continue
            if category and a.get("category") != category:
                continue
            if tag and tag not in a.get("tags", []):
                continue
            out.append(a)
        if sort == "newest":
            out.sort(key=lambda a: a.get("createdAt", 0), reverse=True)
        elif sort == "oldest":
            out.sort(key=lambda a: a.get("createdAt", 0))
        elif sort == "name":
            out.sort(key=lambda a: a.get("name", ""))
        elif sort == "size":
            out.sort(key=lambda a: a.get("bytes", 0), reverse=True)
        return out

    def get(self, asset_id: str) -> dict:
        for a in self._load_catalog().get("assets", []):
            if a.get("id") == asset_id:
                return a
        raise AntaresError(codes.FILE_NOT_FOUND, f"Asset not found: {asset_id}")

    def read_png(self, asset_id: str) -> bytes:
        """PNG bytes của asset — path content-addressed (traversal-safe)."""
        asset = self.get(asset_id)
        sha = asset.get("sha256", "")
        if not re.match(r"^[0-9a-f]{64}$", sha):
            raise AntaresError(codes.VALIDATION_FAILED, "Invalid asset hash")
        p = self._files_dir() / f"{sha}.png"
        if not p.is_file():
            raise AntaresError(codes.FILE_NOT_FOUND, "Asset file missing")
        return p.read_bytes()

    def png_data_uri(self, asset_id: str) -> str:
        import base64
        return "data:image/png;base64," + base64.b64encode(
            self.read_png(asset_id)).decode("ascii")

    # ------------------------------------------------------------------
    # Delete + assign
    # ------------------------------------------------------------------

    def delete(self, asset_id: str) -> bool:
        """Xoá metadata record; file theo hash giữ lại (GC sau khi không ref)."""
        catalog = self._load_catalog()
        before = len(catalog["assets"])
        catalog["assets"] = [a for a in catalog["assets"] if a.get("id") != asset_id]
        if len(catalog["assets"]) == before:
            raise AntaresError(codes.FILE_NOT_FOUND, f"Asset not found: {asset_id}")
        self._save_catalog(catalog)
        return True

    def assign_to_project(self, project_id: str, asset_id: str,
                          target_rel: str) -> dict:
        """Gán asset vào project — copy PNG vào generated/ tại target path.

        target_rel phải nằm trong assets/<ns>/textures/ (chống wrong_path).
        Đây là nguồn path cho face assignment + model compile.
        """
        from services.resources.project import ProjectStore
        store = ProjectStore(self._ctx)
        store.dir_of(project_id)      # validate id + tồn tại (chặn traversal)

        asset = self.get(asset_id)
        rel = (target_rel or "").strip().replace("\\", "/")
        if not rel.startswith("assets/") or not rel.endswith(".png"):
            raise AntaresError(codes.VALIDATION_FAILED,
                               f"Target path phải trong assets/: {rel!r}")
        if ".." in rel or rel.count("//"):
            raise AntaresError(codes.VALIDATION_FAILED, "Invalid target path")
        # phảI nằm trong textures/
        m = re.match(r"^assets/[a-z0-9_-]+/textures/.+\.png$", rel)
        if not m:
            raise AntaresError(codes.VALIDATION_FAILED,
                               f"Target phải dưới textures/: {rel!r}")

        pdir = store.dir_of(project_id)
        dest = pdir / "generated" / rel
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(self.read_png(asset_id))

        # Ghi nhận assignment vào project (mục 5.1: assets array)
        import json as _json
        pj_path = pdir / "project.json"
        project = _json.loads(pj_path.read_text(encoding="utf-8"))
        assets = project.get("assets") or []
        entry = {"assetId": asset["id"], "sha256": asset["sha256"],
                 "path": rel, "assignedAt": time.time()}
        assets = [a for a in assets if a.get("path") != rel] + [entry]
        project["assets"] = assets
        project["updatedAt"] = time.time()
        write_json_atomic(pj_path, project)
        logger.info("Assigned asset %s -> %s (%s)", asset["name"], project_id, rel)
        return {"path": rel, "asset": asset["name"], "sha256": asset["sha256"]}

    # ------------------------------------------------------------------
    # Sanitize helpers
    # ------------------------------------------------------------------

    @staticmethod
    def _safe_name(name: str | None) -> str:
        """Normalize filename (mục 12.2) — [a-z0-9_-], max 64."""
        if not name:
            return ""
        s = name.strip().lower().replace(" ", "-")
        s = re.sub(r"[^a-z0-9_-]", "", s)
        return s[:64]

    @staticmethod
    def _safe_category(category: str) -> str:
        return category if category in _CATEGORIES else "item"

    @staticmethod
    def _safe_tag(tag: str) -> str:
        if not isinstance(tag, str):
            return ""
        return re.sub(r"[^a-z0-9_-]", "", tag.lower())[:24]
