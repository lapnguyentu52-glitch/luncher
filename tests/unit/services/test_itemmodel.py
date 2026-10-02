"""Item model pipeline (Batch 2 — master plan v3 mục 7).

Golden JSON tests: output model JSON khớp schema đã xác minh chính xác từng
field. Legacy < 1.21.4; definition >= 1.21.4 (datapack.wiki review 26.2 +
minecraft.wiki Items model definition — xác minh 2026-09-26).
"""
from __future__ import annotations

import json

from services.resources import itemmodel
from services.visuals import totem


# ----------------------------------------------------------------------
# Legacy generator (mục 7.2)
# ----------------------------------------------------------------------

def test_legacy_generator_golden():
    files = itemmodel.export_files("totem_of_undying",
                                   "minecraft:item/totem_of_undying", "1.21.3")
    # CHÍNH XÁC 1 file — game cũ không đọc items/
    assert list(files.keys()) == ["assets/minecraft/models/item/totem_of_undying.json"]
    assert files["assets/minecraft/models/item/totem_of_undying.json"] == {
        "parent": "item/generated",
        "textures": {"layer0": "minecraft:item/totem_of_undying"},
    }


# ----------------------------------------------------------------------
# Definition generator (mục 7.3) — golden JSON schema 1.21.4+
# ----------------------------------------------------------------------

def test_definition_generator_golden():
    files = itemmodel.export_files("totem_of_undying",
                                   "minecraft:item/totem_of_undying", "1.21.4")
    assert set(files.keys()) == {
        "assets/minecraft/items/totem_of_undying.json",
        "assets/minecraft/models/item/totem_of_undying.json",
    }
    # Definition file — schema chính xác theo nguồn xác minh
    assert files["assets/minecraft/items/totem_of_undying.json"] == {
        "model": {"type": "minecraft:model",
                  "model": "minecraft:item/totem_of_undying"},
    }
    # Model geometry file
    assert files["assets/minecraft/models/item/totem_of_undying.json"] == {
        "parent": "item/generated",
        "textures": {"layer0": "minecraft:item/totem_of_undying"},
    }


def test_definition_mode_for_all_modern_versions():
    for v in ("1.21.4", "1.21.6", "1.21.11", "26.2"):
        files = itemmodel.export_files("x", "minecraft:item/x", v)
        assert "assets/minecraft/items/x.json" in files, v
    for v in ("1.8.9", "1.20.1", "1.21.3"):
        files = itemmodel.export_files("x", "minecraft:item/x", v)
        assert "assets/minecraft/items/x.json" not in files, v


# ----------------------------------------------------------------------
# VoxelSpec -> elements compiler (mục 7.1, 8)
# ----------------------------------------------------------------------

def test_voxel_compiler_golden_simple():
    spec = {
        "grid": 16,
        "cubes": [{"id": "body", "from": [4, 0, 4], "to": [12, 10, 12],
                   "faces": {"north": {"texture": "base"}}}],
        "textures": {"base": "#e8b23a"},
    }
    files = itemmodel.voxel_item_files("totem_of_undying", spec)
    model = files["assets/minecraft/models/item/totem_of_undying.json"]

    assert model["textures"] == {"tex0": "antares:item/tex0"}
    (el,) = model["elements"]
    assert el["from"] == [4.0, 0.0, 4.0] and el["to"] == [12.0, 10.0, 12.0]
    # north có texture key; 5 mặt còn lại fallback màu vàng totem (WARN sau)
    assert el["faces"]["north"]["texture"] == "#tex0"
    assert el["faces"]["north"]["uv"] == [0.0, 0.0, 8.0, 10.0]     # (x, y) chiếu
    assert el["faces"]["up"]["uv"] == [0.0, 0.0, 8.0, 8.0]         # (x, z) chiếu
    assert el["faces"]["east"]["uv"] == [0.0, 0.0, 8.0, 10.0]      # (z, y) chiếu
    # PNG màu palette được báo kèm đúng path
    assert files["assets/antares/textures/item/tex0.png"] == ("png", "#e8b23a")


def test_voxel_compiler_uv_custom_and_asset_texture():
    spec = {
        "grid": 16,
        "cubes": [{"id": "face", "from": [0, 0, 0], "to": [16, 16, 1],
                   "faces": {"north": {"texture": "skin",
                                       "uv": [2, 3, 10, 11]}}}],
        "textures": {"skin": "assets/minecraft/textures/item/custom_face.png"},
    }
    compiled = itemmodel.compile_elements(spec)
    (el,) = compiled["model"]["elements"]
    assert el["faces"]["north"]["uv"] == [2.0, 3.0, 10.0, 11.0]
    slot = el["faces"]["north"]["texture"].lstrip("#")
    assert compiled["model"]["textures"][slot] == "minecraft:item/custom_face"
    assert compiled["assetTextures"] == ["minecraft:item/custom_face"]


def test_voxel_compiler_skips_zero_size_and_bad_cubes():
    spec = {"grid": 16, "cubes": [
        {"id": "flat", "from": [4, 4, 4], "to": [4, 8, 8]},        # zero size
        {"id": "bad", "from": "oops", "to": [8, 8, 8]},            # sai kiểu
        "not-a-dict",
        {"id": "ok", "from": [0, 0, 0], "to": [1, 1, 1]},
    ]}
    compiled = itemmodel.compile_elements(spec)
    assert len(compiled["model"]["elements"]) == 1


def test_voxel_export_legacy_drops_items_dir():
    spec = totem.voxel_spec()
    files = itemmodel.export_files("totem_of_undying",
                                   "minecraft:item/totem_of_undying",
                                   "1.20.1", spec=spec)
    assert not any(k.startswith("assets/minecraft/items/") for k in files)
    assert "assets/minecraft/models/item/totem_of_undying.json" in files


def test_voxel_export_definition_keeps_items_dir():
    spec = totem.voxel_spec()
    files = itemmodel.export_files("totem_of_undying",
                                   "minecraft:item/totem_of_undying",
                                   "1.21.11", spec=spec)
    assert "assets/minecraft/items/totem_of_undying.json" in files


# ----------------------------------------------------------------------
# totem.voxel_spec — Quick preset -> Advanced model (mục 70)
# ----------------------------------------------------------------------

def test_totem_voxel_spec_from_preset():
    spec = totem.voxel_spec({"base": "#ff0000", "accent": "#00ff00",
                             "eye": "#0000ff"})
    assert spec["grid"] == 16
    assert spec["textures"] == {"base": "#ff0000", "accent": "#00ff00",
                                "eye": "#0000ff"}
    ids = [c["id"] for c in spec["cubes"]]
    assert ids == ["body", "head", "eye-l", "eye-r"]
    body = spec["cubes"][0]
    assert body["from"] == [4, 0, 4] and body["to"] == [12, 10, 12]
    # Mắt chỉ định nghĩa mặt north — các mặt khác fallback an toàn
    assert spec["cubes"][2]["faces"] == {"north": {"texture": "eye"}}


def test_color_png_bytes():
    png = itemmodel.color_png_bytes("#e8b23a")
    assert png.startswith(b"\x89PNG\r\n\x1a\n")
    # IHDR: 1x1, bit depth 8, color type 6 (RGBA)
    assert (1, 1) == __import__("struct").unpack(">II", png[16:24])


# ----------------------------------------------------------------------
# Integration: export Totem Quick -> validate -> build theo version
# ----------------------------------------------------------------------

def _svc(ctx):
    from services.resources import ResourceStudioService
    return ResourceStudioService(ctx)


def test_totem_export_modern_build_roundtrip(ctx):
    from services.visuals import VisualStudioService
    svc = VisualStudioService(ctx)
    ctx.set("resource_studio", _svc(ctx))

    result = svc.export_totem_pack("My Totem", {}, "1.21.11")
    pid = result["projectId"]
    gen = ctx.paths.resource_studio / pid / "generated"

    # Texture Quick giữ nguyên (không regress hành vi Quick — gate Batch 2)
    assert (gen / "assets/minecraft/textures/item/totem_of_undying.png").is_file()
    # + model JSON definition branch
    definition = json.loads(
        (gen / "assets/minecraft/items/totem_of_undying.json").read_text("utf-8"))
    assert definition["model"]["type"] == "minecraft:model"

    rs = _svc(ctx)
    assert rs.validate(pid)["ok"] is True
    built = rs.build(pid)
    assert built["files"] >= 4


def test_totem_export_legacy_build_roundtrip(ctx):
    from services.visuals import VisualStudioService
    svc = VisualStudioService(ctx)
    ctx.set("resource_studio", _svc(ctx))

    result = svc.export_totem_pack("Old Totem", {}, "1.19.4")
    pid = result["projectId"]
    gen = ctx.paths.resource_studio / pid / "generated"
    model = json.loads(
        (gen / "assets/minecraft/models/item/totem_of_undying.json").read_text("utf-8"))
    assert model["parent"] == "item/generated"
    assert not (gen / "assets/minecraft/items").exists()

    rs = _svc(ctx)
    assert rs.validate(pid)["ok"] is True
    rs.build(pid)
