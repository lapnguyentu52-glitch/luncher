"""A/B parity goldens — visual renderers (Batch 15.1f follow-up).

Sinh golden RGBA fixtures từ services/visuals/ (Python legacy, nguồn sự thật).
Rust unit tests sẽ đọc các file này (include_str!) và so pixel-qua-pixel với
render_rgba/render_png của antares-visuals — evidence #1 (A/B snapshot) trong
PARITY.md, tầng RGBA pixel (PNG container byte-level khác nhau do zlib.compress
vs stored-blocks — chấp nhận được, mục tiêu là render geometry parity).

Chạy: python -m tests.parity.gen_visuals_golden
Idempotent — output ổn định giữa các lần chạy cùng version legacy.
"""

from __future__ import annotations

import json
import math
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT))

from services.visuals import crosshair, fx, totem  # noqa: E402

OUT = Path(__file__).resolve().parent / "golden" / "visuals"


def rgba_of(png_bytes: bytes) -> dict:
    """PNG 8-bit RGBA (filter 0, của encode_png nội bộ) -> {w, h, rgba hex}."""
    import struct
    import zlib

    if len(png_bytes) < 33 or not png_bytes.startswith(b"\x89PNG\r\n\x1a\n"):
        raise ValueError("not a PNG")
    idx = 8
    ihdr = None
    idat = b""
    while idx + 12 <= len(png_bytes):
        ln = struct.unpack(">I", png_bytes[idx:idx + 4])[0]
        tag = png_bytes[idx + 4:idx + 8]
        body = png_bytes[idx + 8:idx + 8 + ln]
        if tag == b"IHDR":
            ihdr = body
        elif tag == b"IDAT":
            idat += body
        elif tag == b"IEND":
            break
        idx += 12 + ln
    w, h, depth, ctype, _c, _f, _i = struct.unpack(">IIBBBBB", ihdr)
    if depth != 8 or ctype != 6:
        raise ValueError(f"not RGBA8: depth={depth} ctype={ctype}")
    raw = zlib.decompress(idat)
    stride = w * 4 + 1
    px = bytearray(w * h * 4)
    for y in range(h):
        if raw[y * stride] != 0:
            raise ValueError(f"filter {raw[y * stride]} != 0")
        px[y * w * 4:(y + 1) * w * 4] = raw[y * stride + 1:(y + 1) * stride]
    return {"w": w, "h": h, "rgba": px.hex()}


def crosshair_cases():
    return [
        # (tên file, spec, size) — phủ mọi shape + clamp biên
        ("default", {}, 16),
        ("minimal", {"shape": "cross", "thickness": 1, "gap": 4, "dot": False,
                     "color": "#e8eef5", "outline": False}, 16),
        ("pvp", {"shape": "cross", "thickness": 2, "gap": 2, "dot": True,
                 "color": "#ff4655", "outline": True}, 16),
        ("clean_circle", {"shape": "circle", "thickness": 1, "gap": 3, "dot": True,
                          "color": "#5b9dff", "outline": False}, 16),
        ("dot_shape", {"shape": "dot", "thickness": 3, "gap": 0}, 16),
        ("thin_odd_thickness", {"shape": "cross", "thickness": 3, "gap": 1,
                                "outline": True}, 16),
        ("clamped", {"thickness": 99, "gap": -5, "opacity": 5.0}, 16),
        ("size32", {"shape": "cross", "thickness": 2, "gap": 3, "outline": True}, 32),
    ]


def totem_cases():
    return [
        ("default", {}),
        ("glow_wing", {"glow": True, "wing": True}),
        ("no_wing", {"wing": False}),
        ("dark", {"base": "#3a3f4a", "accent": "#1c1f26", "eye": "#ff4655",
                  "glow": True, "wing": True}),
        ("clamped_colors", {"base": "junk", "accent": "#f00", "eye": "#zzzzzz"}),
    ]


def fx_hit_cases():
    return [
        ("flash", {"kind": "flash", "color": "#ff3b30", "alpha": 120, "size": 70}),
        ("vignette", {"kind": "vignette", "color": "#5b9dff", "alpha": 200, "size": 90}),
        ("arrow", {"kind": "arrow", "color": "#ffffff", "alpha": 60, "size": 80}),
        ("cross", {"kind": "cross", "color": "#ffd75b", "alpha": 150, "size": 60}),
        ("none", {"kind": "none"}),
        ("clamped", {"kind": "flash", "alpha": 999, "size": 5}),
    ]


def fx_particle_cases():
    return [
        ("orb4", {"shape": "orb", "color": "#7fd4ff", "glow": True, "frames": 4}),
        ("spark3", {"shape": "spark", "glow": False, "frames": 3}),
        ("star2", {"shape": "star", "frames": 2}),
        ("ring1", {"shape": "ring", "frames": 1}),
        ("smoke8", {"shape": "smoke", "frames": 8}),
        ("clamped", {"shape": "cube", "frames": 99}),
    ]


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    manifest = {"generator": "tests/parity/gen_visuals_golden.py",
                "note": "RGBA pixel parity — PNG container không so byte",
                "cases": {}}

    for name, spec, size in crosshair_cases():
        png = crosshair.render_png(spec, size=size)
        manifest["cases"][f"crosshair_{name}"] = {
            **rgba_of(png), "spec": spec, "size": size, "kind": "crosshair"}

    for name, spec in totem_cases():
        png = totem.render_png(spec)
        manifest["cases"][f"totem_{name}"] = {**rgba_of(png), "spec": spec, "kind": "totem"}

    for name, spec in fx_hit_cases():
        png = fx.render_hit_png(spec)
        manifest["cases"][f"hit_{name}"] = {**rgba_of(png), "spec": spec, "kind": "hit"}

    for name, spec in fx_particle_cases():
        png = fx.render_particle_png(spec)
        manifest["cases"][f"particle_{name}"] = {**rgba_of(png), "spec": spec, "kind": "particle"}

    for name, payload in manifest["cases"].items():
        (OUT / f"{name}.json").write_text(json.dumps(payload, indent=1), encoding="utf-8")
    (OUT / "manifest.json").write_text(json.dumps(manifest, indent=1), encoding="utf-8")
    print(f"goldens: {len(manifest['cases'])} cases -> {OUT}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
