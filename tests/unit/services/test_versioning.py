"""Version foundation (master plan v3 mục 6) — parse, pack_meta, item model
mode, policy version lạ. Bảng _FORMATS đã xác minh 2026-09-26 từ
minecraft.wiki/w/Pack_format + changelog chính thức Mojang.
"""
from __future__ import annotations

import json
import zipfile

from services.resources import ResourceStudioService
from services.resources import versioning


# ----------------------------------------------------------------------
# Parser (mục 6.1)
# ----------------------------------------------------------------------

def test_parse_shapes():
    assert versioning.parse("1.21.4") == (1, 21, 4)
    assert versioning.parse("1.21") == (1, 21, 0)
    assert versioning.parse("26.2") == (26, 2, 0)
    assert versioning.parse("1.21.11") == (1, 21, 11)


# ----------------------------------------------------------------------
# pack_format — matrix theo nguồn xác minh (mục 6.2)
# ----------------------------------------------------------------------

def test_pack_format_official_history():
    # (version, format) — từng mốc đều có test riêng (mục 6.2)
    matrix = [
        ("1.8.9", 1), ("1.9", 2), ("1.12.2", 3), ("1.13.2", 4),
        ("1.15.2", 5), ("1.16.5", 6), ("1.17.1", 7), ("1.18.2", 8),
        ("1.19.2", 9), ("1.19.3", 12), ("1.19.4", 13),
        ("1.20.1", 15), ("1.20.2", 18), ("1.20.3", 22), ("1.20.6", 32),
        ("1.21", 34), ("1.21.1", 34), ("1.21.3", 42), ("1.21.4", 46),
        ("1.21.5", 55), ("1.21.6", 63), ("1.21.7", 64), ("1.21.8", 64),
        ("1.21.9", 69), ("1.21.10", 69), ("1.21.11", 75),
        ("26.1", 84), ("26.2", 88),
    ]
    for ver, fmt in matrix:
        assert versioning.pack_format(ver) == fmt, f"{ver} -> {fmt}"


def test_pack_format_value_minor_shape():
    # 69.0+ có minor — ghi [major, minor]; cũ hơn là int thuần
    assert versioning.pack_format_value("1.21.8") == 64
    assert versioning.pack_format_value("1.21.9") == [69, 0]
    assert versioning.pack_format_value("1.21.11") == [75, 0]
    assert versioning.pack_format_value("26.2") == [88, 0]


# ----------------------------------------------------------------------
# Policy version lạ (mục 6.2: unknown/future không crash, không fallback im lặng)
# ----------------------------------------------------------------------

def test_unknown_future_version_clamps_with_warning():
    # Future version (year-based, >= 26.3) -> clamp về mốc mới nhất + WARNING
    assert versioning.pack_format("26.3") == 88
    assert versioning.pack_format("27.0") == 88
    warnings = versioning.version_warnings("27.0")
    assert len(warnings) == 1 and "mới hơn" in warnings[0]
    vi = versioning.info("27.0")
    assert vi.clamped is True


def test_future_one_x_version_clamps_to_latest_1x():
    # "1.99.0" -> (1, 99, 0): lớn hơn 1.21.11 nhưng NHỎ hơn 26.1 (major 1 < 26)
    # -> clamp về mốc 1.x mới nhất đã xác minh (1.21.11 = 75)
    assert versioning.pack_format("1.99.0") == 75


def test_prehistoric_version_uses_format_1_with_warning():
    assert versioning.pack_format("1.5.2") == 1
    warnings = versioning.version_warnings("1.5.2")
    assert len(warnings) == 1 and "cũ hơn" in warnings[0]


def test_verified_version_has_no_warning():
    assert versioning.version_warnings("1.21.4") == []
    assert versioning.info("1.21.4").clamped is False


def test_garbage_version_does_not_crash():
    # Input rác -> parse về (0, 0, 0) -> nhánh prehistoric, không raise
    assert versioning.pack_format("not-a-version") == 1
    assert versioning.pack_format("") == 1


# ----------------------------------------------------------------------
# pack_meta — schema legacy vs modern (mục 6.3)
# ----------------------------------------------------------------------

def test_pack_meta_legacy_shape():
    meta = versioning.pack_meta("1.20.1")
    assert meta["pack"]["pack_format"] == 15
    assert isinstance(meta["pack"]["pack_format"], int)
    assert "min_format" not in meta["pack"]
    assert "max_format" not in meta["pack"]
    assert "description" in meta["pack"]


def test_pack_meta_min_max_shape_from_1_21_9():
    meta = versioning.pack_meta("1.21.11")
    pack = meta["pack"]
    assert pack["min_format"] == 75
    assert pack["max_format"] > pack["min_format"]
    assert pack["pack_format"] == [75, 0]     # tương thích game cũ hơn format 65


def test_mcmeta_mode():
    assert versioning.mcmeta_mode("1.21.8") == "pack_format"
    assert versioning.mcmeta_mode("1.21.9") == "min_max"
    assert versioning.mcmeta_mode("26.2") == "min_max"


# ----------------------------------------------------------------------
# Item model mode (mục 7 — dùng cho Batch 2)
# ----------------------------------------------------------------------

def test_item_model_mode():
    assert versioning.item_model_mode("1.21.3") == "legacy"
    assert versioning.item_model_mode("1.21.4") == "definition"
    assert versioning.item_model_mode("1.21.11") == "definition"


# ----------------------------------------------------------------------
# Crosshair path (1.20.2 đổi sang sprites/)
# ----------------------------------------------------------------------

def test_crosshair_path_version_aware():
    assert versioning.crosshair_path("1.20.1") == \
        "assets/minecraft/textures/gui/crosshair.png"
    assert versioning.crosshair_path("1.20.2") == \
        "assets/minecraft/textures/gui/sprites/icon/crosshair.png"


# ----------------------------------------------------------------------
# supported_versions cho wizard (không hardcode frontend)
# ----------------------------------------------------------------------

def test_supported_versions_ordered_latest_first():
    # Mỗi mục = version mở đầu của 1 format range (vd 1.6.1 đại diện format 1
    # dùng được đến 1.8.9) — thứ tự mới nhất trước cho wizard (mục 49)
    versions = versioning.supported_versions()
    assert versions[0] == "26.2"
    assert "1.21.11" in versions and "1.6.1" in versions
    assert versions.index("26.2") < versions.index("1.21.11") < versions.index("1.6.1")
    assert len(versions) == len(set(versions))


# ----------------------------------------------------------------------
# Integration: builder sinh pack.mcmeta đúng schema theo version
# ----------------------------------------------------------------------

def test_builder_pack_meta_legacy(ctx):
    rs = ResourceStudioService(ctx)
    proj = rs.create("Legacy", "1.20.1", template="minimal")
    gen = ctx.paths.resource_studio / proj["id"] / "generated"
    meta = json.loads((gen / "pack.mcmeta").read_text(encoding="utf-8"))
    assert meta["pack"]["pack_format"] == 15
    assert "min_format" not in meta["pack"]


def test_builder_pack_meta_modern(ctx):
    rs = ResourceStudioService(ctx)
    proj = rs.create("Modern", "1.21.11", template="minimal")
    gen = ctx.paths.resource_studio / proj["id"] / "generated"
    meta = json.loads((gen / "pack.mcmeta").read_text(encoding="utf-8"))
    assert meta["pack"]["min_format"] == 75
    assert meta["pack"]["pack_format"] == [75, 0]

    # Build + ZIP vẫn pass validator với schema mới
    built = rs.build(proj["id"])
    with zipfile.ZipFile(ctx.paths.resource_studio / proj["id"] / "builds" / built["file"]) as zf:
        names = zf.namelist()
        assert "pack.mcmeta" in names
        zf.read("pack.mcmeta")


def test_wizard_info_exposes_versions(ctx):
    rs = ResourceStudioService(ctx)
    info = rs.wizard_info()
    assert info["versions"] and info["defaultVersion"] == info["versions"][0]
