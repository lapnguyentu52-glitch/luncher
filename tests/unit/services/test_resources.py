"""Resource Pack Studio — versioning, validate, build, install (mục 10, 33)."""
from __future__ import annotations

import json
import zipfile

from services.resources import ResourceStudioService
from services.resources import versioning


def test_pack_format_official_history():
    assert versioning.pack_format("1.8.9") == 1
    assert versioning.pack_format("1.13.2") == 4
    assert versioning.pack_format("1.16.5") == 6
    assert versioning.pack_format("1.20.1") == 15
    assert versioning.pack_format("1.20.3") == 22    # đổi theo PATCH
    assert versioning.pack_format("1.21.1") == 34
    assert versioning.pack_format("1.21.4") == 46
    assert versioning.pack_format("1.21.11") == 75   # 1.x mới nhất đã xác minh
    assert versioning.pack_format("26.2") == 88      # year-based versioning
    assert versioning.pack_format("27.0") == 88      # clamp version tương lai


def test_create_generates_assets_and_meta(ctx):
    rs = ResourceStudioService(ctx)
    proj = rs.create("PvP Pack", "1.20.1", template="pvp")
    gen = ctx.paths.resource_studio / proj["id"] / "generated"
    assert (gen / "pack.mcmeta").is_file()
    assert json.loads((gen / "pack.mcmeta").read_text(encoding="utf-8"))["pack"]["pack_format"] == 15
    assert (gen / "assets/minecraft/textures/gui/crosshair.png").is_file()
    assert (gen / "pack.png").read_bytes().startswith(b"\x89PNG\r\n\x1a\n")


def test_validate_build_install_roundtrip(ctx, instance_id):
    rs = ResourceStudioService(ctx)
    proj = rs.create("My Pack", "1.20.1", template="pvp")
    pid = proj["id"]

    assert rs.validate(pid)["ok"] is True
    built = rs.build(pid)
    assert built["sha256"] and built["files"] >= 3
    with zipfile.ZipFile(ctx.paths.resource_studio / pid / "builds" / built["file"]) as zf:
        assert "pack.mcmeta" in zf.namelist() and "pack.png" in zf.namelist()

    inst = rs.install(pid, instance_id)
    assert inst["backup"] is None
    inst2 = rs.install(pid, instance_id, overwrite=True)
    assert inst2["backup"]                       # overwrite có backup (mục 75)
    try:
        rs.install(pid, instance_id)             # không overwrite -> chặn
        assert False
    except Exception:
        pass
    assert len(rs.installed(instance_id)) == 1
    assert rs.uninstall(instance_id, built["file"]) is True


def test_validator_blocks_broken_pack(ctx):
    rs = ResourceStudioService(ctx)
    proj = rs.create("Broken", "1.21", template="blank")
    gen = ctx.paths.resource_studio / proj["id"] / "generated"
    stray_dir = gen / "assets" / "minecraft"
    stray_dir.mkdir(parents=True, exist_ok=True)   # blank template không sinh texture
    stray = stray_dir / "stray.txt"
    stray.write_text("oops")
    findings = rs.validate(proj["id"])
    assert findings["ok"] is False
    assert "wrong_path" in {f["code"] for f in findings["findings"]}
    try:
        rs.build(proj["id"])
        assert False, "build phải chặn ERROR"
    except Exception:
        pass


def test_path_traversal_blocked(ctx):
    rs = ResourceStudioService(ctx)
    for evil in ("../../etc", "a/b", "..\\x", ""):
        try:
            rs.projects.dir_of(evil)
            assert False, f"phải chặn {evil!r}"
        except Exception:
            pass
