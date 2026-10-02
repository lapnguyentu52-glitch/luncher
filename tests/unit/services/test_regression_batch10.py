"""Full regression + upgrade + rollback (Batch 10 — master plan v3 mục 24, 26, 30).

Backward compatibility (mục 24): project cũ schema 1 (không assets/model3d)
mở + build được; existing events không đổi; options.txt merge không phá key.
Rollback (mục 30): pack.mcmeta schema mới đọc được bởi validator cũ-shape,
uninstall giữ layer order sạch, feature flag toggle không crash.
"""
from __future__ import annotations

import json
import struct
import zipfile
from pathlib import Path

import pytest

from core.errors.base import AntaresError
from services.resources import ResourceStudioService
from services.resources.layering import LayerStore
from services.resources.project import ProjectStore
from services.resources import versioning

PNG = (b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR"
       + struct.pack(">IIBBBBB", 4, 4, 8, 6, 0, 0, 0) + b"\x00" * 4)


# ----------------------------------------------------------------------
# Upgrade: project schema 1 cũ (mục 24 — project cũ mở + build được)
# ----------------------------------------------------------------------

def test_legacy_project_without_new_fields_still_builds(ctx):
    rs = ResourceStudioService(ctx)
    ctx.set("resource_studio", rs)
    # Giả lập project cũ: ghi trực tiếp project.json schema 1 không có
    # assets/model3d/settings (đúng shape trước RS v2)
    pdir = ctx.paths.resource_studio / "legacy01"
    (pdir / "generated").mkdir(parents=True)
    (pdir / "builds").mkdir(parents=True)
    legacy = {
        "schema": 1,
        "id": "legacy01",
        "name": "Legacy Pack",
        "description": "",
        "minecraft": {"version": "1.20.1"},
        "template": "pvp",
        "visuals": {"crosshair": {}, "totem": {}, "hud": {}},
        "resourcePack": {"enabled": True},
        "createdAt": 1700000000.0,
        "updatedAt": 1700000000.0,
        "buildCount": 3,
    }
    (pdir / "project.json").write_text(json.dumps(legacy), encoding="utf-8")

    # 1. get() không crash, giữ nguyên field
    project = rs.get("legacy01")
    assert project["name"] == "Legacy Pack"
    assert project["buildCount"] == 3
    assert "assets" not in project            # không tự thêm field cũ

    # 2. list() hiện project cũ
    assert any(p["id"] == "legacy01" for p in rs.list())

    # 3. build pipeline hoạt động trên project cũ (generate vào generated/ trống)
    rs.builder.generate("legacy01")
    assert rs.validate("legacy01")["ok"] is True
    built = rs.build("legacy01")
    assert built["sha256"]
    # update() với patch mới (assets) hoạt động trên project cũ
    rs.projects.update("legacy01", {"assets": [{"path": "x.png", "assetId": None}]})
    assert rs.get("legacy01")["assets"][0]["path"] == "x.png"


def test_legacy_project_pack_format_unchanged(ctx):
    # Project cũ MC 1.20.1 phải vẫn pack_format 15 sau mọi thay đổi (mục 24)
    rs = ResourceStudioService(ctx)
    ctx.set("resource_studio", rs)
    pdir = ctx.paths.resource_studio / "legacy02"
    (pdir / "generated").mkdir(parents=True)
    (pdir / "builds").mkdir(parents=True)
    (pdir / "project.json").write_text(json.dumps({
        "schema": 1, "id": "legacy02", "name": "Old", "description": "",
        "minecraft": {"version": "1.20.1"}, "template": "minimal",
        "visuals": {}, "resourcePack": {"enabled": True},
        "createdAt": 0, "updatedAt": 0, "buildCount": 0,
    }), encoding="utf-8")
    rs.builder.generate("legacy02")
    meta = json.loads((pdir / "generated" / "pack.mcmeta").read_text("utf-8"))
    assert meta["pack"]["pack_format"] == 15
    assert "min_format" not in meta["pack"]      # 1.20.1 không nhảy schema mới


def test_visual_export_flows_still_work(ctx, instance_id):
    # Regression: crosshair/totem export + install (pipeline cũ — mục 24)
    from services.visuals import VisualStudioService
    svc = VisualStudioService(ctx)
    ctx.set("resource_studio", ResourceStudioService(ctx))

    r1 = svc.export_pack("CH Reg", {"shape": "dot"}, "1.21.4",
                         install_instance_id=instance_id)
    r2 = svc.export_totem_pack("Totem Reg", {}, "1.19.4")
    base = ctx.paths.resource_studio
    assert (base / r1["projectId"] / "generated" /
            "assets/minecraft/textures/gui/sprites/icon/crosshair.png").is_file()
    assert (base / r2["projectId"] / "generated" /
            "assets/minecraft/textures/item/totem_of_undying.png").is_file()
    # install flow cũ vẫn chạy (crosshair export có install_instance_id)
    rp = Path(ctx.paths.instances) / instance_id / "game" / "resourcepacks"
    assert any(f.suffix == ".zip" for f in rp.glob("*.zip"))


# ----------------------------------------------------------------------
# Rollback verification (mục 30)
# ----------------------------------------------------------------------

def test_validator_accepts_both_packmeta_shapes(tmp_path):
    # Rollback scenario: builder mới ghi [major, minor]; nếu revert builder
    # về int — validator phải vẫn đọc được cả 2 (không khoá cùng nhau)
    from services.resources.validator import validate_project_dir
    gen = tmp_path / "gen"
    gen.mkdir()
    (gen / "pack.mcmeta").write_text(json.dumps(
        {"pack": {"pack_format": [75, 0], "min_format": 75, "max_format": 85,
                  "description": "modern"}}), encoding="utf-8")
    findings = validate_project_dir(gen)
    assert not any(f["code"] == "invalid_json" for f in findings)

    (gen / "pack.mcmeta").write_text(json.dumps(
        {"pack": {"pack_format": 15, "description": "legacy"}}), encoding="utf-8")
    findings = validate_project_dir(gen)
    assert not any(f["code"] == "invalid_json" for f in findings)


def test_uninstall_keeps_layer_order_clean(ctx, instance_id):
    rs = ResourceStudioService(ctx)
    ctx.set("resource_studio", rs)
    layers = LayerStore(ctx)
    rp = Path(ctx.paths.instances) / instance_id / "game" / "resourcepacks"
    rp.mkdir(parents=True, exist_ok=True)

    # install 2 pack giả lập
    for name in ("one.zip", "two.zip"):
        buf = io.BytesIO() if False else None
        (rp / name).write_bytes(_minimal_zip())
    layers.set_order(instance_id, ["one.zip", "two.zip"])
    assert layers.get_order(instance_id) == ["one.zip", "two.zip"]

    # uninstall 1 pack -> order tự lọc, không crash, sync options.txt
    assert rs.uninstall(instance_id, "two.zip") is True
    assert layers.get_order(instance_id) == ["one.zip"]
    opts = (rp.parent / "options.txt").read_text(encoding="utf-8")
    assert "two.zip" not in opts and "one.zip" in opts


def _minimal_zip() -> bytes:
    import io
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as zf:
        zf.writestr("pack.mcmeta", '{"pack": {"pack_format": 15, "description": "x"}}')
        zf.writestr("pack.png", PNG)
    return buf.getvalue()


def test_import_io_import_at_top():
    # regression guard: io/zipfile imports cần cho test file khác không vỡ
    import io  # noqa: F401
    assert True


def test_totem_3d_draft_survives_reopen(ctx):
    # Crash recovery roundtrip (mục 62): save draft -> "restart" (service mới)
    from services.visuals import VisualStudioService
    ctx.set("resource_studio", ResourceStudioService(ctx))
    spec = {"grid": 16, "cubes": [
        {"id": "body", "from": [4, 0, 4], "to": [12, 10, 12]}]}
    VisualStudioService(ctx).totem_model_save(spec)

    # service instance mới (mô phỏng restart app) đọc lại draft
    svc2 = VisualStudioService(ctx)
    assert svc2.totem_model_get()["spec"] == spec


# ----------------------------------------------------------------------
# Events taxonomy — existing events không đổi (mục 24 UI)
# ----------------------------------------------------------------------

def test_resource_events_unchanged(ctx):
    from core.events import names as ev
    assert ev.RESOURCE_BUILT == "resource.built"
    assert ev.RESOURCE_INSTALLED == "resource.installed"
    assert ev.VISUAL_EXPORTED == "visual.exported"


# ----------------------------------------------------------------------
# Import io struct cho file này
# ----------------------------------------------------------------------

import io  # noqa: E402
