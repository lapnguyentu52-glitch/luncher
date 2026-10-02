"""Pack templates — base template + PNG generator (spec 3.0 mục 10.2).

PNG viết bằng zlib+struct (stdlib) — không phụ thuộc Pillow. Mỗi texture là
PNG RGBA placeholder với pattern đơn giản, đủ nhìn ra pack đang áp dụng.
"""
from __future__ import annotations

import struct
import zlib


def encode_png(width: int, height: int, rgba: bytes) -> bytes:
    """rgba = width*height*4 bytes -> PNG 8-bit RGBA bytes."""
    raw = b"".join(
        b"\x00" + rgba[y * width * 4:(y + 1) * width * 4]  # filter 0 mỗi dòng
        for y in range(height)
    )

    def chunk(tag: bytes, data: bytes) -> bytes:
        return (struct.pack(">I", len(data)) + tag + data
                + struct.pack(">I", zlib.crc32(tag + data) & 0xFFFFFFFF))

    ihdr = struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0)
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr)
            + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def write_png(path, width: int, height: int, rgba: bytes) -> None:
    """Ghi PNG từ raw RGBA."""
    with open(path, "wb") as f:
        f.write(encode_png(width, height, rgba))


def _clamp(v: int) -> int:
    return max(0, min(255, v))


def solid(color: tuple[int, int, int], alpha: int = 255, size: int = 16) -> bytes:
    return bytes((*color, _clamp(alpha))) * (size * size)


def _set(px: bytearray, size: int, x: int, y: int, color: tuple[int, int, int], alpha: int = 230) -> None:
    if 0 <= x < size and 0 <= y < size:
        i = (y * size + x) * 4
        px[i:i + 4] = bytes((*color, _clamp(alpha)))


def crosshair_texture(color: tuple[int, int, int], gap: int = 3,
                      thickness: int = 1, size: int = 16) -> bytes:
    """Crosshair kiểu '+': 4 thanh từ tâm, chừa gap."""
    px = bytearray(solid((0, 0, 0, 0), size=size))
    mid = size // 2
    for i in range(size):
        if abs(i - mid) > gap - 1:  # phần ngoài gap
            for t in range(thickness):
                _set(px, size, i, mid + t, color)        # thanh ngang
                _set(px, size, mid + t, i, color)        # thanh dọc
    return bytes(px)


def sword_icon(color: tuple[int, int, int], size: int = 32) -> bytes:
    """Placeholder item: lưỡi chéo đơn giản."""
    px = bytearray(solid((0, 0, 0, 0), size=size))
    for i in range(size):
        x, y = size - 1 - i, i
        for d in range(3):
            _set(px, size, max(0, x - d), min(size - 1, y + d), color, 235)
    return bytes(px)


# ------------------------------------------------------------------
# Base templates (mục 10.2: Select base template + visual modules)
# ------------------------------------------------------------------

BASE_TEMPLATES: dict[str, dict] = {
    "minimal": {
        "labelKey": "minimal",
        "descKey": "minimalDesc",
        "modules": {"crosshair": True, "items": False, "hud": False},
        "accent": (231, 76, 60),
    },
    "pvp": {
        "labelKey": "pvp",
        "descKey": "pvpDesc",
        "modules": {"crosshair": True, "items": True, "hud": True},
        "accent": (52, 152, 219),
    },
    "blank": {
        "labelKey": "blank",
        "descKey": "blankDesc",
        "modules": {"crosshair": False, "items": False, "hud": False},
        "accent": (149, 165, 166),
    },
}

#: Texture files mỗi module sinh (relative path trong pack).
MODULE_FILES = {
    "crosshair": ["assets/minecraft/textures/gui/crosshair.png"],
    "items": ["assets/minecraft/textures/item/antares_placeholder_sword.png"],
    "hud": ["assets/minecraft/textures/gui/widgets_antares.png"],
}


def _png(size: int, rgba: bytes) -> bytes:
    """Raw RGBA data -> encoded PNG bytes."""
    return encode_png(size, size, rgba)


def generate_files(template_id: str, mc_version: str) -> dict[str, bytes]:
    """Sinh {relative_path: PNG bytes encoded} cho template + version chọn."""
    tpl = BASE_TEMPLATES.get(template_id) or BASE_TEMPLATES["minimal"]
    accent = tpl["accent"]
    out: dict[str, bytes] = {}
    mods = tpl["modules"]
    if mods.get("crosshair"):
        out["assets/minecraft/textures/gui/crosshair.png"] = _png(
            16, crosshair_texture(accent))
    if mods.get("items"):
        out["assets/minecraft/textures/item/antares_placeholder_sword.png"] = _png(
            32, sword_icon(accent))
    if mods.get("hud"):
        out["assets/minecraft/textures/gui/widgets_antares.png"] = _png(
            32, crosshair_texture(accent, gap=2, size=32))
    return out
