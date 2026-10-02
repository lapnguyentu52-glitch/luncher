"""Atlas packer + PNG decode (master plan v3 mục 8.4, 12.2).

Pipeline mục 8.4: Assets -> Decode -> Validate -> Normalize -> Pack rectangle
-> Atlas PNG -> UV mapping.

Deterministic (mục 8.4 checklist):
- Textures sort theo key trước khi pack — build 2 lần cùng input -> cùng
  output bytes (sha256 giống nhau).
- Shelf first-fit: chiều cao giảm dần, tie-break theo key A-Z.
- Padding policy: PAD = 1px giữa các rect (tránh bleed khi sample lân cận).

PNG decode stdlib-only, fail-safe (mục 12.2):
- Không trust extension — check signature + IHDR.
- Giới hạn kích thước (MAX_DIM) chống decompression bomb.
- Chỉ RGBA 8-bit, filter 0 (đúng format encode_png nội bộ) — PNG lạ bị từ
  chối với lỗi rõ ràng, không crash.
"""
from __future__ import annotations

import struct
import zlib
from typing import Any

#: Atlas max dimension (decision Batch 0 — mục 34)
MAX_ATLAS_DIM = 1024

#: Padding giữa rect (px) — chống texture bleed
PAD = 1

#: Giới hạn decode input (mục 12.2: huge image)
MAX_INPUT_DIM = 4096
MAX_INPUT_BYTES = 8 * 1024 * 1024

PNG_SIGNATURE = b"\x89PNG\r\n\x1a\n"


# ----------------------------------------------------------------------
# PNG decode (stdlib, fail-safe)
# ----------------------------------------------------------------------

class AtlasError(ValueError):
    """Lỗi decode/pack atlas — message an toàn cho UI (mục 68)."""


def decode_png_rgba(data: bytes) -> tuple[int, int, bytes]:
    """PNG 8-bit RGBA (filter 0) -> (w, h, rgba bytes). Raise AtlasError."""
    if not isinstance(data, (bytes, bytearray)):
        raise AtlasError("PNG data phải là bytes")
    data = bytes(data)
    if len(data) > MAX_INPUT_BYTES:
        raise AtlasError(f"PNG quá lớn: {len(data)} bytes > {MAX_INPUT_BYTES}")
    if len(data) < 33 or not data.startswith(PNG_SIGNATURE):
        raise AtlasError("Không phải PNG hợp lệ")

    idx = 8
    ihdr: bytes | None = None
    idat = b""
    while idx + 12 <= len(data):
        ln = struct.unpack(">I", data[idx:idx + 4])[0]
        tag = data[idx + 4:idx + 8]
        if idx + 12 + ln > len(data):
            raise AtlasError("PNG chunk bị cắt cụt")
        body = data[idx + 8:idx + 8 + ln]
        if tag == b"IHDR":
            ihdr = body
        elif tag == b"IDAT":
            idat += body
        elif tag == b"IEND":
            break
        idx += 12 + ln

    if ihdr is None or len(ihdr) < 13:
        raise AtlasError("Thiếu IHDR")
    w, h, depth, ctype, _comp, _filt, _interlace = struct.unpack(">IIBBBBB", ihdr)
    if w == 0 or h == 0:
        raise AtlasError("PNG kích thước 0")
    if w > MAX_INPUT_DIM or h > MAX_INPUT_DIM:
        raise AtlasError(f"PNG quá lớn: {w}x{h} > {MAX_INPUT_DIM}")
    if depth != 8 or ctype != 6:
        raise AtlasError(f"Chỉ hỗ trợ RGBA 8-bit, thấy depth={depth} ctype={ctype}")
    if _interlace != 0:
        raise AtlasError("Không hỗ trợ interlaced PNG")
    if not idat:
        raise AtlasError("Thiếu IDAT")

    try:
        raw = zlib.decompress(idat)
    except zlib.error as e:
        raise AtlasError(f"IDAT hỏng: {e}") from e

    stride = w * 4 + 1
    if len(raw) < stride * h:
        raise AtlasError("Dữ liệu pixel thiếu")
    # Enforce filter 0 (đúng format nội bộ); filter khác -> từ chối rõ ràng
    for y in range(h):
        if raw[y * stride] != 0:
            raise AtlasError(f"Filter {raw[y * stride]} không hỗ trợ (chỉ 0)")
    rgba = bytearray(w * h * 4)
    for y in range(h):
        row = raw[y * stride + 1:(y + 1) * stride]
        rgba[y * w * 4:(y + 1) * w * 4] = row
    return w, h, bytes(rgba)


# ----------------------------------------------------------------------
# Rect packing — shelf first-fit deterministic
# ----------------------------------------------------------------------

def _next_pow2(n: int) -> int:
    p = 1
    while p < n:
        p *= 2
    return p


def pack(entries: dict[str, tuple[int, int]]) -> dict[str, Any]:
    """Pack rects vào atlas — deterministic.

    Args:
        entries: {key: (w, h)} — w/h > 0.
    Returns:
        {"placements": {key: (x, y, w, h)}, "width": W, "height": H}
    Raises:
        AtlasError: rect vượt MAX_ATLAS_DIM hoặc w/h <= 0.
    """
    placements: dict[str, tuple[int, int, int, int]] = {}
    if not entries:
        return {"placements": {}, "width": 0, "height": 0}

    # Sort: chiều cao giảm dần, tie-break key A-Z — kết quả ổn định tuyệt đối
    ordered = sorted(entries.items(), key=lambda kv: (-kv[1][1], kv[0]))

    # Chiều rộng = max w (pow2-friendly nhưng không bắt buộc)
    width = max(w for _k, (w, _h) in ordered)
    if width > MAX_ATLAS_DIM:
        raise AtlasError(f"Texture rộng {width}px > {MAX_ATLAS_DIM}")

    shelves: list[dict] = []      # {"y", "h", "x", "keys"}
    for key, (w, h) in ordered:
        if w <= 0 or h <= 0:
            raise AtlasError(f"Texture '{key}' kích thước {w}x{h} không hợp lệ")
        if w > MAX_ATLAS_DIM or h > MAX_ATLAS_DIM:
            raise AtlasError(f"Texture '{key}' {w}x{h} > {MAX_ATLAS_DIM}")
        placed = False
        for shelf in shelves:
            if h <= shelf["h"] and shelf["x"] + w + PAD <= width:
                placements[key] = (shelf["x"], shelf["y"], w, h)
                shelf["x"] += w + PAD
                placed = True
                break
        if not placed:
            y = 0 if not shelves else shelves[-1]["y"] + shelves[-1]["h"] + PAD
            if y + h > MAX_ATLAS_DIM:
                raise AtlasError(f"Atlas vượt chiều cao {MAX_ATLAS_DIM} "
                                 f"(pack '{key}')")
            if w > width:   # không xảy ra vì width = max(w), giữ an toàn
                raise AtlasError(f"Texture '{key}' rộng hơn atlas")
            shelves.append({"y": y, "h": h, "x": w + PAD})
            placements[key] = (0, y, w, h)

    height = shelves[-1]["y"] + shelves[-1]["h"] if shelves else 0
    return {"placements": placements, "width": width, "height": height}


def build_atlas(textures: dict[str, bytes]) -> dict[str, Any]:
    """{key: PNG bytes} -> atlas PNG + UV map deterministic (mục 8.4).

    Return:
    {
      "png": bytes,                       # atlas RGBA PNG
      "uv": {key: [x1, y1, x2, y2]},      # pixel coords, chuẩn hoá theo w/h
      "width": W, "height": H,
      "placements": {key: (x, y, w, h)},
    }
    """
    if not textures:
        raise AtlasError("Không có texture để pack atlas")
    if len(textures) > 256:
        raise AtlasError(f"Quá nhiều texture: {len(textures)} > 256")

    decoded: dict[str, tuple[int, int, bytes]] = {}
    for key, data in textures.items():
        w, h, rgba = decode_png_rgba(data)
        decoded[key] = (w, h, rgba)

    packed = pack({k: (w, h) for k, (w, h, _r) in decoded.items()})
    W, H = packed["width"], packed["height"]

    px = bytearray(W * H * 4)      # transparent
    for key, (x, y, w, h) in packed["placements"].items():
        _tw, _th, rgba = decoded[key]
        for row in range(h):
            src = (row * w) * 4
            dst = ((y + row) * W + x) * 4
            px[dst:dst + w * 4] = rgba[src:src + w * 4]

    uv = {key: [float(x), float(y), float(x + w), float(y + h)]
          for key, (x, y, w, h) in packed["placements"].items()}
    png = _encode(W, H, bytes(px))
    return {"png": png, "uv": uv, "width": W, "height": H,
            "placements": packed["placements"]}


def _encode(width: int, height: int, rgba: bytes) -> bytes:
    """PNG encode nội bộ (giống templates.encode_png — tránh import chéo)."""
    raw = b"".join(
        b"\x00" + rgba[y * width * 4:(y + 1) * width * 4]
        for y in range(height)
    )

    def chunk(tag: bytes, body: bytes) -> bytes:
        return (struct.pack(">I", len(body)) + tag + body
                + struct.pack(">I", zlib.crc32(tag + body) & 0xFFFFFFFF))

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (PNG_SIGNATURE + chunk(b"IHDR", ihdr)
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def sha256(data: bytes) -> str:
    """Hash helper — deterministic check (mục 8.4: build 2 lần cùng hash)."""
    import hashlib
    return hashlib.sha256(data).hexdigest()
