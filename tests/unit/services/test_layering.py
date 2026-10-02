"""Multi-pack layering (Batch 7 — master plan v3 mục 25).

Gates: deterministic output, conflict resolution, install order, persistence.
Minecraft áp pack cuối cùng trong options.txt cho path trùng — order lưu
THẤP priority trước, pack cao cuối cùng = thắng.
"""
from __future__ import annotations

import io
import struct
import zipfile
from pathlib import Path

import pytest

from core.errors.base import AntaresError
from services.resources import ResourceStudioService
from services.resources.layering import LayerStore

PNG = (b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR"
       + struct.pack(">IIBBBBB", 4, 4, 8, 6, 0, 0, 0) + b"\x00" * 4)


def _pack_bytes(files: dict[str, bytes]) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
        zf.writestr("pack.mcmeta", '{"pack": {"pack_format": 15, "description": "x"}}')
        zf.writestr("pack.png", PNG)
        for name, data in files.items():
            zf.writestr(name, data)
    return buf.getvalue()


@pytest.fixture()
def setup(ctx, instance_id):
    """2 project build xong + install vào instance, nội dung path trùng."""
    rs = ResourceStudioService(ctx)
    ctx.set("resource_studio", rs)
    gen1 = ctx.paths.resource_studio  # placeholder để build trực tiếp ZIP

    def make_zip(name: str, files: dict[str, bytes]) -> str:
        rp = Path(ctx.paths.instances) / instance_id / "game" / "resourcepacks"
        rp.mkdir(parents=True, exist_ok=True)
        zp = rp / f"{name}.zip"
        zp.write_bytes(_pack_bytes(files))
        return zp.name

    zip_a = make_zip("pack-a", {
        "assets/minecraft/textures/item/shared.png": PNG,
        "assets/minecraft/textures/item/only-a.png": PNG,
    })
    zip_b = make_zip("pack-b", {
        "assets/minecraft/textures/item/shared.png": PNG + b"b",
        "assets/minecraft/textures/item/only-b.png": PNG,
    })
    return rs, ctx, instance_id, [zip_a, zip_b]


def test_order_persistence_and_auto_append(setup):
    rs, ctx, inst, zips = setup
    layers = LayerStore(ctx)
    # chưa có order — pack mới auto-append theo alphabet, thấp trước
    order = layers.get_order(inst)
    assert order == ["pack-a.zip", "pack-b.zip"]
    # lưu order đảo lại
    saved = layers.set_order(inst, ["pack-b.zip", "pack-a.zip"])
    assert saved == ["pack-b.zip", "pack-a.zip"]
    assert layers.get_order(inst) == ["pack-b.zip", "pack-a.zip"]


def test_order_validation(setup):
    rs, ctx, inst, zips = setup
    layers = LayerStore(ctx)
    with pytest.raises(AntaresError):
        layers.set_order(inst, ["not-installed.zip"])            # chưa install
    with pytest.raises(AntaresError):
        layers.set_order(inst, ["pack-a.zip", "pack-a.zip"])     # duplicate
    with pytest.raises(AntaresError):
        layers.set_order(inst, ["../../evil.zip"])               # traversal
    # pack bị xoá -> tự lọc khỏi order
    rp = Path(ctx.paths.instances) / inst / "game" / "resourcepacks"
    (rp / "pack-b.zip").unlink()
    assert layers.get_order(inst) == ["pack-a.zip"]


def test_effective_resolution_deterministic(setup):
    rs, ctx, inst, zips = setup
    layers = LayerStore(ctx)
    layers.set_order(inst, ["pack-a.zip", "pack-b.zip"])   # b cao hơn a

    # shared.png: pack-b priority cao -> b thắng (deterministic)
    eff = layers.effective_asset(inst, "assets/minecraft/textures/item/shared.png")
    assert eff == {"pack": "pack-b.zip", "priority": 1}
    # chỉ pack a có
    eff = layers.effective_asset(inst, "assets/minecraft/textures/item/only-a.png")
    assert eff["pack"] == "pack-a.zip"
    # không pack nào có
    assert layers.effective_asset(inst, "assets/minecraft/textures/item/nope.png") is None

    # đảo order -> kết quả đổi theo (không cache mồ côi)
    layers.set_order(inst, ["pack-b.zip", "pack-a.zip"])
    eff = layers.effective_asset(inst, "assets/minecraft/textures/item/shared.png")
    assert eff["pack"] == "pack-a.zip"


def test_preview_effective_conflicts(setup):
    rs, ctx, inst, zips = setup
    layers = LayerStore(ctx)
    layers.set_order(inst, ["pack-a.zip", "pack-b.zip"])
    preview = layers.preview_effective(inst)
    assert preview["order"] == ["pack-a.zip", "pack-b.zip"]
    assert preview["assets"]["assets/minecraft/textures/item/shared.png"]["pack"] == "pack-b.zip"
    conflict_paths = {c["path"] for c in preview["conflicts"]}
    assert "assets/minecraft/textures/item/shared.png" in conflict_paths


def test_install_order_low_to_high(setup):
    rs, ctx, inst, zips = setup
    layers = LayerStore(ctx)
    layers.set_order(inst, ["pack-a.zip", "pack-b.zip"])   # b cao
    order = layers.install_order(inst)
    # thấp priority trước, pack cao CUỐI (Minecraft: pack cuối thắng)
    assert order == ["pack-a.zip", "pack-b.zip"]
    layers.set_order(inst, ["pack-b.zip", "pack-a.zip"])   # a cao
    assert layers.install_order(inst) == ["pack-b.zip", "pack-a.zip"]


def test_options_txt_sync_keeps_unknown_keys(setup):
    rs, ctx, inst, zips = setup
    layers = LayerStore(ctx)
    opts = Path(ctx.paths.instances) / inst / "game" / "options.txt"
    opts.parent.mkdir(parents=True, exist_ok=True)
    opts.write_text('version:3655\nresourcePacks:["old.zip"]\nmaxFps:120\n',
                    encoding="utf-8")
    layers.set_order(inst, ["pack-a.zip", "pack-b.zip"])
    text = opts.read_text(encoding="utf-8")
    assert 'resourcePacks:["pack-a.zip","pack-b.zip"]' in text
    assert "version:3655" in text and "maxFps:120" in text   # key lạ giữ nguyên
    assert "old.zip" not in text


def test_move_up_down(setup):
    rs, ctx, inst, zips = setup
    layers = LayerStore(ctx)
    layers.set_order(inst, ["pack-a.zip", "pack-b.zip"])
    # move pack-b lên (delta=-1 -> tăng priority)
    order = layers.move(inst, "pack-b.zip", -1)
    assert order == ["pack-b.zip", "pack-a.zip"]
    # move pack-b xuống lại
    order = layers.move(inst, "pack-b.zip", 1)
    assert order == ["pack-a.zip", "pack-b.zip"]


def test_unknown_instance_rejected(ctx):
    layers = LayerStore(ctx)
    with pytest.raises(AntaresError):
        layers.get_order("ghost-instance")
