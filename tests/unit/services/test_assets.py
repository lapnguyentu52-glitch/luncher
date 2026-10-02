"""Asset Library (Batch 5 — master plan v3 mục 12, 23).

Security matrix mục 12.2: size/dim limits, signature, malformed PNG,
filename normalize, path safety. Import -> assign -> build roundtrip.
"""
from __future__ import annotations

import base64
import json
import struct
import zlib

import pytest

from core.errors.base import AntaresError
from services.resources import ResourceStudioService
from services.resources.assets import AssetStore
from services.resources.templates import encode_png


@pytest.fixture()
def store(ctx) -> AssetStore:
    return AssetStore(ctx)


def _png(w: int = 16, h: int = 16, rgb=(232, 178, 58)) -> bytes:
    return encode_png(w, h, bytes((*rgb, 255)) * (w * h))       # RGBA 4B/px


# ----------------------------------------------------------------------
# Import security (mục 12.2)
# ----------------------------------------------------------------------

def test_import_valid_png(store):
    entry = store.import_png(_png(16, 16), name="My Asset!", category="item",
                             tags=["Food", "vanilla!!"])
    assert entry["width"] == 16 and entry["height"] == 16
    assert entry["name"] == "my-asset"                    # normalize
    assert entry["tags"] == ["food", "vanilla"]           # sanitize
    assert len(entry["sha256"]) == 64
    # File content-addressed tồn tại
    raw = store.read_png(entry["id"])
    assert raw == _png(16, 16)


def test_import_rejects_empty_and_huge(store):
    with pytest.raises(AntaresError):
        store.import_png(b"")
    with pytest.raises(AntaresError):
        store.import_png(b"x" * (8 * 1024 * 1024 + 1))


def test_import_rejects_non_png_signature(store):
    # fake .png extension không giúp gì (mục 12.2)
    with pytest.raises(AntaresError, match="signature"):
        store.import_png(b"PK\x03\x04" + b"\x00" * 100)         # ZIP magic
    with pytest.raises(AntaresError, match="signature"):
        store.import_png(b"<html><body>x</body></html>")


def test_import_rejects_malformed_png(store):
    sig_hdr = b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR" + \
        struct.pack(">IIBBBBB", 16, 16, 8, 6, 0, 0, 0)
    # IDAT rác -> decode fail
    bad = sig_hdr + struct.pack(">I", 10) + b"IDAT" + b"\x00" * 10
    with pytest.raises(AntaresError, match="decode"):
        store.import_png(bad)
    # dims 0
    ihdr0 = b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR" + \
        struct.pack(">IIBBBBB", 0, 0, 8, 6, 0, 0, 0)
    with pytest.raises(AntaresError):
        store.import_png(ihdr0)


def test_import_rejects_oversized_dims(store):
    # header khai báo 8192px vượt 4096 (decompression bomb guard)
    # IHDR chunk đủ 33 bytes: sig(8)+len(4)+tag(4)+body(13)+crc(4)
    fake = b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR" + \
        struct.pack(">IIBBBBB", 8192, 8192, 8, 6, 0, 0, 0) + b"\x00" * 4
    with pytest.raises(AntaresError, match="giới hạn"):
        store.import_png(fake)


def test_import_duplicate_hash_reuses_file(store, tmp_path):
    e1 = store.import_png(_png(), name="first")
    e2 = store.import_png(_png(), name="second")
    assert e1["sha256"] == e2["sha256"]
    assert e1["id"] != e2["id"]                     # 2 record
    files = list((tmp_path / "data" / "assets").glob("*.png"))
    assert len(files) == 1                          # 1 file content-addressed


def test_name_and_category_sanitize(store):
    e = store.import_png(_png(), name="../../etc/passwd", category="weird")
    assert "/" not in e["name"] and ".." not in e["name"]
    assert e["category"] == "item"                  # fallback


# ----------------------------------------------------------------------
# Query (mục 12.4)
# ----------------------------------------------------------------------

def test_list_search_filter_sort(store):
    store.import_png(_png(16, 16, (255, 0, 0)), name="apple", tags=["food"])
    store.import_png(_png(16, 16, (0, 255, 0)), name="sword", category="item")
    store.import_png(_png(32, 32, (0, 0, 255)), name="stone", category="block")

    assert len(store.list()) == 3
    assert [a["name"] for a in store.list(query="swor")] == ["sword"]
    assert all(a["category"] == "block" for a in store.list(category="block"))
    assert [a["name"] for a in store.list(tag="food")] == ["apple"]
    names = [a["name"] for a in store.list(sort="name")]
    assert names == sorted(names)
    assert store.list(sort="size")[0]["name"] == "stone"   # 32x32 lớn nhất


def test_get_delete_roundtrip(store):
    e = store.import_png(_png())
    assert store.get(e["id"])["name"] == e["name"]
    assert store.delete(e["id"]) is True
    with pytest.raises(AntaresError):
        store.get(e["id"])
    with pytest.raises(AntaresError):
        store.delete(e["id"])


def test_read_png_rejects_bad_hash(store):
    e = store.import_png(_png())
    store.get(e["id"])           # ok
    # giả mạo hash trong catalog -> chặn traversal qua read_png
    catalog_path = store._catalog_path
    catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
    catalog["assets"][0]["sha256"] = "../../etc/passwd"
    catalog_path.write_text(json.dumps(catalog), encoding="utf-8")
    with pytest.raises(AntaresError, match="hash"):
        store.read_png(e["id"])


# ----------------------------------------------------------------------
# Assign -> build roundtrip (mục 12 -> 16)
# ----------------------------------------------------------------------

def test_assign_to_project_and_build(ctx, store):
    rs = ResourceStudioService(ctx)
    proj = rs.create("AssetPack", "1.21.11", template="blank")
    pid = proj["id"]

    e = store.import_png(_png(16, 16, (255, 100, 0)), name="orange")
    out = store.assign_to_project(pid, e["id"],
                                  "assets/minecraft/textures/item/orange.png")
    assert out["path"] == "assets/minecraft/textures/item/orange.png"

    gen = ctx.paths.resource_studio / pid / "generated"
    assert (gen / "assets/minecraft/textures/item/orange.png").is_file()
    # project.json ghi nhận assignment
    project = rs.get(pid)
    assert project["assets"][0]["assetId"] == e["id"]

    assert rs.validate(pid)["ok"] is True
    built = rs.build(pid)
    assert built["files"] >= 3


def test_assign_rejects_bad_paths(ctx, store):
    rs = ResourceStudioService(ctx)
    proj = rs.create("P", "1.20.1", template="blank")
    e = store.import_png(_png())
    for evil in ("../../etc/x.png", "assets/../../x.png",
                 "assets/minecraft/models/item/x.png",     # không phải textures/
                 "assets/minecraft/textures/x.txt",        # không phải .png
                 "pack.png"):
        with pytest.raises(AntaresError):
            store.assign_to_project(proj["id"], e["id"], evil)


def test_assign_unknown_project_or_asset(ctx, store):
    rs = ResourceStudioService(ctx)
    proj = rs.create("P2", "1.20.1", template="blank")
    with pytest.raises(AntaresError):
        store.assign_to_project(proj["id"], "nope", "assets/minecraft/textures/item/x.png")
    e = store.import_png(_png())
    with pytest.raises(AntaresError):
        store.assign_to_project("../../../evil", e["id"],
                                "assets/minecraft/textures/item/x.png")
