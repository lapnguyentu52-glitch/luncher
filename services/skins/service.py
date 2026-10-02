"""Skin & Cape Studio — quản lý skin/cape local + áp vào instance (mục 43).

Storage (content-addressed, nhất quán Asset Library — mục 12):
- data/skins/<sha256>.png     — mọi skin/cape file
- data/skin-catalog.json      — metadata list
- data/skin-state.json        — apply state per-instance (phục hồi khi launch)

Skin hợp lệ: PNG RGBA 64x64 (modern) hoặc 64x32 (legacy — chuyển qua 64x64).
Cape hợp lệ: PNG RGBA 92x64 (chuẩn Mojang).
Generate: Steve/Alex-style generator — thân màu đặc, mắt, nụ cười (mục 43.2).
Dùng versioning.py (pack_format) — nhất quán RS v2, không hardcode version.
"""
from __future__ import annotations

import hashlib
import json
import struct
import time
import uuid
import zlib
from pathlib import Path

from app.context import AppContext
from core.config.writer import write_json_atomic
from core.errors import codes
from core.errors.base import AntaresError
from core.events import names as ev
from core.logging.setup import get_logger
from services.visuals.atlas import decode_png_rgba

logger = get_logger("skins")

#: Giới hạn nhất quán Asset Library (mục 12.2).
MAX_FILE_BYTES = 8 * 1024 * 1024

_SKIN_DIMS = {(64, 64), (64, 32)}      # modern / legacy
_CAPE_DIMS = {(92, 64), (64, 32)}      # chuẩn Mojang / size legacy OptiFine
_PNG_SIG = b"\x89PNG\r\n\x1a\n"
_NAME_RE_TEMPLATE = r"^[a-z0-9][a-z0-9_-]{0,%d}$"


class SkinCapeService:
    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx

    # ------------------------------------------------------------------
    # Paths
    # ------------------------------------------------------------------

    @property
    def _catalog_path(self) -> Path:
        return self._ctx.paths.data / "skin-catalog.json"

    @property
    def _state_path(self) -> Path:
        return self._ctx.paths.data / "skin-state.json"

    def _files_dir(self) -> Path:
        d = self._ctx.paths.data / "skins"
        d.mkdir(parents=True, exist_ok=True)
        return d

    # ------------------------------------------------------------------
    # Catalog + state
    # ------------------------------------------------------------------

    def _load_catalog(self) -> dict:
        path = self._catalog_path
        if not path.is_file():
            return {"skins": [], "capes": []}
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
            if not isinstance(data, dict):
                return {"skins": [], "capes": []}
            data.setdefault("skins", [])
            data.setdefault("capes", [])
            return data
        except Exception:
            return {"skins": [], "capes": []}

    def _save_catalog(self, catalog: dict) -> None:
        write_json_atomic(self._catalog_path, catalog)

    def _load_state(self) -> dict:
        path = self._state_path
        if not path.is_file():
            return {}
        try:
            data = json.loads(path.read_text(encoding="utf-8"))
            return data if isinstance(data, dict) else {}
        except Exception:
            return {}

    def _save_state(self, state: dict) -> None:
        write_json_atomic(self._state_path, state)

    # ------------------------------------------------------------------
    # Import (file picker -> base64 -> bytes — nhất quán Asset Library)
    # ------------------------------------------------------------------

    def import_skin(self, data: bytes, name: str = "", *,
                    auto_upgrade: bool = True) -> dict:
        entry = self._import(data, name, kind="skin", dims=_SKIN_DIMS,
                             auto_upgrade=auto_upgrade)
        return entry

    def import_cape(self, data: bytes, name: str = "") -> dict:
        return self._import(data, name, kind="cape", dims=_CAPE_DIMS,
                            auto_upgrade=False)

    def _import(self, data: bytes, name: str, *, kind: str, dims: set,
                auto_upgrade: bool) -> dict:
        if not isinstance(data, bytes) or not data:
            raise AntaresError(codes.VALIDATION_FAILED, "Empty payload")
        if len(data) > MAX_FILE_BYTES:
            raise AntaresError(codes.ASSET_TOO_LARGE,
                               f"File > {MAX_FILE_BYTES // (1024 * 1024)}MB")
        w, h = self._png_dims(data)          # validate signature + IHDR
        if (w, h) not in dims:
            allowed = " hoặc ".join(f"{a}x{b}" for a, b in sorted(dims))
            raise AntaresError(codes.VALIDATION_FAILED,
                               f"{kind} phải là PNG {allowed} (nhận {w}x{h})")
        rgba, ww, hh = self._decode(data)

        converted = False
        if kind == "skin" and (ww, hh) == (64, 32) and auto_upgrade:
            data = self._legacy_to_modern(rgba, ww, hh)
            w, h = 64, 64
            converted = True

        sha = hashlib.sha256(data).hexdigest()
        files = self._files_dir()
        asset_path = files / f"{sha}.png"
        if not asset_path.exists():
            tmp = files / f".tmp-{uuid.uuid4().hex}"
            tmp.write_bytes(data)
            tmp.replace(asset_path)          # atomic move (mục 14)

        catalog = self._load_catalog()
        entry = {
            "id": uuid.uuid4().hex[:12],
            "kind": kind,
            "name": self._safe_name(name) or f"{kind}-{sha[:8]}",
            "sha256": sha,
            "width": w, "height": h,
            "bytes": len(data),
            "converted": converted,
            "createdAt": time.time(),
        }
        catalog[f"{kind}s"].append(entry)
        self._save_catalog(catalog)
        self._ctx.events.publish(ev.SKINS_CHANGED, {"action": "import",
                                                    f"{kind}Id": entry["id"]})
        logger.info("Imported %s %s (%dx%d%s)", kind, entry["name"], w, h,
                    ", legacy 64x32 -> modern" if converted else "")
        return entry

    # ------------------------------------------------------------------
    # Generate — Steve/Alex-style (mục 43.2): thân màu đặc + mắt + miệng
    # ------------------------------------------------------------------

    def generate(self, kind: str, name: str, base_color: str,
                 accent_color: str = "#3b2a1a") -> dict:
        """Tạo skin 'Steve-style' 64x64 hoặc cape màu đặc 92x64.

        base_color: hex RRGGBB thân/áo. accent_color: màu tóc/đai.
        Canvas 64x64 chuẩn UV: head 8x8, body 8x12, arms/legs 4x12.
        """
        kind = str(kind or "").lower()
        if kind not in ("skin", "cape"):
            raise AntaresError(codes.VALIDATION_FAILED, f"Unknown kind: {kind}")
        name = self._safe_name(name)
        if not name:
            raise AntaresError(codes.VALIDATION_FAILED, "Name required")
        rgb = self._hex_to_rgb(base_color, "base_color")
        accent = self._hex_to_rgb(accent_color, "accent_color")

        if kind == "skin":
            w = h = 64
            px = bytearray(w * h * 4)
            # Head front (8x8) tại (8,8): da màu base sáng hơn 10%
            face = self._shade(rgb, 1.1)
            self._rect(px, w, 8, 8, 8, 8, face)
            # Mắt (2px trắng + 1px accent mỗi bên) — hàng y=12
            self._rect(px, w, 9, 12, 2, 1, (255, 255, 255, 255))
            self._rect(px, w, 13, 12, 1, 1, (*accent, 255))
            self._rect(px, w, 11, 12, 1, 1, (*accent, 255))
            self._rect(px, w, 12, 12, 2, 1, (255, 255, 255, 255))
            # Miệng 2px y=14
            self._rect(px, w, 11, 14, 2, 1, self._shade(accent, 0.7))
            self._rect(px, w, 12, 14, 2, 1, self._shade(accent, 0.7))
            # Tóc: hàng trên của head top (8x8) tại (0,0) + viền front
            hair = self._shade(accent, 0.8)
            self._rect(px, w, 0, 0, 8, 8, hair)
            self._rect(px, w, 8, 8, 8, 2, hair)
            # Body (8x12) tại (16,16): áo màu base
            self._rect(px, w, 16, 16, 8, 12, rgb)
            # Arms (4x12) tại (40,16) & (32,48): tay áo + da
            self._rect(px, w, 40, 16, 4, 12, rgb)
            self._rect(px, w, 32, 48, 4, 12, rgb)
            self._rect(px, w, 40, 20, 4, 8, face)   # da tay
            self._rect(px, w, 32, 52, 4, 8, face)
            # Legs (4x12) tại (16,48) & (0,16): quần tối hơn 25%
            pants = self._shade(rgb, 0.75)
            self._rect(px, w, 16, 48, 4, 12, pants)
            self._rect(px, w, 0, 16, 4, 12, pants)
            data = encode_png(w, h, bytes(px))
        else:
            w, h = 92, 64
            px = bytearray(w * h * 4)
            self._rect(px, w, 12, 2, 10, 16, rgb)    # thân cape chính
            self._rect(px, w, 12, 18, 10, 30, self._shade(rgb, 0.85))
            self._rect(px, w, 12, 2, 10, 2, accent)  # đai cổ
            data = encode_png(w, h, bytes(px))

        sha = hashlib.sha256(data).hexdigest()
        files = self._files_dir()
        asset_path = files / f"{sha}.png"
        if not asset_path.exists():
            tmp = files / f".tmp-{uuid.uuid4().hex}"
            tmp.write_bytes(data)
            tmp.replace(asset_path)

        catalog = self._load_catalog()
        entry = {
            "id": uuid.uuid4().hex[:12],
            "kind": kind,
            "name": name,
            "sha256": sha,
            "width": w, "height": h,
            "bytes": len(data),
            "generated": True,
            "createdAt": time.time(),
        }
        catalog[f"{kind}s"].append(entry)
        self._save_catalog(catalog)
        self._ctx.events.publish(ev.SKINS_CHANGED, {"action": "generate",
                                                    f"{kind}Id": entry["id"]})
        logger.info("Generated %s %s", kind, name)
        return entry

    # ------------------------------------------------------------------
    # Query / delete
    # ------------------------------------------------------------------

    def list(self, kind: str | None = None) -> dict:
        catalog = self._load_catalog()
        out = {"skins": catalog["skins"], "capes": catalog["capes"]}
        if kind in ("skin", "cape"):
            out = {f"{kind}s": catalog[f"{kind}s"]}
        return out

    def get(self, kind: str, item_id: str) -> dict | None:
        catalog = self._load_catalog()
        for e in catalog[f"{kind}s"]:
            if e["id"] == item_id:
                return e
        return None

    def delete(self, kind: str, item_id: str, *, confirm: bool = False) -> bool:
        if not confirm:
            raise AntaresError(codes.VALIDATION_FAILED,
                               "Delete requires explicit confirmation")
        catalog = self._load_catalog()
        items = catalog[f"{kind}s"]
        entry = next((e for e in items if e["id"] == item_id), None)
        if not entry:
            return False
        # Chặn xoá item đang áp trên instance nào
        state = self._load_state()
        for iid, applied in state.items():
            if any(a.get(f"{kind}Id") == item_id for a in
                   (applied if isinstance(applied, list) else [applied])):
                raise AntaresError(codes.VALIDATION_FAILED,
                                   f"{kind} đang áp trên instance — unapply trước")
        catalog[f"{kind}s"] = [e for e in items if e["id"] != item_id]
        self._save_catalog(catalog)
        self._ctx.events.publish(ev.SKINS_CHANGED, {"action": "delete",
                                                    f"{kind}Id": item_id})
        return True

    def png_data_uri(self, kind: str, item_id: str) -> str | None:
        """Data URI PNG cho preview (đọc file content-addressed)."""
        entry = self.get(kind, item_id)
        if not entry:
            return None
        path = self._files_dir() / f"{entry['sha256']}.png"
        if not path.is_file():
            return None
        import base64
        return "data:image/png;base64," + base64.b64encode(path.read_bytes()).decode()

    # ------------------------------------------------------------------
    # Apply per-instance — nguồn sự thật cho launch pipeline (mục 43.3)
    # ------------------------------------------------------------------

    def apply(self, instance_id: str, kind: str, item_id: str | None) -> dict:
        """Ghi apply state (None = gỡ). Launch pipeline đọc state này."""
        if kind not in ("skin", "cape"):
            raise AntaresError(codes.VALIDATION_FAILED, f"Unknown kind: {kind}")
        if not self._ctx.get("instances").get(instance_id):
            raise AntaresError(codes.INSTANCE_NOT_FOUND, "Instance not found")
        if item_id is not None and self.get(kind, item_id) is None:
            raise AntaresError(codes.PROFILE_NOT_FOUND, f"{kind} not found: {item_id}")
        state = self._load_state()
        slot = state.setdefault(instance_id, {})
        slot[f"{kind}Id"] = item_id
        slot[f"{kind}At"] = time.time() if item_id else None
        self._save_state(state)
        self._ctx.events.publish(ev.SKIN_APPLIED,
                                 {"instanceId": instance_id, "kind": kind,
                                  "itemId": item_id})
        return {"instanceId": instance_id, "kind": kind, "itemId": item_id}

    def applied(self, instance_id: str) -> dict:
        """State apply của instance — launch pipeline đọc khi bấm Play."""
        state = self._load_state()
        slot = state.get(instance_id) or {}
        out: dict = {}
        for kind in ("skin", "cape"):
            item_id = slot.get(f"{kind}Id")
            if not item_id:
                continue
            entry = self.get(kind, item_id)
            if entry:
                path = self._files_dir() / f"{entry['sha256']}.png"
                out[kind] = {**entry, "file": str(path)}
        return out

    def unapply(self, instance_id: str, kind: str) -> dict:
        return self.apply(instance_id, kind, None)

    # ------------------------------------------------------------------
    # PNG helpers
    # ------------------------------------------------------------------

    def _png_dims(self, data: bytes) -> tuple[int, int]:
        if not data.startswith(_PNG_SIG):
            raise AntaresError(codes.VALIDATION_FAILED, "Không phải PNG hợp lệ")
        if len(data) < 33 or data[12:16] != b"IHDR":
            raise AntaresError(codes.ASSET_INVALID, "PNG header không hợp lệ")
        import struct
        w, h = struct.unpack(">II", data[16:24])
        if w == 0 or h == 0 or w > 4096 or h > 4096:
            raise AntaresError(codes.VALIDATION_FAILED,
                               f"Kích thước {w}x{h} ngoài giới hạn")
        return w, h

    def _decode(self, data: bytes) -> tuple[bytes, int, int]:
        try:
            w, h, rgba = decode_png_rgba(data)
            return rgba, w, h
        except Exception as e:
            raise AntaresError(codes.VALIDATION_FAILED,
                               f"PNG decode thất bại: {e}") from e

    def _legacy_to_modern(self, rgba: bytes, w: int, h: int) -> bytes:
        """64x32 -> 64x64: mirror cột arm/leg + alpha dưới = 0 (mục 43.1)."""
        import struct
        nw, nh = 64, 64
        out = bytearray(nw * nh * 4)
        for y in range(h):
            for x in range(w):
                si = (y * w + x) * 4
                di = (y * nw + x) * 4
                out[di:di + 4] = rgba[si:si + 4]
        # Mirror cột phải của leg (old leg 16x16 tại (0,16) -> mirror vào (0,48))
        for y in range(16, 32):
            for x in range(0, 16):
                si = (y * w + x) * 4
                di = ((y + 32) * nw + x) * 4
                out[di:di + 4] = rgba[si:si + 4]
        # Mirror arm (old arm 16x16 tại (40,16)) -> (40,48)
        for y in range(16, 32):
            for x in range(40, 56):
                si = (y * w + x) * 4
                di = ((y + 32) * nw + x) * 4
                out[di:di + 4] = rgba[si:si + 4]
        return encode_png(nw, nh, bytes(out))

    def _safe_name(self, name: str) -> str:
        import re
        name = str(name or "").strip().lower().replace(" ", "_")
        return name if re.match(_NAME_RE_TEMPLATE % 63, name) else ""

    def _hex_to_rgb(self, s: str, field: str) -> tuple[int, int, int]:
        import re
        s = str(s or "").strip().lstrip("#")
        if not re.match(r"^[0-9a-fA-F]{6}$", s):
            raise AntaresError(codes.VALIDATION_FAILED,
                               f"{field} phải là hex RRGGBB")
        return int(s[0:2], 16), int(s[2:4], 16), int(s[4:6], 16)

    @staticmethod
    def _shade(c: tuple[int, int, int], f: float) -> tuple[int, int, int]:
        return (max(0, min(255, int(c[0] * f))),
                max(0, min(255, int(c[1] * f))),
                max(0, min(255, int(c[2] * f))))

    @staticmethod
    def _rect(px: bytearray, w: int, x: int, y: int, rw: int, rh: int,
              color) -> None:
        r, g, b = color[0], color[1], color[2]
        a = color[3] if len(color) > 3 else 255
        for yy in range(y, y + rh):
            for xx in range(x, x + rw):
                i = (yy * w + xx) * 4
                px[i] = r
                px[i + 1] = g
                px[i + 2] = b
                px[i + 3] = a


# ------------------------------------------------------------------
# PNG encoder (stdlib zlib+struct — không Pillow)
# ------------------------------------------------------------------

def encode_png(width: int, height: int, rgba: bytes) -> bytes:
    raw = b"".join(
        b"\x00" + rgba[y * width * 4:(y + 1) * width * 4]
        for y in range(height)
    )

    def chunk(tag: bytes, payload: bytes) -> bytes:
        return (struct.pack(">I", len(payload)) + tag + payload
                + struct.pack(">I", zlib.crc32(tag + payload) & 0xFFFFFFFF))

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr)
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))
