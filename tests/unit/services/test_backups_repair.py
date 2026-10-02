"""Backup Center (mục 22) + Repair Center (mục 39)."""
from __future__ import annotations

import json
import zipfile

import pytest

from core.errors.base import AntaresError
from services.backups import BackupService
from services.repair import RepairService


# ------------------------------------------------------------------
# Backup Center
# ------------------------------------------------------------------

def test_snapshot_restore_roundtrip(ctx, instance_id):
    bk = BackupService(ctx)
    mods = ctx.paths.instances / instance_id / "game" / "mods"
    (mods / "sodium.jar").write_bytes(b"SODIUM")
    ctx.config.set("selectedInstance", instance_id, flush_now=True)

    meta = bk.create(instance_id=instance_id, targets=["instance", "config"], label="v1")
    assert meta["files"] >= 2

    # Phá state
    (mods / "sodium.jar").unlink()
    (mods / "bad.jar").write_bytes(b"BAD")
    ctx.config.set("selectedInstance", "other", flush_now=True)

    r = bk.restore(meta["id"])
    assert (mods / "sodium.jar").read_bytes() == b"SODIUM"     # bị xoá -> quay lại
    assert not (mods / "bad.jar").exists()                     # thêm sau -> dọn
    assert ctx.config.get("selectedInstance") == instance_id   # config reload cả cache
    # Pre-restore snapshot tự tạo (mục 74)
    labels = [s.get("label") or "" for s in bk.list()]
    assert any(l.startswith("pre-restore") for l in labels)


def test_restore_is_two_way(ctx, instance_id):
    bk = BackupService(ctx)
    mods = ctx.paths.instances / instance_id / "game" / "mods"
    (mods / "a.jar").write_bytes(b"A")
    snap_id = bk.create(instance_id=instance_id, targets=["mods"])["id"]

    (mods / "b.jar").write_bytes(b"B")
    pre = bk.restore(snap_id)["preRestoreSnapshotId"]
    assert (mods / "b.jar").exists() is False

    bk.restore(pre)                      # undo restore
    assert (mods / "b.jar").read_bytes() == b"B"


def test_export_import_zip(ctx, instance_id):
    bk = BackupService(ctx)
    snap = bk.create(instance_id=instance_id, targets=["config"], label="zip-me")
    zip_path = bk.export_zip(snap["id"])
    assert zip_path.is_file() and zip_path.suffix == ".zip"
    assert bk.export_zip(snap["id"]) == zip_path         # idempotent
    assert bk.import_zip(str(zip_path))["id"] == snap["id"]


def test_import_zip_bomb_guard(ctx, monkeypatch):
    """§128 — archive vượt cap uncompressed → reject, không giải nén gì."""
    import services.backups.service as bk_mod

    bk = BackupService(ctx)
    snap = bk.create(targets=["config"], label="v1")
    zip_path = bk.export_zip(snap["id"])
    monkeypatch.setattr(bk_mod, "_MAX_IMPORT_BYTES", 5)
    with pytest.raises(AntaresError):
        bk.import_zip(str(zip_path))
    assert (bk._backups / snap["id"] / "metadata.json").is_file()


def test_import_keeps_existing_snapshot_if_extract_fails(ctx, monkeypatch):
    """§4.4 — extract fail giữa chừng → snapshot cũ cùng id không bị mất
    (code cũ rmtree TRƯỚC extractall)."""
    bk = BackupService(ctx)
    snap = bk.create(targets=["config"], label="v1")
    zip_path = bk.export_zip(snap["id"])

    def boom(self, path):
        raise OSError("disk full")

    monkeypatch.setattr(zipfile.ZipFile, "extractall", boom)
    with pytest.raises(OSError):
        bk.import_zip(str(zip_path))
    assert (bk._backups / snap["id"] / "metadata.json").is_file()
    # không còn thư mục tạm .importing-* sót
    assert not [p for p in bk._backups.glob(".importing-*")]


def test_backup_validation(ctx):
    bk = BackupService(ctx)
    for bad in (
        lambda: bk.create(targets=["sai"]),
        lambda: bk.create(instance_id="ghost", targets=["instance"]),
        lambda: bk._snap_dir("../../etc"),
    ):
        try:
            bad()
            assert False
        except Exception:
            pass


# ------------------------------------------------------------------
# Repair Center
# ------------------------------------------------------------------

def test_repair_scan_is_dry_run(ctx, instance_id):
    rp = RepairService(ctx)
    orphan = ctx.paths.instances / "orphan-junk"
    orphan.mkdir()
    try:
        scan = rp.scan("instance_metadata")
        assert any("orphan-junk" in p["path"] for p in scan["planned"])
        assert orphan.exists(), "dry-run không được xoá"
    finally:
        import shutil
        shutil.rmtree(orphan, ignore_errors=True)


def test_repair_fixes_all_issues(ctx, instance_id):
    rp = RepairService(ctx)
    game = ctx.paths.instances / instance_id / "game"

    # Phá: thiếu dir + duplicate options + stale .part + corrupt manifest + broken zip
    (game / "saves").rmdir()
    (game / "options.txt").write_text("renderDistance:12\nrenderDistance:6\n", encoding="utf-8")
    (ctx.paths.cache / "downloads" / "x.part").parent.mkdir(parents=True, exist_ok=True)
    (ctx.paths.cache / "downloads" / "x.part").write_bytes(b"p")
    (ctx.paths.cache / "manifests" / "bad.json").parent.mkdir(parents=True, exist_ok=True)
    (ctx.paths.cache / "manifests" / "bad.json").write_text("{broken", encoding="utf-8")
    broken = game / "resourcepacks" / "broken.zip"
    with zipfile.ZipFile(broken, "w") as zf:
        zf.writestr("nope.txt", "x")     # thiếu pack.mcmeta

    rp.run("missing_dirs")
    assert (game / "saves").is_dir()

    rp.run("option_files")
    lines = (game / "options.txt").read_text().splitlines()
    assert [l for l in lines if l.startswith("renderDistance:")] == ["renderDistance:12"]

    rp.run("downloads")
    assert not (ctx.paths.cache / "downloads" / "x.part").exists()

    rp.run("caches")
    assert not (ctx.paths.cache / "manifests" / "bad.json").exists()

    rp.run("resource_packs")
    assert not broken.exists()

    # Sạch
    for a in ("missing_dirs", "option_files", "downloads", "caches", "resource_packs"):
        assert not rp.scan(a)["hasIssues"], a


def test_repair_unknown_action_blocked(ctx):
    rp = RepairService(ctx)
    try:
        rp.scan("khong-ton-tai")
        assert False
    except Exception:
        pass


def test_repair_event_fires(ctx, instance_id):
    seen = []
    ctx.events.subscribe("repair.run", lambda e: seen.append(e.payload["action"]))
    RepairService(ctx).run("downloads")
    assert seen == ["downloads"]
