"""Skin & Cape Studio — import/generate/apply/preview + PNG layout (mục 43)."""
from __future__ import annotations

import base64
import hashlib
import struct
import zlib

import pytest

from core.errors import codes
from core.errors.base import AntaresError
from core.events import names as ev
from services.skins.service import SkinCapeService, encode_png
from services.visuals.atlas import decode_png_rgba


@pytest.fixture()
def svc(ctx):
    ctx.set("skins", SkinCapeService(ctx))
    return ctx.get("skins")


def png_bytes(width: int, height: int, rgba: bytes | None = None) -> bytes:
    if rgba is None:
        rgba = bytes((200, 100, 50, 255)) * (width * height)
    return encode_png(width, height, rgba)


# ------------------------------------------------------------------
# Import + validate
# ------------------------------------------------------------------

def test_import_skin_modern_64x64(svc):
    entry = svc.import_skin(png_bytes(64, 64), "My Skin")
    assert entry["kind"] == "skin"
    assert entry["width"] == 64 and entry["height"] == 64
    assert entry["converted"] is False
    assert (svc._files_dir() / f"{entry['sha256']}.png").is_file()


def test_import_skin_legacy_64x32_auto_upgrades(svc):
    entry = svc.import_skin(png_bytes(64, 32), "Old Skin")
    assert entry["width"] == 64 and entry["height"] == 64
    assert entry["converted"] is True
    # File vật lý giờ là 64x64
    data = (svc._files_dir() / f"{entry['sha256']}.png").read_bytes()
    w, h = struct.unpack(">II", data[16:24])
    assert (w, h) == (64, 64)


def test_import_skin_wrong_size_rejected(svc):
    with pytest.raises(AntaresError) as ei:
        svc.import_skin(png_bytes(128, 128), "big")
    assert ei.value.code == codes.VALIDATION_FAILED
    assert "64" in ei.value.message


def test_import_skin_not_png_rejected(svc):
    with pytest.raises(AntaresError):
        svc.import_skin(b"not a png at all" * 10, "junk")


def test_import_skin_too_large_rejected(svc):
    with pytest.raises(AntaresError) as ei:
        svc.import_skin(b"\x89PNG\r\n\x1a\n" + b"\x00" * (9 * 1024 * 1024), "big")
    assert ei.value.code == codes.ASSET_TOO_LARGE


def test_import_cape_92x64(svc):
    entry = svc.import_cape(png_bytes(92, 64), "My Cape")
    assert entry["kind"] == "cape"
    assert (entry["width"], entry["height"]) == (92, 64)


def test_import_cape_wrong_size_rejected(svc):
    with pytest.raises(AntaresError):
        svc.import_cape(png_bytes(64, 64), "nope")


def test_import_duplicate_content_reuses_file(svc):
    a = svc.import_skin(png_bytes(64, 64), "one")
    b = svc.import_skin(png_bytes(64, 64), "two")
    assert a["sha256"] == b["sha256"]
    assert a["id"] != b["id"]          # 2 record, 1 file content-addressed


def test_import_publishes_event(ctx, svc):
    seen = []
    ctx.events.subscribe(ev.SKINS_CHANGED, lambda e: seen.append(e.payload["action"]))
    svc.import_skin(png_bytes(64, 64), "evt")
    assert seen == ["import"]


# ------------------------------------------------------------------
# Generate
# ------------------------------------------------------------------

def test_generate_skin_steve_style(svc):
    entry = svc.generate("skin", "cool_guy", "#c8965a", "#3b2a1a")
    assert entry["kind"] == "skin"
    assert (entry["width"], entry["height"]) == (64, 64)
    path = svc._files_dir() / f"{entry['sha256']}.png"
    w, h, rgba = decode_png_rgba(path.read_bytes())
    assert (w, h) == (64, 64)
    # Head front tại (8..15, 8..15) phải không trong suốt
    idx = (10 * 64 + 10) * 4
    assert rgba[idx + 3] == 255
    # Vùng ngoài canvas UV (ví dụ (60..63, 60..63)) alpha = 0
    idx2 = (62 * 64 + 62) * 4
    assert rgba[idx2 + 3] == 0


def test_generate_cape(svc):
    entry = svc.generate("cape", "hero_cape", "#2040a0")
    assert (entry["width"], entry["height"]) == (92, 64)
    w, h, rgba = decode_png_rgba(
        (svc._files_dir() / f"{entry['sha256']}.png").read_bytes())
    assert (w, h) == (92, 64)


def test_generate_rejects_bad_color_and_kind(svc):
    with pytest.raises(AntaresError):
        svc.generate("skin", "x", "red")            # không phải hex
    with pytest.raises(AntaresError):
        svc.generate("hat", "x", "#aabbcc")         # sai kind
    with pytest.raises(AntaresError):
        svc.generate("skin", "BAD NAME!!", "#aabbcc")  # tên không safe


def test_generate_publishes_event(ctx, svc):
    seen = []
    ctx.events.subscribe(ev.SKINS_CHANGED, lambda e: seen.append(e.payload["action"]))
    svc.generate("skin", "gen", "#aabbcc")
    assert seen == ["generate"]


# ------------------------------------------------------------------
# List / get / delete / preview
# ------------------------------------------------------------------

def test_list_kinds(svc):
    svc.import_skin(png_bytes(64, 64), "s1")
    svc.import_cape(png_bytes(92, 64), "c1")
    out = svc.list()
    assert len(out["skins"]) == 1 and len(out["capes"]) == 1
    assert len(svc.list("skin")["skins"]) == 1
    assert "capes" not in svc.list("skin")


def test_delete_requires_confirm_and_blocks_applied(svc, ctx, instance_id):
    entry = svc.import_skin(png_bytes(64, 64), "del-me")
    with pytest.raises(AntaresError):
        svc.delete("skin", entry["id"])            # thiếu confirm
    svc.apply(instance_id, "skin", entry["id"])
    with pytest.raises(AntaresError):              # đang áp -> chặn
        svc.delete("skin", entry["id"], confirm=True)
    svc.unapply(instance_id, "skin")
    assert svc.delete("skin", entry["id"], confirm=True) is True
    assert svc.get("skin", entry["id"]) is None


def test_preview_data_uri(svc):
    entry = svc.import_skin(png_bytes(64, 64), "prev")
    uri = svc.png_data_uri("skin", entry["id"])
    assert uri.startswith("data:image/png;base64,")
    raw = base64.b64decode(uri.split(",", 1)[1])
    assert raw.startswith(b"\x89PNG")
    assert svc.png_data_uri("skin", "ghost") is None


# ------------------------------------------------------------------
# Apply per-instance
# ------------------------------------------------------------------

def test_apply_unapply_roundtrip(svc, ctx, instance_id):
    skin = svc.import_skin(png_bytes(64, 64), "ap")
    cape = svc.import_cape(png_bytes(92, 64), "cp")
    svc.apply(instance_id, "skin", skin["id"])
    svc.apply(instance_id, "cape", cape["id"])
    applied = svc.applied(instance_id)
    assert applied["skin"]["id"] == skin["id"]
    assert applied["skin"]["file"].endswith(f"{skin['sha256']}.png")
    assert applied["cape"]["id"] == cape["id"]

    svc.unapply(instance_id, "skin")
    applied2 = svc.applied(instance_id)
    assert "skin" not in applied2 and "cape" in applied2


def test_apply_unknown_item_and_instance(svc, ctx):
    with pytest.raises(AntaresError) as ei:
        svc.apply("ghost-instance", "skin", "x")
    assert ei.value.code == codes.INSTANCE_NOT_FOUND
    ctx.get("instances").create("I2", "1.21")
    with pytest.raises(AntaresError):
        svc.apply(ctx.config.get("selectedInstance") or "x", "skin", "ghost-item")


def test_apply_none_clears_slot(svc, ctx, instance_id):
    skin = svc.import_skin(png_bytes(64, 64), "clr")
    svc.apply(instance_id, "skin", skin["id"])
    svc.apply(instance_id, "skin", None)
    assert "skin" not in svc.applied(instance_id)


def test_apply_publishes_event(ctx, svc, instance_id):
    seen = []
    ctx.events.subscribe(ev.SKIN_APPLIED, lambda e: seen.append(e.payload))
    skin = svc.import_skin(png_bytes(64, 64), "ev2")
    svc.apply(instance_id, "skin", skin["id"])
    assert seen[-1] == {"instanceId": instance_id, "kind": "skin", "itemId": skin["id"]}


def test_state_persists_across_service_instances(ctx, svc, instance_id):
    skin = svc.import_skin(png_bytes(64, 64), "pers")
    svc.apply(instance_id, "skin", skin["id"])
    svc2 = SkinCapeService(ctx)
    assert svc2.applied(instance_id)["skin"]["id"] == skin["id"]


# ------------------------------------------------------------------
# encode_png layout (nhất quán stub PNG dự án)
# ------------------------------------------------------------------

def test_encode_png_layout():
    data = encode_png(2, 2, bytes((255, 0, 0, 255)) * 4)
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    assert data[12:16] == b"IHDR"
    w, h = struct.unpack(">II", data[16:24])
    assert (w, h) == (2, 2)
    assert zlib.crc32(b"IHDR" + data[16:29]) & 0xFFFFFFFF == struct.unpack(
        ">I", data[29:33])[0]
