"""ZIP pack importer (Batch 6 — master plan v3 mục 13, 23, 93).

Security matrix mục 13.2: zip-slip, absolute Windows, UNC, ../, nested
archive, duplicate filename, huge file/count, malformed ZIP, symlink-like
entry, overwrite policy, rollback. Conflict policies mục 93:
keep/replace/keep_both + per-file overrides.
"""
from __future__ import annotations

import io
import struct
import zipfile
from pathlib import Path

import pytest

from core.errors.base import AntaresError
from services.resources import ResourceStudioService
from services.resources.importer import (
    ZipSecurityError,
    import_zip,
    inspect_zip,
    sanitize_entry_name,
)

PNG = (b"\x89PNG\r\n\x1a\n" + struct.pack(">I", 13) + b"IHDR"
       + struct.pack(">IIBBBBB", 4, 4, 8, 6, 0, 0, 0) + b"\x00" * 4)


def _zip(entries: dict[str, bytes]) -> bytes:
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_DEFLATED) as zf:
        for name, data in entries.items():
            zf.writestr(name, data)
    return buf.getvalue()


def _write_zip(ctx, entries: dict[str, bytes]) -> Path:
    p = Path(ctx.paths.data) / "tmp-zips"
    p.mkdir(parents=True, exist_ok=True)
    zp = p / "pack.zip"
    zp.write_bytes(_zip(entries))
    return zp


@pytest.fixture()
def proj_id(ctx):
    rs = ResourceStudioService(ctx)
    ctx.set("resource_studio", rs)     # importer lấy qua ctx.get
    return rs.create("ZipPack", "1.21.11", template="blank")["id"]


# ----------------------------------------------------------------------
# sanitize_entry_name — path validation (mục 13.1)
# ----------------------------------------------------------------------

def test_sanitize_rejects_evil_paths():
    for evil in ("../../etc/passwd", "..\\windows\\evil.png", "/abs/path.png",
                 "C:/Windows/evil.png", "C:\\evil.png", "\\\\server\\share\\x.png",
                 "//server/share/x.png", "a/../b.png", "..", "",
                 "x\x01y.png"):
        with pytest.raises(ZipSecurityError):
            sanitize_entry_name(evil)


def test_sanitize_normalizes_good_paths():
    assert sanitize_entry_name("assets/minecraft/textures/item/x.png") == \
        "assets/minecraft/textures/item/x.png"
    assert sanitize_entry_name("assets\\minecraft\\item\\y.png") == \
        "assets/minecraft/item/y.png"
    assert sanitize_entry_name("./assets/./z.png") == "assets/z.png"


# ----------------------------------------------------------------------
# inspect — không ghi gì, trả findings (mục 13.1)
# ----------------------------------------------------------------------

def test_inspect_clean_pack(proj_id, ctx):
    zp = _write_zip(ctx, {
        "assets/minecraft/textures/item/a.png": PNG,
        "assets/minecraft/textures/block/b.json": b'{"a": 1}',
    })
    rs = ResourceStudioService(ctx)
    report = rs.zip_inspect(zp)
    assert report["ok"] is True
    assert len(report["entries"]) == 2
    assert report["totals"]["bytes"] == len(PNG) + len(b'{"a": 1}')


def test_inspect_flags_all_evil_entries(proj_id, ctx):
    zp = _write_zip(ctx, {
        "../evil.png": PNG,                                  # zip-slip
        "/abs.png": PNG,                                     # absolute
        "assets/x.jar": PNG,                                 # nested archive
        "assets/weird.exe": b"MZ",                           # ext không hợp lệ
        "pack.mcmeta": b"{}",                                # skip name
        "not-assets/loose.png": PNG,                         # ngoài assets/
    })
    rs = ResourceStudioService(ctx)
    report = rs.zip_inspect(zp)
    codes = {f["code"] for f in report["findings"]}
    assert "zip_slip" in codes
    assert "zip_nested_archive" in codes
    assert "zip_unsupported_entry" in codes
    assert "zip_skipped_entry" in codes
    assert report["ok"] is False        # có ERROR


def test_inspect_duplicate_after_normalize(ctx):
    # 2 tên khác cách viết nhưng cùng path sau normalize
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as zf:
        zf.writestr("assets/minecraft/x.png", PNG)
        zf.writestr("assets\\minecraft\\x.png", PNG)
    with zipfile.ZipFile(io.BytesIO(buf.getvalue())) as zf:
        report = inspect_zip(zf)
    assert any(f["code"] == "zip_duplicate_entry" for f in report["findings"])


def test_inspect_symlink_entry_rejected(ctx):
    # external_attr symlink mode (0xA1FF << 16)
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as zf:
        info = zipfile.ZipInfo("assets/minecraft/link.png")
        info.external_attr = (0o120777 << 16)
        zf.writestr(info, PNG)
    with zipfile.ZipFile(io.BytesIO(buf.getvalue())) as zf:
        report = inspect_zip(zf)
    assert any(f["code"] == "zip_symlink" for f in report["findings"])
    assert report["ok"] is False


def test_inspect_malformed_crc(ctx):
    # ZIP STORED (không nén) rồi flip 1 byte trong local data (giữ nguyên
    # central dir) -> testzip() phát hiện CRC mismatch.
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w", zipfile.ZIP_STORED) as zf:
        zf.writestr("assets/minecraft/x.png", PNG)
    data = bytearray(buf.getvalue())
    idx = bytes(data).find(b"PK\x03\x04")
    name_len, extra_len = struct.unpack_from("<HH", data, idx + 26)
    data_start = idx + 30 + name_len + extra_len
    data[data_start] ^= 0xFF
    try:
        with zipfile.ZipFile(io.BytesIO(bytes(data))) as zf:
            report = inspect_zip(zf)
            # Hoặc BadZipFile (central dir hỏng), hoặc CRC error -> malformed
            assert any(f["code"] == "zip_malformed" for f in report["findings"])
    except zipfile.BadZipFile:
        pass     # acceptable — corrupt trước khi đọc được central dir


def test_inspect_entry_too_large_metadata(ctx):
    # Giả mạo central directory khai báo 65MB — inspect phải chặn mà không
    # cần data thật (mục 13.2 huge file — chặn trước khi giải nén)
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as zf:
        zf.writestr("assets/other.png", PNG)
        zf.writestr("assets/minecraft/huge.png", b"")
    data = bytearray(buf.getvalue())
    idx = bytes(data).find(b"PK\x01\x02")
    while idx != -1:
        name_len = struct.unpack_from("<H", data, idx + 28)[0]
        name = bytes(data[idx + 46:idx + 46 + name_len])
        if name == b"assets/minecraft/huge.png":
            struct.pack_into("<I", data, idx + 20, 65 * 1024 * 1024)
            struct.pack_into("<I", data, idx + 24, 65 * 1024 * 1024)
        idx = bytes(data).find(b"PK\x01\x02", idx + 4)
    try:
        with zipfile.ZipFile(io.BytesIO(bytes(data))) as zf:
            report = inspect_zip(zf)
        assert any(f["code"] == "zip_entry_too_large" for f in report["findings"])
    except NotImplementedError:
        pass     # python zipfile từ chối đọc được patched CD ở 1 số version — chấp nhận


# ----------------------------------------------------------------------
# import — happy path + conflicts (mục 93)
# ----------------------------------------------------------------------

def test_import_happy_path_and_build(ctx, proj_id):
    zp = _write_zip(ctx, {
        "assets/minecraft/textures/item/apple.png": PNG,
        "assets/minecraft/textures/item/banana.png": PNG,
    })
    rs = ResourceStudioService(ctx)
    out = rs.zip_import(proj_id, zp)
    assert len(out["imported"]) == 2 and not out["conflicts"]
    gen = ctx.paths.resource_studio / proj_id / "generated"
    assert (gen / "assets/minecraft/textures/item/apple.png").is_file()
    # project.assets ghi nhận
    project = rs.get(proj_id)
    assert {a["path"] for a in project["assets"]} >= {
        "assets/minecraft/textures/item/apple.png",
        "assets/minecraft/textures/item/banana.png"}
    # build pass sau import
    assert rs.validate(proj_id)["ok"] is True
    rs.build(proj_id)


def test_import_conflict_policies(ctx, proj_id):
    rs = ResourceStudioService(ctx)
    gen = ctx.paths.resource_studio / proj_id / "generated"
    target = gen / "assets/minecraft/textures/item/x.png"
    target.parent.mkdir(parents=True, exist_ok=True)
    target.write_bytes(PNG)     # file sẵn có (đủ khác nội dung import)

    zp = _write_zip(ctx, {"assets/minecraft/textures/item/x.png": PNG + b"new"})

    # policy keep -> giữ file cũ
    out = rs.zip_import(proj_id, zp, policy="keep")
    assert out["conflicts"][0]["resolution"] == "kept_existing"
    assert target.read_bytes() == PNG

    # policy keep_both -> file mới rename -imported
    out = rs.zip_import(proj_id, zp, policy="keep_both")
    assert out["conflicts"][0]["resolution"] == "kept_both"
    assert (gen / "assets/minecraft/textures/item/x-imported.png").is_file()
    assert target.read_bytes() == PNG           # file cũ giữ nguyên

    # policy replace -> ghi đè
    out = rs.zip_import(proj_id, zp, policy="replace")
    assert out["conflicts"][0]["resolution"] == "replaced"
    assert target.read_bytes() == PNG + b"new"

    # per-file override thắng global policy
    zp2 = _write_zip(ctx, {"assets/minecraft/textures/item/x.png": PNG + b"v3"})
    out = rs.zip_import(proj_id, zp2, policy="replace",
                        conflict_overrides={"assets/minecraft/textures/item/x.png": "keep"})
    assert out["conflicts"][0]["resolution"] == "kept_existing"
    assert target.read_bytes() == PNG + b"new"


def test_import_identical_content_not_conflict(ctx, proj_id):
    zp = _write_zip(ctx, {"assets/minecraft/textures/item/same.png": PNG})
    rs = ResourceStudioService(ctx)
    rs.zip_import(proj_id, zp)
    out = rs.zip_import(proj_id, zp)            # import lại
    assert not out["conflicts"]
    assert len(out["imported"]) == 1


# ----------------------------------------------------------------------
# import — rollback khi lỗi giữa chừng (mục 13.2 partial import)
# ----------------------------------------------------------------------

def test_import_invalid_zip_raises_and_cleans(ctx, proj_id):
    # ZIP chứa entry evil -> import bị chặn, không có file nào xuất hiện
    zp = _write_zip(ctx, {
        "assets/minecraft/good.png": PNG,
        "../evil.png": PNG,
    })
    rs = ResourceStudioService(ctx)
    with pytest.raises(AntaresError) as e:
        rs.zip_import(proj_id, zp)
    assert e.value.code == "ZIP_SECURITY_REJECTED"
    gen = ctx.paths.resource_studio / proj_id / "generated"
    assert not (gen / "assets/minecraft/good.png").exists()
    # staging temp được dọn
    leftovers = [p for p in (ctx.paths.data / "tmp").glob("zip-import-*")] \
        if (ctx.paths.data / "tmp").exists() else []
    assert not leftovers


def test_import_bad_policy_rejected(ctx, proj_id):
    zp = _write_zip(ctx, {"assets/minecraft/x.png": PNG})
    rs = ResourceStudioService(ctx)
    with pytest.raises(AntaresError, match="Policy"):
        rs.zip_import(proj_id, zp, policy="nuke")


def test_import_missing_zip(ctx, proj_id):
    rs = ResourceStudioService(ctx)
    with pytest.raises(AntaresError):
        rs.zip_import(proj_id, Path(ctx.paths.data) / "nonexistent.zip")


# ----------------------------------------------------------------------
# Limits (decision Batch 0: 256MB total / 64MB file / 4096 entries)
# ----------------------------------------------------------------------

def test_inspect_too_many_entries(ctx):
    buf = io.BytesIO()
    with zipfile.ZipFile(buf, "w") as zf:
        for i in range(5001):        # > MAX_ENTRIES 4096 — chỉ tên, không data
            zf.writestr(f"assets/minecraft/textures/item/f{i}.png", b"")
    with zipfile.ZipFile(io.BytesIO(buf.getvalue())) as zf:
        report = inspect_zip(zf)
    assert any(f["code"] == "zip_too_many_entries" for f in report["findings"])
