"""FX Studio — hit effects + particles renderer/export (spec 3.0 mục 52)."""
from __future__ import annotations

import struct
import zlib

from services.visuals import fx
from services.visuals.service import VisualStudioService


def _png_size(png: bytes) -> tuple[int, int]:
    return struct.unpack(">II", png[16:24])


# ------------------------------------------------------------------
# Sanitize — clamp mọi input (mục 76)
# ------------------------------------------------------------------

def test_normalize_hit_clamps():
    s = fx.normalize_hit({"kind": "nope-kind", "color": "red;drop",
                          "alpha": 999, "size": -5})
    assert s["kind"] == "flash"          # kind lạ -> default
    assert s["color"] == fx.DEFAULT_HIT["color"]
    assert s["alpha"] == fx.HIT_MAX_ALPHA
    assert s["size"] == 20               # min clamp
    assert fx.normalize_hit(None) == fx.DEFAULT_HIT


def test_normalize_particle_clamps():
    s = fx.normalize_particle({"shape": "cube", "frames": 99})
    assert s["shape"] == "orb"
    assert s["frames"] == fx.P_MAX_FRAMES
    s2 = fx.normalize_particle({"frames": 0})
    assert s2["frames"] == 1


# ------------------------------------------------------------------
# Render — PNG header + nội dung đúng kind
# ------------------------------------------------------------------

def test_render_hit_sizes_and_transparent_none():
    for kind in fx.HIT_KINDS:
        png = fx.render_hit_png({"kind": kind})
        assert _png_size(png) == (128, 128)
    # 'none' -> mọi pixel alpha=0: kiểm qua IDAT decode thô
    png = fx.render_hit_png({"kind": "none"})
    ihdr_end = png.index(b"IDAT")
    raw = zlib.decompress(png[ihdr_end + 4:png.index(b"IEND")])
    assert all(b == 0 for b in raw), "none phải trong suốt hoàn toàn"


def test_render_particle_atlas_vertical_layout():
    n = 5
    png = fx.render_particle_png({"shape": "spark", "frames": n})
    assert _png_size(png) == (16, 16 * n)   # atlas dọc chuẩn MC
    one = fx.render_particle_png({"shape": "spark", "frames": 1})
    assert _png_size(one) == (16, 16)


def test_mcmeta_shape():
    import json
    meta = json.loads(fx.particle_mcmeta(4, frametime=3))
    assert meta["animation"]["frames"] == [0, 1, 2, 3]
    assert meta["animation"]["frametime"] == 3
    assert json.loads(fx.particle_mcmeta(99))["animation"]["frames"] == list(range(fx.P_MAX_FRAMES))


# ------------------------------------------------------------------
# Service — defaults + previews
# ------------------------------------------------------------------

def test_fx_defaults_and_previews(ctx):
    vs = VisualStudioService(ctx)
    d = vs.fx_defaults()
    assert d["hit"]["kinds"] == list(fx.HIT_KINDS)
    assert d["particle"]["maxFrames"] == fx.P_MAX_FRAMES
    assert vs.render_hit_b64({"kind": "vignette"}).startswith("data:image/png;base64,")
    assert vs.render_particle_b64({"shape": "star"}).startswith("data:image/png;base64,")


# ------------------------------------------------------------------
# Export pack end-to-end (dùng pipeline Resource Studio thật)
# ------------------------------------------------------------------

def test_export_fx_pack_end_to_end(ctx, instance_id):
    from services.resources import ResourceStudioService
    ctx.set("resource_studio", ResourceStudioService(ctx))

    vs = VisualStudioService(ctx)
    r = vs.export_fx_pack(
        "FX Test",
        {"kind": "flash", "color": "#ff3b30", "alpha": 130, "size": 75},
        {"shape": "orb", "color": "#7fd4ff", "glow": True, "frames": 4},
        "1.21.4", install_instance_id=instance_id)
    assert r["build"]["sha256"]
    assert r["visuals"]["hit"]["kind"] == "flash"
    assert r["visuals"]["particle"]["frames"] == 4
    assert r["install"]["file"]         # ZIP đã install vào instance

    # pack ZIP chứa cả hit.png + glow.png + glow.png.mcmeta
    import zipfile
    rs = ctx.get("resource_studio")
    zip_path = rs.projects.dir_of(r["projectId"]) / "builds" / r["build"]["file"]
    with zipfile.ZipFile(zip_path) as z:
        names = z.namelist()
    assert any(n.endswith("gui/sprites/hit.png") for n in names)
    assert any(n.endswith("particle/glow.png") for n in names)
    assert any(n.endswith("glow.png.mcmeta") for n in names)


def test_export_fx_requires_at_least_one_spec(ctx):
    from services.resources import ResourceStudioService
    ctx.set("resource_studio", ResourceStudioService(ctx))
    vs = VisualStudioService(ctx)
    try:
        vs.export_fx_pack("Empty", None, None, "1.21.4")
        assert False, "phải raise"
    except Exception:
        pass
