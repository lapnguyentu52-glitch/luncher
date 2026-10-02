"""Totem Studio backend — texture + presets (spec 3.0 mục 9).

MVP hướng resource pack: totem của undying hiển thị khi chủ nhân chết —
thay texture 32x32 `items/totem_of_undying.png` theo preset màu + pattern.
Model 3D thật (mục 9.2 orbit/zoom) cần renderer web — để giai đoạn sau;
khuôn mẫu export giống crosshair (project RS + build + install).
"""
from __future__ import annotations

from services.resources.templates import encode_png

DEFAULT_SPEC = {
    "base": "#e8b23a",      # vàng totem
    "accent": "#8c5a2b",    # nâu gỗ
    "eye": "#3ad0e8",       # màu mắt
    "glow": False,          # viền sáng quanh hình
    "wing": True,           # vẽ cánh
}

PRESETS = {
    "classic": {"base": "#e8b23a", "accent": "#8c5a2b", "eye": "#3ad0e8", "glow": False, "wing": True},
    "crystal": {"base": "#7ef0ff", "accent": "#2b6f8c", "eye": "#ffffff", "glow": True, "wing": True},
    "minimal": {"base": "#e8b23a", "accent": "#8c5a2b", "eye": "#e8b23a", "glow": False, "wing": False},
    "dark": {"base": "#3a3f4a", "accent": "#1c1f26", "eye": "#ff4655", "glow": True, "wing": True},
    "red": {"base": "#ff4655", "accent": "#7a1420", "eye": "#ffd75b", "glow": True, "wing": True},
    "glass": {"base": "#cfe8ff", "accent": "#7fa8c8", "eye": "#5b9dff", "glow": False, "wing": True},
    "lowpoly": {"base": "#f0c95a", "accent": "#a06a2c", "eye": "#2c2c2c", "glow": False, "wing": False},
    "competitive": {"base": "#ffd75b", "accent": "#8c5a2b", "eye": "#ff4655", "glow": False, "wing": True},
}

PRESET_ORDER = ("classic", "crystal", "minimal", "dark", "red", "glass", "lowpoly", "competitive")

_SIZE = 32

#: Hình totem: thân + đầu + mắt (grid 32x32, đối xứng trục giữa).
_BODY_COLS = range(12, 20)       # thân 8 cột giữa
_HEAD_ROWS = range(4, 12)
_TORSO_ROWS = range(12, 26)
_EYE_POS = ((13, 7), (18, 7))    # (x, y) mắt trái/phải


def _clamp(v: str, default: str) -> str:
    return v if isinstance(v, str) and _is_hex(v) else default


def _is_hex(c: str) -> bool:
    c = c.strip()
    return c.startswith("#") and len(c) in (4, 7) and _hex_ok(c[1:])


def _hex_ok(c: str) -> bool:
    try:
        int(c, 16)
        return True
    except ValueError:
        return False


def hex_rgb(color: str) -> tuple[int, int, int]:
    c = color.lstrip("#")
    if len(c) == 3:
        c = "".join(ch * 2 for ch in c)
    return int(c[0:2], 16), int(c[2:4], 16), int(c[4:6], 16)


def normalize(spec: dict | None) -> dict:
    out = {**DEFAULT_SPEC}
    if not isinstance(spec, dict):
        return out
    out["base"] = _clamp(spec.get("base", out["base"]), out["base"])
    out["accent"] = _clamp(spec.get("accent", out["accent"]), out["accent"])
    out["eye"] = _clamp(spec.get("eye", out["eye"]), out["eye"])
    out["glow"] = bool(spec.get("glow", out["glow"]))
    out["wing"] = bool(spec.get("wing", out["wing"]))
    return out


def render_png(spec: dict) -> bytes:
    """Spec -> PNG 32x32 texture totem (đối xứng, đủ nhận diện preset)."""
    s = normalize(spec)
    base = hex_rgb(s["base"])
    accent = hex_rgb(s["accent"])
    eye = hex_rgb(s["eye"])
    glow_a = 90

    px = bytearray(_SIZE * _SIZE * 4)

    def put(x: int, y: int, c: tuple[int, int, int], a: int = 255) -> None:
        if 0 <= x < _SIZE and 0 <= y < _SIZE:
            i = (y * _SIZE + x) * 4
            px[i:i + 4] = bytes((*c, a))

    if s["glow"]:
        # quầng: mọi pixel sát hình (đơn giản: viền ngoài thân/đầu)
        for (x0, y0, w0, h0) in ((12, 4, 8, 22), (8, 10, 16, 12)):
            for i in range(w0):
                put(x0 + i, y0 - 1, base, glow_a)
                put(x0 + i, y0 + h0, base, glow_a)
            for j in range(h0):
                put(x0 - 1, y0 + j, base, glow_a)
                put(x0 + w0, y0 + j, base, glow_a)

    # Đầu
    for y in _HEAD_ROWS:
        for x in _BODY_COLS:
            put(x, y, base)
    # Mắt
    for (ex, ey) in _EYE_POS:
        put(ex, ey, eye)
        put(ex + 1, ey, eye)

    # Thân + hoạ tiết accent (zigzag)
    for y in _TORSO_ROWS:
        for x in _BODY_COLS:
            put(x, y, accent if (y // 2) % 2 == 0 and (x - 12) % 3 else base)
    # Chân
    for y in range(26, 30):
        for x in (13, 14, 17, 18):
            put(x, y, accent)

    # Cánh đối xứng
    if s["wing"]:
        for dy in range(10):
            for dx in range(5 - dy // 2):
                put(8 - dx, 10 + dy, accent, 220)
                put(23 + dx, 10 + dy, accent, 220)

    return encode_png(_SIZE, _SIZE, bytes(px))


# ----------------------------------------------------------------------
# VoxelSpec (mục 70 — Quick preset -> default voxel model cho Advanced)
# ----------------------------------------------------------------------

#: Voxel totem 16x16x16: thân + đầu + 2 mắt, khớp hình texture 32x32
#: (đầu 4..12, thân 4..12 -> trục Z 4..12). Không cánh — cánh là hoạ tiết phẳng.
def voxel_spec(spec: dict | None = None) -> dict:
    """Quick preset -> VoxelSpec (mục 70 migration: Quick -> Advanced).

    return {"grid": 16, "cubes": [body, head, eyeL, eyeR],
            "textures": {"base", "accent", "eye"}} — màu lấy từ normalize().
    """
    s = normalize(spec)
    return {
        "grid": 16,
        "cubes": [
            {"id": "body", "from": [4, 0, 4], "to": [12, 10, 12],
             "faces": {f: {"texture": "base"} for f in
                       ("north", "south", "east", "west", "up", "down")}},
            {"id": "head", "from": [4, 10, 4], "to": [12, 16, 12],
             "faces": {f: {"texture": "accent"} for f in
                       ("north", "south", "east", "west", "up", "down")}},
            {"id": "eye-l", "from": [5, 14, 3], "to": [7, 15, 4],
             "faces": {"north": {"texture": "eye"}}},
            {"id": "eye-r", "from": [9, 14, 3], "to": [11, 15, 4],
             "faces": {"north": {"texture": "eye"}}},
        ],
        "textures": {"base": s["base"], "accent": s["accent"], "eye": s["eye"]},
    }
