"""fx.py — Hit Effects + Particles renderer (spec 3.0 mục 52 subsections).

Mỗi kind export ra **1 texture PNG (+ .mcmeta cho particle animation)**:

- **hit**: overlay màn hình khi trúng đòn (mục 8.1 "Damage flash" /
  mục 52 "Hit Effects") → `textures/gui/sprites/hit.png` (128×128, alpha
  theo loại effect). Texture override thuần resource pack — không cần mod.
- **particles**: texture atlas animation (chuẩn MC: frame 16×16 xếp dọc
  1 cột) + `.mcmeta` khai báo frames — `textures/particle/glow.png`.

Sanitize + clamp mọi field (path safety, mục 76): màu hex, số nguyên bounded.
"""
from __future__ import annotations

from services.resources.templates import encode_png
from services.visuals.totem import hex_rgb

# ------------------------------------------------------------------
# Hit Effects (mục 8.1 damage flash / mục 52 Hit Effects)
# ------------------------------------------------------------------

HIT_KINDS = ("none", "flash", "vignette", "arrow", "cross")
HIT_SIZE = 128
HIT_MAX_ALPHA = 200

#: Mặc định + ranh giới (mục 76: clamp mọi input)
DEFAULT_HIT = {
    "kind": "flash",
    "color": "#ff3b30",
    "alpha": 120,           # 0..HIT_MAX_ALPHA
    "size": 70,             # % kích thước ảnh (vignette/cross/arrow)
}


def _is_hex(c: str) -> bool:
    if not isinstance(c, str) or not c.startswith("#") or len(c) != 7:
        return False
    return all(ch in "0123456789abcdefABCDEF" for ch in c[1:])


def normalize_hit(spec: dict | None) -> dict:
    out = {**DEFAULT_HIT}
    if not isinstance(spec, dict):
        return out
    if spec.get("kind") in HIT_KINDS:
        out["kind"] = spec["kind"]
    c = spec.get("color", out["color"])
    if isinstance(c, str) and _is_hex(c):
        out["color"] = c
    a = spec.get("alpha", out["alpha"])
    if isinstance(a, (int, float)) and not isinstance(a, bool):
        out["alpha"] = max(0, min(HIT_MAX_ALPHA, int(a)))
    z = spec.get("size", out["size"])
    if isinstance(z, (int, float)) and not isinstance(z, bool):
        out["size"] = max(20, min(100, int(z)))
    return out


def render_hit_png(spec: dict) -> bytes:
    """Hit overlay 128×128 RGBA. kind=none -> texture trong suốt hoàn toàn
    (export 'none' chỉ có ý nghĩa reset về mặc định game)."""
    s = normalize_hit(spec)
    r, g, b = hex_rgb(s["color"])
    a_hit = max(0, min(255, s["alpha"]))
    size = HIT_SIZE
    px = bytearray(size * size * 4)

    def put(x: int, y: int, rr: int, gg: int, bb: int, aa: int) -> None:
        if 0 <= x < size and 0 <= y < size:
            i = (y * size + x) * 4
            px[i:i + 4] = bytes((rr, gg, bb, max(0, min(255, aa))))

    kind = s["kind"]
    if kind != "none":
        frac = s["size"] / 100.0
        half = size / 2.0
        inner = half * frac          # bán kính vùng effect chính

        for y in range(size):
            for x in range(size):
                dx = x + 0.5 - half
                dy = y + 0.5 - half
                dist = (dx * dx + dy * dy) ** 0.5
                if kind == "flash":
                    # radial fade: đặc ở tâm -> suốt ở rìa
                    if dist < inner:
                        t = dist / inner
                        put(x, y, r, g, b, int(a_hit * (1.0 - t * 0.85)))
                elif kind == "vignette":
                    # ngược flash: rìa đậm, tâm giữ
                    if dist > inner:
                        t = min(1.0, (dist - inner) / max(1.0, half - inner))
                        put(x, y, r, g, b, int(a_hit * t))
                elif kind == "arrow":
                    # directional hit marker: tam giác từ mép vào tâm
                    if abs(dx) < half * 0.06 and 0 < dy < inner:
                        t = 1.0 - dy / inner
                        put(x, y, r, g, b, int(a_hit * (0.4 + 0.6 * t)))
                elif kind == "cross":
                    # 4 vạch chéo hướng tâm (hit marker X)
                    adx, ady = abs(dx), abs(dy)
                    if inner * 0.35 < dist < inner and abs(adx - ady) < half * 0.05:
                        put(x, y, r, g, b, a_hit)
    return encode_png(size, size, bytes(px))


# ------------------------------------------------------------------
# Particles — atlas animation (chuẩn MC: frame 16×16 xếp dọc 1 cột)
# ------------------------------------------------------------------

PARTICLE_SHAPES = ("orb", "spark", "star", "ring", "smoke")
P_FRAME = 16
P_MAX_FRAMES = 8

DEFAULT_PARTICLE = {
    "shape": "orb",
    "color": "#7fd4ff",
    "glow": True,
    "frames": 4,            # 1..8
}


def normalize_particle(spec: dict | None) -> dict:
    out = {**DEFAULT_PARTICLE}
    if not isinstance(spec, dict):
        return out
    if spec.get("shape") in PARTICLE_SHAPES:
        out["shape"] = spec["shape"]
    c = spec.get("color", out["color"])
    if isinstance(c, str) and _is_hex(c):
        out["color"] = c
    out["glow"] = bool(spec.get("glow", out["glow"]))
    f = spec.get("frames", out["frames"])
    if isinstance(f, (int, float)) and not isinstance(f, bool):
        out["frames"] = max(1, min(P_MAX_FRAMES, int(f)))
    return out


def render_particle_png(spec: dict) -> bytes:
    """Atlas particle: N frame 16×16 xếp DỌC (chuẩn MC) -> PNG 16×(16×N).
    Frame tiên triển khai đơn giản: shape co lại/dịu dần theo t (0..1)."""
    s = normalize_particle(spec)
    r, g, b = hex_rgb(s["color"])
    n = s["frames"]
    W = P_FRAME
    H = P_FRAME * n
    px = bytearray(W * H * 4)

    def put(x: int, y: int, rr: int, gg: int, bb: int, aa: int) -> None:
        if 0 <= x < W and 0 <= y < H:
            i = (y * W + x) * 4
            px[i:i + 4] = bytes((rr, gg, bb, max(0, min(255, aa))))

    half = W / 2.0
    for f in range(n):
        top = f * P_FRAME
        t = f / max(1, n - 1)          # 0..1 qua các frame
        shape = s["shape"]
        for y in range(P_FRAME):
            for x in range(W):
                dx = x + 0.5 - half
                dy = y + 0.5 - half
                dist = (dx * dx + dy * dy) ** 0.5
                # bán kính co dần theo frame; smoke thì loe ra
                grow = t if shape == "smoke" else (1.0 - t)
                rad = half * (0.35 + 0.6 * grow)
                alpha = 0
                if shape == "ring":
                    if rad * 0.72 < dist < rad:
                        alpha = int(230 * (1.0 - t * 0.5))
                elif shape == "spark":
                    # 4 tia + lõi
                    adx, ady = abs(dx), abs(dy)
                    if dist < rad * 0.3:
                        alpha = 235
                    elif (adx < 1.2 or ady < 1.2) and dist < rad:
                        alpha = int(200 * (1.0 - t * 0.4))
                elif shape == "star":
                    # 4 cánh: gần trục chéo
                    adx, ady = abs(dx), abs(dy)
                    if dist < rad and (abs(adx - ady) < 1.3 or dist < rad * 0.35):
                        alpha = int(225 * (1.0 - t * 0.35))
                elif shape == "smoke":
                    if dist < rad:
                        alpha = int(120 * (1.0 - t) + 25)
                else:  # orb
                    if dist < rad:
                        alpha = int(235 * (1.0 - (dist / rad) ** 2))
                if alpha > 0:
                    if s["glow"] and dist < rad * 0.55:
                        alpha = min(255, alpha + 25)   # lõi sáng hơn
                    put(x, top + y, r, g, b, alpha)
    return encode_png(W, H, bytes(px))


# ------------------------------------------------------------------
# .mcmeta cho particle animation (chuỗi JSON đúng định dạng MC)
# ------------------------------------------------------------------

def particle_mcmeta(frames: int, frametime: int = 2) -> str:
    """pack.mcmeta sibling `glow.png.mcmeta` — animation dọc chuẩn MC."""
    frames = max(1, min(P_MAX_FRAMES, int(frames)))
    frametime = max(1, min(10, int(frametime)))
    import json
    return json.dumps({
        "animation": {
            "frames": list(range(frames)),
            "frametime": frametime,
        }
    }, indent=2) + "\n"
