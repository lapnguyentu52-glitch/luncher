"""Static renderer (master plan v3 mục 9) — VoxelSpec -> PNG.

Dùng cho: thumbnail, pack preview, WebGL fallback, test render, CI.
Không cần GPU, không cần Pillow — stdlib only (mục 9 acceptance).

Camera isometric cố định (deterministic — không orbit):
- View từ hướng (+x, +y, +z); 3 mặt thấy được: up (+y), east (+x), south (+z).
- Chiếu dimetric: sx = (x - z), sy = (x + z) * 0.5 - y.
- Shading cố định: up 1.0, east 0.8, south 0.62 (không lighting engine).

Raster: z-buffer per pixel. Depth = x + y + z; viewer nằm ở +∞ theo hướng
(1, 1, 1) nên điểm có tổng LỚN HƠN = gần camera hơn -> z-buffer giữ MAX.

Texture sampling (mục 9: không photorealistic):
- "#hex" -> màu solid + shading.
- asset path / key thiếu -> màu fallback (validator đã bắt riêng ở model3d).

Render 2 lần cùng spec + size -> cùng bytes (regression test mục 9).
"""
from __future__ import annotations

import math
from typing import Any

from services.visuals import model3d

#: Shading per visible face (cố định, deterministic)
SHADE = {"up": 1.0, "east": 0.8, "south": 0.62}

#: Màu fallback khi resolve texture thất bại (validator model3d sẽ bắt riêng)
FALLBACK = (232, 178, 58)      # vàng totem #e8b23a

#: Camera isometric — chỉ thấy 3 mặt này
VISIBLE_FACES = ("south", "east", "up")


def _hex_rgb(color: str) -> tuple[int, int, int]:
    c = color.lstrip("#")
    if len(c) == 3:
        c = "".join(ch * 2 for ch in c)
    try:
        return int(c[0:2], 16), int(c[2:4], 16), int(c[4:6], 16)
    except ValueError:
        return FALLBACK


def _resolve_color(value: Any, textures: dict) -> tuple[int, int, int] | None:
    """Face texture ref -> RGB. None nếu không resolve được (dùng FALLBACK)."""
    if not isinstance(value, str) or not value.strip():
        return None
    v = value.strip()
    if v.startswith("#"):
        return _hex_rgb(v)
    target = textures.get(v)
    if isinstance(target, str) and target.strip().startswith("#"):
        return _hex_rgb(target)
    if isinstance(target, str) and target.strip().startswith("assets/"):
        return None        # asset texture — không load file ở static render
    return None


def _face_geometry(cube: dict) -> list[tuple[str, list[list[float]]]]:
    """Cube -> [(face, 3 gốc của mặt), ...] cho 3 mặt thấy được.

    Mỗi mặt: [origin3, e1_3, e2_3] — origin là góc có x+y+z nhỏ nhất của mặt,
    e1/e2 là 2 cạnh (quaternion chiếu ra parallelogram màn hình).
    """
    try:
        frm = [float(v) for v in cube["from"]]
        to = [float(v) for v in cube["to"]]
    except (KeyError, TypeError, ValueError):
        return []
    lo = [min(frm[i], to[i]) for i in range(3)]
    hi = [max(frm[i], to[i]) for i in range(3)]
    dx, dy, dz = hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]
    if dx <= 0 or dy <= 0 or dz <= 0:
        return []

    x0, y0, z0 = lo
    x1, y1, z1 = hi
    # Các mặt tham chiếu góc (x0,y0,z0) làm origin chung
    return [
        ("up",    [[x0, y1, z0], [dx, 0, 0], [0, 0, dz]]),
        ("east",  [[x1, y0, z0], [0, 0, dz], [0, dy, 0]]),
        ("south", [[x0, y0, z1], [dx, 0, 0], [0, dy, 0]]),
    ]


def _project(p: list[float]) -> tuple[float, float]:
    """3D -> 2D isometric: sx = x - z, sy = (x + z) * 0.5 - y."""
    x, y, z = p
    return (x - z, (x + z) * 0.5 - y)


def render_rgba(spec: dict, size: int = 256, pad: int = 12) -> tuple[int, bytes]:
    """VoxelSpec -> (size, rgba bytes) RGBA có nền trong suốt."""
    size = max(16, min(1024, int(size)))
    norm = model3d.normalize(spec)
    textures = norm.get("textures") or {}
    px = bytearray(size * size * 4)          # transparent
    zbuf = [-math.inf] * (size * size)       # depth lớn hơn = gần camera

    faces: list[dict] = []
    for cube in norm["cubes"]:
        for fname, (origin, e1, e2) in _face_geometry(cube):
            if fname not in VISIBLE_FACES:
                continue
            p0 = _project(origin)
            pa = _project([origin[i] + e1[i] for i in range(3)])
            pb = _project([origin[i] + e2[i] for i in range(3)])
            d0 = sum(origin)
            da = sum(e1)
            db = sum(e2)
            color = _resolve_color(
                (cube.get("faces") or {}).get(fname, {}).get("texture")
                if isinstance((cube.get("faces") or {}).get(fname), dict) else None,
                textures)
            if color is None:
                color = FALLBACK
            shade = SHADE[fname]
            faces.append({"p0": p0, "a": (pa[0] - p0[0], pa[1] - p0[1]),
                          "b": (pb[0] - p0[0], pb[1] - p0[1]),
                          "d0": d0, "da": da, "db": db,
                          "color": tuple(int(c * shade) for c in color)})

    if not faces:
        return size, bytes(px)

    # Bounds toàn cảnh -> scale fit + center (deterministic)
    xs: list[float] = []
    ys: list[float] = []
    for f in faces:
        for ex in (0.0, 1.0):
            for ey in (0.0, 1.0):
                xs.append(f["p0"][0] + ex * f["a"][0] + ey * f["b"][0])
                ys.append(f["p0"][1] + ex * f["a"][1] + ey * f["b"][1])
    min_x, max_x = min(xs), max(xs)
    min_y, max_y = min(ys), max(ys)
    span = max(max_x - min_x, max_y - min_y) or 1.0
    scale = (size - 2 * pad) / span
    off_x = (size - (max_x - min_x) * scale) / 2 - min_x * scale
    off_y = (size - (max_y - min_y) * scale) / 2 - min_y * scale

    # Raster từng face — z-buffer per pixel
    for f in faces:
        ax, ay = f["a"]
        bx, by = f["b"]
        p0x, p0y = f["p0"]
        corners = [(p0x + ex * ax + ey * bx, p0y + ex * ay + ey * by)
                   for ex, ey in ((0, 0), (1, 0), (1, 1), (0, 1))]
        cx = [c[0] * scale + off_x for c in corners]
        cy = [c[1] * scale + off_y for c in corners]
        x_lo = max(0, int(math.floor(min(cx))))
        x_hi = min(size - 1, int(math.ceil(max(cx))))
        y_lo = max(0, int(math.floor(min(cy))))
        y_hi = min(size - 1, int(math.ceil(max(cy))))
        if x_lo > x_hi or y_lo > y_hi:
            continue
        det = ax * by - ay * bx
        if abs(det) < 1e-12:
            continue
        r, g, b = f["color"]
        d0, da, db = f["d0"], f["da"], f["db"]
        for py in range(y_lo, y_hi + 1):
            fy = py + 0.5
            for pxx in range(x_lo, x_hi + 1):
                fx = pxx + 0.5
                # solve [a b] [u]   (fx - p0x')
                #        [c d] [v] = (fy - p0y')
                rx = fx - (p0x * scale + off_x)
                ry = fy - (p0y * scale + off_y)
                u = (rx * by - ry * bx) / det
                v = (ax * ry - ay * rx) / det
                if not (0.0 <= u <= 1.0 and 0.0 <= v <= 1.0):
                    continue
                depth = d0 + u * da + v * db
                idx = py * size + pxx
                if depth > zbuf[idx]:    # gần camera hơn (sum lớn hơn) -> đè
                    zbuf[idx] = depth
                    i = idx * 4
                    px[i:i + 4] = bytes((r, g, b, 255))
    return size, bytes(px)


def render_png(spec: dict, size: int = 256) -> bytes:
    """VoxelSpec -> PNG RGBA bytes (mục 9: PNG output, deterministic)."""
    from services.resources.templates import encode_png
    s, rgba = render_rgba(spec, size=size)
    return encode_png(s, s, rgba)
