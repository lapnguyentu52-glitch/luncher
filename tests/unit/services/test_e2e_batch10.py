"""End-to-end integration (Batch 10 — master plan v3 mục 26).

Một flow chạy toàn bộ pipeline RS v2: create -> ZIP import -> asset
assign -> Totem 3D draft -> validate -> build -> install -> layer -> sync.
Đây là regression test "user journey" — nếu 1 mắt xích vỡ, test fail.
"""
from __future__ import annotations

import io
import json
import struct
import zipfile
from pathlib import Path

from services.resources import ResourceStudioService
from services.resources.assets import AssetStore
from services.resources.layering import LayerStore
from services.resources.templates import encode_png
from services.visuals import VisualStudioService

# PNG RGBA 8x8 hợp lệ (encode thật — qua full decode pipeline ở assets)
PNG = encode_png(8, 8, bytes((232, 178, 58, 255)) * 64)


def _zip_with_texture(entries: dict[str, bytes]) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
        for name, data in entries.items():
            zf.writestr(name, data)
    return buf.getvalue()


def test_full_user_journey(ctx, instance_id):
    # Setup services như bootstrap
    rs = ResourceStudioService(ctx)
    ctx.set("resource_studio", rs)
    assets = AssetStore(ctx)
    ctx.set("assets", assets)
    visual = VisualStudioService(ctx)

    # ---------- 1. Create project ----------
    proj = rs.create("Journey Pack", "1.21.11", template="blank")
    pid = proj["id"]

    # ---------- 2. ZIP import (mục 13) ----------
    zp = Path(ctx.paths.data) / "journey.zip"
    zp.write_bytes(_zip_with_texture({
        "assets/minecraft/textures/item/gem.png": PNG,
        "assets/minecraft/textures/item/gem.json": '{"parent": "item/generated"}',
    }))
    out = rs.zip_import(pid, zp, policy="keep")
    assert len(out["imported"]) == 2

    # ---------- 3. Asset import + assign (mục 12) ----------
    entry = assets.import_png(PNG, name="gem-alt", category="item")
    assigned = assets.assign_to_project(
        pid, entry["id"], "assets/minecraft/textures/item/gem-alt.png")
    assert assigned["path"].endswith("gem-alt.png")
    # ZIP import đã ghi project.assets — assign cộng thêm
    paths = {a["path"] for a in rs.get(pid)["assets"]}
    assert "assets/minecraft/textures/item/gem.png" in paths
    assert "assets/minecraft/textures/item/gem-alt.png" in paths

    # ---------- 4. Totem 3D draft (mục 62) + export kèm model ----------
    spec = {"grid": 16, "cubes": [
        {"id": "body", "from": [4, 0, 4], "to": [12, 10, 12],
         "faces": {"north": {"texture": "gem"}}}],
        "textures": {"gem": "assets/minecraft/textures/item/gem.png"}}
    assert visual.totem_model_save(spec)["saved"]

    # Export totem pack với version definition branch (mục 7.3)
    result = visual.export_totem_pack("Journey Totem", {}, "1.21.11",
                                      install_instance_id=instance_id)
    totem_pid = result["projectId"]
    gen = ctx.paths.resource_studio / totem_pid / "generated"
    assert (gen / "assets/minecraft/items/totem_of_undying.json").is_file()

    # ---------- 5. Validate + build + install (mục 14/16) ----------
    assert rs.validate(totem_pid)["ok"] is True
    built = rs.build(totem_pid)
    assert built["sha256"]
    with zipfile.ZipFile(ctx.paths.resource_studio / totem_pid / "builds" / built["file"]) as zf:
        names = zf.namelist()
        assert "pack.mcmeta" in names
        assert "assets/minecraft/items/totem_of_undying.json" in names
    # install đã xảy ra trong export_totem_pack
    rp = Path(ctx.paths.instances) / instance_id / "game" / "resourcepacks"
    installed = {f.name for f in rp.glob("*.zip")}
    assert built["file"] in installed

    # ---------- 6. Layer order + options.txt sync (mục 25) ----------
    layers = LayerStore(ctx)
    order = layers.set_order(instance_id, [built["file"]])
    assert order == [built["file"]]
    opts = (rp.parent / "options.txt").read_text(encoding="utf-8")
    assert built["file"] in opts

    # ---------- 7. Effective asset resolution — pack duy nhất cung cấp ----------
    eff = layers.effective_asset(instance_id,
                                 "assets/minecraft/items/totem_of_undying.json")
    assert eff and eff["pack"] == built["file"]     # definition json có trong pack
    eff = layers.effective_asset(instance_id, "pack.mcmeta")
    assert eff and eff["pack"] == built["file"]

    # ---------- 8. Draft persistence sau "restart" ----------
    assert visual.totem_model_get()["spec"] == spec

    # ---------- 9. Uninstall + layer tự lọc ----------
    assert rs.uninstall(instance_id, built["file"]) is True
    assert built["file"] not in layers.get_order(instance_id)
    opts = (rp.parent / "options.txt").read_text(encoding="utf-8")
    assert built["file"] not in opts
