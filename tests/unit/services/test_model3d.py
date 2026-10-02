"""Model3D backend (Batch 3 — master plan v3 mục 8, 9, 15).

Validator matrix + atlas deterministic (mục 8.4: build 2 lần cùng hash) +
static renderer regression (mục 9: deterministic, không GPU).
"""
from __future__ import annotations

import struct
import zlib

from services.visuals import atlas, model3d, renderer
from services.visuals.atlas import AtlasError


# ----------------------------------------------------------------------
# Validator — geometry rules (mục 8.1)
# ----------------------------------------------------------------------

def _ok(spec) -> list[dict]:
    return model3d.validate(spec)


def test_valid_spec_clean():
    spec = {"grid": 16, "cubes": [
        {"id": "body", "from": [4, 0, 4], "to": [12, 10, 12],
         "faces": {"north": {"texture": "#e8b23a"}}},
    ], "textures": {"base": "#8c5a2b"}}
    assert _ok(spec) == []


def test_non_dict_and_empty_cubes():
    assert _ok("nope")[0]["code"] == "MODEL_SPEC_INVALID"
    findings = _ok({"grid": 16, "cubes": []})
    assert findings[0]["code"] == "MODEL_NO_CUBES"
    assert findings[0]["fixable"] is True


def test_zero_and_negative_size_blocked():
    findings = _ok({"cubes": [{"id": "a", "from": [4, 4, 4], "to": [4, 8, 8]}]})
    assert any(f["code"] == "MODEL_CUBE_EMPTY" and f["fixable"] for f in findings)
    findings = _ok({"cubes": [{"id": "a", "from": [8, 4, 4], "to": [4, 8, 8]}]})
    assert any(f["code"] == "MODEL_CUBE_EMPTY" and "âm" in f["message"]
               for f in findings)


def test_nan_infinity_rejected():
    for bad in (float("nan"), float("inf")):
        findings = _ok({"cubes": [{"id": "a", "from": [bad, 0, 0],
                                   "to": [4, 8, 8]}]})
        assert any(f["code"] == "MODEL_COORD_INVALID" and "hữu hạn" in f["message"]
                   for f in findings)


def test_coords_out_of_bounds():
    # Minecraft element bounds [-16, 32]
    findings = _ok({"cubes": [{"id": "a", "from": [-17, 0, 0], "to": [-16, 1, 1]}]})
    assert any(f["code"] == "MODEL_COORD_INVALID" and "giới hạn" in f["message"]
               for f in findings)
    assert _ok({"cubes": [{"id": "a", "from": [-16, 0, 0], "to": [-15, 1, 1]}]}) == []


def test_float_coords_allowed():
    # Decision Batch 0: floating-point cho phép
    assert _ok({"cubes": [{"id": "a", "from": [0.5, 0.25, 0.75],
                           "to": [4.5, 8.25, 8.75]}]}) == []


def test_duplicate_id_warning_only():
    findings = _ok({"cubes": [
        {"id": "a", "from": [0, 0, 0], "to": [1, 1, 1]},
        {"id": "a", "from": [2, 0, 0], "to": [3, 1, 1]},
    ]})
    dup = [f for f in findings if f["code"] == "MODEL_ID_DUPLICATE"]
    assert len(dup) == 1 and dup[0]["severity"] == "WARNING"
    assert not model3d.has_errors(findings)


def test_invalid_face_and_missing_texture():
    findings = _ok({"cubes": [{"id": "a", "from": [0, 0, 0], "to": [1, 1, 1],
                               "faces": {"left": {}}}]})
    assert any(f["code"] == "MODEL_FACE_INVALID" and f["fixable"] for f in findings)
    findings = _ok({"cubes": [{"id": "a", "from": [0, 0, 0], "to": [1, 1, 1],
                               "faces": {"north": {"texture": "unknown-key"}}}]})
    assert any(f["code"] == "MODEL_TEXTURE_MISSING" for f in findings)


def test_texture_resolution_paths_valid():
    spec = {"cubes": [{"id": "a", "from": [0, 0, 0], "to": [1, 1, 1],
                       "faces": {"north": {"texture": "base"},
                                 "up": {"texture": "skin"}}}],
            "textures": {"base": "#e8b23a",
                         "skin": "assets/minecraft/textures/item/x.png"}}
    assert _ok(spec) == []
    # textures trỏ tới giá trị rác
    spec["textures"]["base"] = 42
    findings = _ok(spec)
    assert any(f["code"] == "MODEL_TEXTURE_INVALID" for f in findings)


def test_uv_validation():
    base = {"id": "a", "from": [0, 0, 0], "to": [8, 8, 8]}
    # uv đảo -> ERROR fixable
    findings = _ok({"cubes": [{**base, "faces":
                               {"north": {"uv": [8, 8, 0, 0]}}}]})
    assert any(f["code"] == "MODEL_UV_INVALID" and f["fixable"] for f in findings)
    # uv ngoài 0..16 -> WARNING (MC tile)
    findings = _ok({"cubes": [{**base, "faces":
                               {"north": {"uv": [0, 0, 20, 8]}}}]})
    assert any(f["code"] == "MODEL_UV_OUT_OF_RANGE"
               and f["severity"] == "WARNING" for f in findings)
    assert not model3d.has_errors(findings)
    # uv không phải 4 số
    findings = _ok({"cubes": [{**base, "faces":
                               {"north": {"uv": [1, 2, 3]}}}]})
    assert any(f["code"] == "MODEL_UV_INVALID" for f in findings)


def test_cube_count_policy():
    cubes = [{"id": f"c{i}", "from": [i, 0, 0], "to": [i + 1, 1, 1]}
             for i in range(65)]
    findings = _ok({"cubes": cubes})
    assert any(f["code"] == "MODEL_CUBE_LIMIT" and f["severity"] == "WARNING"
               for f in findings)
    # > 256 hard cap -> FATAL
    cubes = [{"id": f"c{i}", "from": [0, 0, 0], "to": [1, 1, 1]}
             for i in range(300)]
    findings = _ok({"cubes": cubes})
    assert any(f["code"] == "MODEL_CUBE_LIMIT" and f["severity"] == "FATAL"
               for f in findings)


def test_finding_shape_and_helpers():
    findings = _ok({"cubes": []})
    f = findings[0]
    assert set(f.keys()) == {"severity", "code", "path", "message", "fixable"}
    assert model3d.has_errors(findings) and len(model3d.errors(findings)) == 1


# ----------------------------------------------------------------------
# Normalize
# ----------------------------------------------------------------------

def test_normalize_renames_duplicate_and_drops_junk():
    norm = model3d.normalize({"cubes": [
        {"id": "a", "from": [0, 0, 0], "to": [1, 1, 1]},
        {"id": "a", "from": [2, 0, 0], "to": [3, 1, 1]},
        "junk",
        {"from": [4, 0, 0], "to": [5, 1, 1]},
    ]})
    ids = [c["id"] for c in norm["cubes"]]
    assert ids[0] == "a" and ids[1] == "a-2"
    assert "junk" not in ids
    assert len(norm["cubes"]) == 3       # cube không id -> id="cube"


# ----------------------------------------------------------------------
# Atlas — decode fail-safe (mục 12.2)
# ----------------------------------------------------------------------

def _png(w: int, h: int, rgba: bytes) -> bytes:
    return atlas._encode(w, h, rgba)


def test_decode_rejects_garbage_and_bombs():
    for bad in (b"", b"not a png", PNG := b"\x89PNG\r\n\x1a\n" + b"\x00" * 20):
        try:
            atlas.decode_png_rgba(bad)
            assert False, f"phải raise cho {bad[:10]!r}"
        except AtlasError:
            pass
    # kích thước 0
    try:
        atlas.decode_png_rgba(_png(0, 0, b""))
        assert False
    except AtlasError:
        pass
    # quá lớn (bomb) — header 8193px vượt MAX_INPUT_DIM
    ihdr = struct.pack(">IIBBBBB", 8193, 8193, 8, 6, 0, 0, 0)
    fake = (b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR" + ihdr
            + struct.pack(">I", 0) + b"IEND" + b"")
    try:
        atlas.decode_png_rgba(fake)
        assert False
    except AtlasError as e:
        assert "quá lớn" in str(e)


def test_decode_roundtrip():
    px = bytes((255, 0, 0, 255)) * (2 * 2)
    w, h, rgba = atlas.decode_png_rgba(_png(2, 2, px))
    assert (w, h) == (2, 2) and rgba == px


# ----------------------------------------------------------------------
# Atlas — pack deterministic (mục 8.4)
# ----------------------------------------------------------------------

def test_pack_no_overlap_and_within_bounds():
    entries = {"a": (16, 16), "b": (32, 8), "c": (8, 32), "d": (16, 8)}
    result = atlas.pack(entries)
    W, H = result["width"], result["height"]
    assert 0 < W <= atlas.MAX_ATLAS_DIM and 0 < H <= atlas.MAX_ATLAS_DIM
    rects = list(result["placements"].values())
    for i, (x1, y1, w1, h1) in enumerate(rects):
        assert x1 >= 0 and y1 >= 0 and x1 + w1 <= W and y1 + h1 <= H
        for j, (x2, y2, w2, h2) in enumerate(rects):
            if i == j:
                continue
            overlap = not (x1 + w1 <= x2 or x2 + w2 <= x1
                           or y1 + h1 <= y2 or y2 + h2 <= y1)
            assert not overlap, f"{i} overlaps {j}"


def test_pack_deterministic_across_input_order():
    e1 = {"a": (16, 16), "b": (32, 8), "c": (8, 32)}
    e2 = {"c": (8, 32), "a": (16, 16), "b": (32, 8)}     # khác thứ tự input
    assert atlas.pack(e1) == atlas.pack(e2)


def test_build_atlas_deterministic_hash():
    textures = {"base": _png(4, 4, bytes((232, 178, 58, 255)) * 16),
                "accent": _png(2, 3, bytes((140, 90, 43, 255)) * 6)}
    out1 = atlas.build_atlas(textures)
    out2 = atlas.build_atlas(dict(reversed(list(textures.items()))))
    assert atlas.sha256(out1["png"]) == atlas.sha256(out2["png"])
    assert out1["uv"] == out2["uv"]
    # UV nằm trong atlas bounds, không overlap (đã check pack riêng)
    for x1, y1, x2, y2 in out1["uv"].values():
        assert 0 <= x1 < x2 <= out1["width"] and 0 <= y1 < y2 <= out1["height"]


def test_build_atlas_limits():
    try:
        atlas.build_atlas({})
        assert False
    except AtlasError:
        pass
    # texture quá rộng (2048x2, đủ data pixel — từ chối vì > MAX_ATLAS_DIM)
    try:
        atlas.build_atlas({"big": _png(2048, 2, b"\x00" * (2048 * 2 * 4))})
        assert False
    except AtlasError as e:
        assert "rộng" in str(e)


# ----------------------------------------------------------------------
# Static renderer (mục 9)
# ----------------------------------------------------------------------

TOTEM_SPEC = {
    "grid": 16,
    "cubes": [
        {"id": "body", "from": [4, 0, 4], "to": [12, 10, 12],
         "faces": {f: {"texture": "base"} for f in
                   ("north", "south", "east", "west", "up", "down")}},
        {"id": "head", "from": [4, 10, 4], "to": [12, 16, 12],
         "faces": {f: {"texture": "accent"} for f in
                   ("north", "south", "east", "west", "up", "down")}},
    ],
    "textures": {"base": "#e8b23a", "accent": "#8c5a2b"},
}


def _decode_png(png: bytes) -> tuple[int, int, list[list[bytes]]]:
    idx, idat = 8, b""
    while idx < len(png):
        ln = struct.unpack(">I", png[idx:idx + 4])[0]
        tag = png[idx + 4:idx + 8]
        if tag == b"IDAT":
            idat += png[idx + 8:idx + 8 + ln]
        idx += 12 + ln
    raw = zlib.decompress(idat)
    w, h = struct.unpack(">II", png[16:24])
    stride = w * 4 + 1
    rows = [raw[y * stride + 1:(y + 1) * stride] for y in range(h)]
    return w, h, [[row[x * 4:x * 4 + 4] for x in range(w)] for row in rows]


def test_renderer_deterministic_bytes():
    p1 = renderer.render_png(TOTEM_SPEC, size=128)
    p2 = renderer.render_png(TOTEM_SPEC, size=128)
    assert p1 == p2      # build 2 lần -> cùng bytes (mục 9 regression)


def test_renderer_output_shapes():
    size, rgba = renderer.render_rgba(TOTEM_SPEC, size=128)
    assert size == 128 and len(rgba) == size * size * 4
    png = renderer.render_png(TOTEM_SPEC, size=64)
    w, h, img = _decode_png(png)
    assert (w, h) == (64, 64)


def test_renderer_pixels_colored_and_background_transparent():
    w, h, img = _decode_png(renderer.render_png(TOTEM_SPEC, size=96))
    colored = sum(1 for row in img for px in row if px[3] == 255)
    transparent = sum(1 for row in img for px in row if px[3] == 0)
    assert colored > 200                            # hình totem có mặt
    assert transparent > colored                    # nền trong suốt chiếm đa số
    # Không có pixel alpha lạ (chỉ 0 hoặc 255)
    alphas = {px[3] for row in img for px in row}
    assert alphas <= {0, 255}


def test_renderer_face_shading_distinct():
    # 3 mặt thấy được phải có 3 mức sáng khác nhau (up > east > south)
    w, h, img = _decode_png(renderer.render_png(TOTEM_SPEC, size=96))
    seen = {bytes(px) for row in img for px in row if px[3] == 255}
    # base màu vàng: south 0.62, east 0.8, up 1.0 (nhân kênh, làm tròn)
    shades = {tuple(c[:3]) for c in seen}
    assert len({s for s in shades
                if s == (int(232 * 0.62), int(178 * 0.62), int(58 * 0.62))
                or s == (int(232 * 0.8), int(178 * 0.8), int(58 * 0.8))
                or s == (232, 178, 58)}) == 3


def test_renderer_empty_spec_transparent():
    size, rgba = renderer.render_rgba({"grid": 16, "cubes": []}, size=64)
    assert rgba == b"\x00" * (size * size * 4)


def test_renderer_64_cubes_no_crash_and_bounded():
    spec = {"grid": 16, "cubes": [
        {"id": f"c{i}", "from": [i % 8 * 2, i // 8 * 2, 0],
         "to": [i % 8 * 2 + 1, i // 8 * 2 + 1, 1]}
        for i in range(64)]}
    size, rgba = renderer.render_rgba(spec, size=128)
    assert len(rgba) == size * size * 4
