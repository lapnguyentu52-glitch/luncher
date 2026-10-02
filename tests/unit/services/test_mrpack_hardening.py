"""§4.3 — hardening install_mrpack: host allowlist + sha1 bắt buộc.

Trước khi vá, `install_mrpack` lấy `file["downloads"][0]` thẳng (không check
scheme/host — URL do mrpack chỉ định, nguồn không tin cậy) và sha1 là tuỳ
chọn → tải về gì cũng nhận.
"""
from __future__ import annotations

import io
import json
import zipfile
from pathlib import Path
from types import SimpleNamespace

import pytest

from core.errors.base import AntaresError
from services.mods.mrpack import _validate_download_url, install_mrpack


def _task() -> SimpleNamespace:
    return SimpleNamespace(cancelled=False, message="", progress=0.0)


def _write_mrpack(path: Path, files: list[dict]) -> Path:
    index = {"name": "T", "versionId": "v1",
             "dependencies": {"minecraft": "1.21.1"},
             "files": files}
    with zipfile.ZipFile(path, "w", zipfile.ZIP_DEFLATED) as zf:
        zf.writestr("modrinth.index.json", json.dumps(index))
    return path


class TestValidateUrl:
    def test_allows_modrinth_https(self):
        _validate_download_url("https://cdn.modrinth.com/data/x.jar")

    def test_rejects_http_scheme(self):
        with pytest.raises(AntaresError) as e:
            _validate_download_url("http://cdn.modrinth.com/x.jar")
        assert e.value.code == "VALIDATION_FAILED"

    def test_rejects_unknown_host(self):
        with pytest.raises(AntaresError):
            _validate_download_url("https://evil.example.com/x.jar")

    def test_rejects_non_http_schemes(self):
        for url in ("file:///etc/passwd", "ftp://cdn.modrinth.com/x",
                    "javascript:alert(1)"):
            with pytest.raises(AntaresError):
                _validate_download_url(url)

    def test_rejects_userinfo_trick(self):
        # "cdn.modrinth.com@evil.com" — hostname thật là evil.com
        with pytest.raises(AntaresError):
            _validate_download_url("https://cdn.modrinth.com@evil.com/x.jar")


class TestInstallMrpack:
    def test_rejects_disallowed_host(self, ctx, tmp_path):
        pack = _write_mrpack(tmp_path / "p.mrpack", [{
            "path": "mods/a.jar",
            "downloads": ["https://evil.example.com/a.jar"],
            "hashes": {"sha1": "a" * 40},
        }])
        with pytest.raises(AntaresError) as e:
            install_mrpack(ctx, pack, "no-such-instance", _task(),
                           skip_dependencies=True)
        assert e.value.code == "VALIDATION_FAILED"
        assert "not allowed" in str(e.value)

    def test_rejects_missing_sha1(self, ctx, tmp_path):
        pack = _write_mrpack(tmp_path / "p.mrpack", [{
            "path": "mods/a.jar",
            "downloads": ["https://cdn.modrinth.com/a.jar"],
            "hashes": {},
        }])
        with pytest.raises(AntaresError) as e:
            install_mrpack(ctx, pack, "no-such-instance", _task(),
                           skip_dependencies=True)
        assert "sha1" in str(e.value)

    def test_rejects_missing_downloads(self, ctx, tmp_path):
        pack = _write_mrpack(tmp_path / "p.mrpack", [{
            "path": "mods/a.jar",
            "downloads": [],
            "hashes": {"sha1": "b" * 40},
        }])
        with pytest.raises(AntaresError) as e:
            install_mrpack(ctx, pack, "no-such-instance", _task(),
                           skip_dependencies=True)
        assert "download" in str(e.value).lower()

    def test_rejects_insecure_scheme(self, ctx, tmp_path):
        pack = _write_mrpack(tmp_path / "p.mrpack", [{
            "path": "mods/a.jar",
            "downloads": ["http://cdn.modrinth.com/a.jar"],
            "hashes": {"sha1": "c" * 40},
        }])
        with pytest.raises(AntaresError):
            install_mrpack(ctx, pack, "no-such-instance", _task(),
                           skip_dependencies=True)
