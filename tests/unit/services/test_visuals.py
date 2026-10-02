"""Visual Studio — crosshair normalize/render/export (mục 8)."""
from __future__ import annotations

import struct
import zlib

from services.visuals import crosshair
from services.visuals import VisualStudioService


def test_normalize_clamps_and_rejects_bad_color():
    n = crosshair.normalize({"thickness": "99", "gap": -5, "opacity": 5,
                             "shape": "weird", "color": "#ff4655"})
    assert n["thickness"] == 4 and n["gap"] == 0 and n["opacity"] == 1.0
    assert n["shape"] == "cross" and n["color"] == "#ff4655"
    assert crosshair.normalize({"color": "javascript:alert(1)"})["color"] == "#ff4655"


def _decode_rgba(png: bytes) -> list[list[bytes]]:
    idx, idat = 8, b""
    while idx < len(png):
        ln = struct.unpack(">I", png[idx:idx + 4])[0]
        tag = png[idx + 4:idx + 8]
        if tag == b"IDAT":
            idat += png[idx + 8:idx + 8 + ln]
        idx += 12 + ln
    raw = zlib.decompress(idat)
    stride = 16 * 4 + 1
    return [[raw[y * stride + 1 + x * 4: y * stride + 1 + x * 4 + 4]
             for x in range(16)] for y in range(16)]


def test_render_geometry_cross_gap_thickness(ctx):
    spec = {"shape": "cross", "thickness": 2, "gap": 1, "color": "#ff0000",
            "outline": False, "dot": False, "opacity": 1.0}
    img = _decode_rgba(crosshair.render_png(spec))
    red = lambda px: px[:3] == b"\xff\x00\x00"

    # Hàng giữa: 16 - lỗ giữa 2px = 14 (thickness 2 + gap 1 -> lỗ 2x2)
    assert sum(1 for x in range(16) if red(img[8][x])) == 14
    # Lỗ giữa trống
    assert not red(img[7][7]) and not red(img[8][8])
    # 4 cánh chạm mép
    assert red(img[8][0]) and red(img[8][15]) and red(img[0][8]) and red(img[15][8])
    # Hàng ngoài gap: chỉ 2 cột dọc
    assert sum(1 for x in range(16) if red(img[3][x])) == 2


def test_render_dot_and_outline():
    spec = {"shape": "cross", "thickness": 2, "gap": 1, "color": "#ff0000",
            "outline": True, "dot": True, "opacity": 1.0}
    img = _decode_rgba(crosshair.render_png(spec))
    # Dot tô tâm
    assert any(img[y][x][:3] == b"\xff\x00\x00" for y in (7, 8) for x in (7, 8))
    # Outline: đen sát cạnh cánh (trên hàng giữa, cạnh lỗ)
    assert img[8][7][:3] == b"\x00\x00\x00"


def test_all_presets_render_valid_png():
    for pid in crosshair.PRESET_ORDER:
        png = crosshair.render_png(crosshair.PRESETS[pid])
        assert png.startswith(b"\x89PNG\r\n\x1a\n")
        assert struct.unpack(">II", png[16:24]) == (16, 16)


def test_export_pack_end_to_end(ctx, instance_id):
    svc = VisualStudioService(ctx)
    ctx.set("resource_studio", ResourceStudioForTest(ctx))

    spec = {"shape": "cross", "thickness": 2, "gap": 1, "color": "#ff0000",
            "outline": False, "dot": False, "opacity": 1.0}
    result = svc.export_pack("My CH", spec, "1.21.4", install_instance_id=instance_id)
    pid = result["projectId"]

    # Pixel-perfect: texture trong project == renderer
    tex = (ctx.paths.resource_studio / pid /
           "generated" / "assets/minecraft/textures/gui/sprites/icon/crosshair.png")
    assert tex.read_bytes() == crosshair.render_png(spec)
    # Spec JSON lưu project (mục 34)
    assert ctx.get("resource_studio").get(pid)["visuals"]["crosshair"]["thickness"] == 2
    # ZIP cài vào instance
    rp_dir = ctx.paths.instances / instance_id / "game" / "resourcepacks"
    assert any(f.suffix == ".zip" for f in rp_dir.glob("*.zip"))


def test_export_path_version_aware(ctx):
    svc = VisualStudioService(ctx)
    ctx.set("resource_studio", ResourceStudioForTest(ctx))
    spec = {"shape": "dot"}

    new = svc.export_pack("New", spec, "1.21.4")
    old = svc.export_pack("Old", spec, "1.19.4")
    base = ctx.paths.resource_studio
    assert (base / new["projectId"] / "generated" /
            "assets/minecraft/textures/gui/sprites/icon/crosshair.png").is_file()
    assert (base / old["projectId"] / "generated" /
            "assets/minecraft/textures/gui/crosshair.png").is_file()


class ResourceStudioForTest:
    """Lazy wrapper — tránh circular import trong SUT (service lấy qua ctx.get)."""

    def __init__(self, ctx) -> None:
        from services.resources import ResourceStudioService
        self._impl = ResourceStudioService(ctx)
        self.projects = self._impl.projects

    def __getattr__(self, name):
        return getattr(self._impl, name)
