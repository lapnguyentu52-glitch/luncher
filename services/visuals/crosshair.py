"""Crosshair Studio backend — spec renderer + profile (mục 8.1, 8.3).

- Renderer vẽ crosshair theo spec (shape/size/thickness/gap/outline/dot/color)
  ra PNG 16x16 — dùng đúng file `assets/minecraft/textures/gui/icons.png`
  slot crosshair khi export resource pack.
- Profile JSON lưu trong project visuals (mục 34) — KHÔNG lưu binary.
- Presets theo mục 8.1: Minimal / Dot / Classic / Thin / PvP / Clean /
  Competitive.
- Validation theo mục 76: mọi giá trị number bị clamp, màu phải hex hợp lệ.
"""
from __future__ import annotations

from services.resources.templates import encode_png

#: Mặc định spec editor (mục 8.1).
DEFAULT_SPEC = {
    "shape": "cross",           # cross | circle | dot
    "size": 16,                 # canvas 16 -> crosshair chiếm trọn
    "thickness": 1,
    "gap": 3,
    "color": "#ff4655",
    "outline": True,
    "outlineColor": "#000000",
    "dot": False,
    "opacity": 0.9,
}

PRESETS: dict[str, dict] = {
    "minimal": {"shape": "cross", "thickness": 1, "gap": 4, "dot": False,
                "color": "#e8eef5", "outline": False},
    "dot": {"shape": "dot", "thickness": 2, "gap": 0, "dot": False,
            "color": "#ff4655", "outline": True},
    "classic": {"shape": "cross", "thickness": 1, "gap": 3, "dot": False,
                "color": "#e8eef5", "outline": True},
    "thin": {"shape": "cross", "thickness": 1, "gap": 2, "dot": True,
             "color": "#7ef29a", "outline": False},
    "pvp": {"shape": "cross", "thickness": 2, "gap": 2, "dot": True,
            "color": "#ff4655", "outline": True},
    "clean": {"shape": "circle", "thickness": 1, "gap": 3, "dot": True,
              "color": "#5b9dff", "outline": False},
    "competitive": {"shape": "cross", "thickness": 2, "gap": 1, "dot": False,
                    "color": "#ffd75b", "outline": True},
}

PRESET_ORDER = ("minimal", "dot", "classic", "thin", "pvp", "clean", "competitive")

#: Ràng buộc an toàn (mục 76).
_LIMITS = {
    "thickness": (1, 4),
    "gap": (0, 7),
    "opacity": (0.1, 1.0),
}


def normalize(spec: dict | None) -> dict:
    """Clone + clamp spec — không tin input (mục 76)."""
    out = {**DEFAULT_SPEC}
    if not isinstance(spec, dict):
        return out
    shape = spec.get("shape")
    if shape in ("cross", "circle", "dot"):
        out["shape"] = shape
    color = spec.get("color")
    if isinstance(color, str) and _is_hex(color):
        out["color"] = color
    oc = spec.get("outlineColor")
    if isinstance(oc, str) and _is_hex(oc):
        out["outlineColor"] = oc
    out["outline"] = bool(spec.get("outline", out["outline"]))
    out["dot"] = bool(spec.get("dot", out["dot"]))
    for key, (lo, hi) in _LIMITS.items():
        try:
            v = float(spec.get(key, out[key]))
        except (TypeError, ValueError):
            continue
        out[key] = int(v) if key != "opacity" else round(min(hi, max(lo, v)), 2)
        if key != "opacity":
            out[key] = min(hi, max(lo, int(v)))
    return out


def _is_hex(c: str) -> bool:
    c = c.strip()
    if not c.startswith("#") or len(c) not in (4, 7):
        return False
    try:
        int(c[1:], 16)
        return True
    except ValueError:
        return False


def hex_rgb(color: str) -> tuple[int, int, int]:
    c = color.lstrip("#")
    if len(c) == 3:
        c = "".join(ch * 2 for ch in c)
    return int(c[0:2], 16), int(c[2:4], 16), int(c[4:6], 16)


# ------------------------------------------------------------------
# Renderer — PNG bytes từ spec (khớp canvas preview phía UI)
# ------------------------------------------------------------------


def render_png(spec: dict, size: int = 16) -> bytes:
    """Render spec -> PNG RGBA bytes. Preview UI vẽ canvas cùng hình học."""
    s = normalize(spec)
    rgb = hex_rgb(s["color"])
    o_rgb = hex_rgb(s["outlineColor"])
    alpha = int(255 * s["opacity"])

    px = bytearray(size * size * 4)  # transparent

    def put(x: int, y: int, color: tuple[int, int, int], a: int) -> None:
        if 0 <= x < size and 0 <= y < size:
            i = (y * size + x) * 4
            px[i:i + 4] = bytes((*color, max(0, min(255, a))))

    mid = size // 2
    t = s["thickness"]
    gap = s["gap"]

    def arm_pixels():
        if s["shape"] == "dot":
            for dy in range(-t // 2, t // 2 + 1):
                for dx in range(-t // 2, t // 2 + 1):
                    yield mid + dx, mid + dy, rgb, alpha
            return
        # 4 thanh: từ mép gap tới rìa
        for i in range(gap, mid):
            for k in range(t):
                yield mid + k, mid - 1 - i, rgb, alpha      # lên
                yield mid + k, mid + i, rgb, alpha          # xuống
                yield mid - 1 - i, mid + k, rgb, alpha      # trái
                yield mid + i, mid + k, rgb, alpha          # phải
        if s["shape"] == "circle":
            # bo góc: các điểm ở ~45° của khung vuông ngoại tiếp
            import math
            r = mid - gap / 2
            for step in range(72):
                ang = step * math.pi / 36
                x = int(mid + r * math.cos(ang) - t / 2)
                y = int(mid + r * math.sin(ang) - t / 2)
                for k in range(t):
                    for j in range(t):
                        put(x + j, y + k, rgb, alpha)

    # Outline: vẽ viền đen quanh pixel chính (đơn giản: offset 4 hướng)
    if s["outline"]:
        main = list(arm_pixels())
        for x, y, _c, _a in main:
            for dx, dy in ((1, 0), (-1, 0), (0, 1), (0, -1)):
                put(x + dx, y + dy, o_rgb, alpha)
        for x, y, c, a in main:
            put(x, y, c, a)
    else:
        for x, y, c, a in arm_pixels():
            put(x, y, c, a)

    # Dot giữa (nếu bật)
    if s["dot"]:
        r = max(0, t - 1) // 2
        for dy in range(-r, r + 1):
            for dx in range(-r, r + 1):
                put(mid + dx, mid + dy, rgb, alpha)

    return encode_png(size, size, bytes(px))
