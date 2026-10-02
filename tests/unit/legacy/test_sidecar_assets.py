"""Tests cho B8a sidecar asset.*/resource.* handlers (§12 — Asset Library).

Handlers bọc AssetStore/ResourceStudioService thật — mock ở tầng service.
Preview data URI không đọc file thật (mock png_data_uri).
"""

import base64
import struct
import sys
import zlib
from pathlib import Path
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parent.parent.parent.parent
sys.path.insert(0, str(REPO_ROOT / "legacy" / "python"))

import sidecar  # noqa: E402


def _png_bytes(w: int = 8, h: int = 8) -> bytes:
    """PNG 8x8 tối thiểu (IHDR + IDAT rỗng nén zlib)."""
    sig = b"\x89PNG\r\n\x1a\n"

    def chunk(ctype: bytes, data: bytes) -> bytes:
        return struct.pack(">I", len(data)) + ctype + data + \
            struct.pack(">I", zlib.crc32(ctype + data) & 0xFFFFFFFF)

    ihdr = struct.pack(">IIBBBBB", w, h, 8, 6, 0, 0, 0)  # 8bit RGBA
    raw = b"".join(b"\x00" + b"\x00\x00\x00\xff" * w for _ in range(h))
    idat = zlib.compress(raw)
    return sig + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")


def _call(method: str, params: dict, *, assets=None, studio=None):
    ctx = mock.MagicMock()
    with mock.patch.object(sidecar, "_legacy_ctx", return_value=ctx), \
         mock.patch.object(sidecar, "_asset_store", return_value=assets or mock.MagicMock()), \
         mock.patch.object(sidecar, "_resource_studio", return_value=studio or mock.MagicMock()):
        response = sidecar.handle_request({"id": "t", "method": method, "params": params})
    return response


class TestRegistry:
    def test_all_b8a_methods_registered(self):
        expected = [
            "asset.list", "asset.import", "asset.get", "asset.delete", "asset.assign",
            "resource.list", "resource.get",
        ]
        for method in expected:
            assert method in sidecar.HANDLERS, f"missing handler: {method}"


class TestAssetList:
    def test_list_passes_filters(self):
        assets = mock.MagicMock()
        assets.list.return_value = [{"id": "a1", "name": "sword"}]
        response = _call("asset.list",
                         {"query": "sw", "category": "item", "tag": "weapon", "sort": "name"},
                         assets=assets)
        assert response["ok"] is True
        assert response["data"]["assets"][0]["name"] == "sword"
        assets.list.assert_called_once_with(query="sw", category="item", tag="weapon",
                                            sort="name")

    def test_list_defaults(self):
        assets = mock.MagicMock()
        assets.list.return_value = []
        response = _call("asset.list", {}, assets=assets)
        assert response["ok"] is True
        assets.list.assert_called_once_with(query="", category="", tag="", sort="newest")


class TestAssetImport:
    def test_import_ok(self):
        assets = mock.MagicMock()
        assets.import_png.return_value = {"id": "a1", "name": "sword.png", "width": 8,
                                          "height": 8, "sha256": "ab" * 32}
        data_b64 = base64.b64encode(_png_bytes()).decode()
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("asset.import",
                             {"dataB64": data_b64, "name": "sword.png",
                              "category": "item", "tags": ["weapon"]},
                             assets=assets)
        assert response["ok"] is True
        assert response["data"]["asset"]["id"] == "a1"
        raw = assets.import_png.call_args[0][0]
        assert raw.startswith(b"\x89PNG")
        assets.import_png.assert_called_once_with(raw, name="sword.png", category="item",
                                                  tags=["weapon"])
        n.assert_called_once()

    def test_import_requires_data(self):
        response = _call("asset.import", {})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_import_invalid_base64(self):
        response = _call("asset.import", {"dataB64": "!!!not-base64!!!"})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_import_service_validation_error_kept(self):
        from core.errors.base import AntaresError

        assets = mock.MagicMock()
        assets.import_png.side_effect = AntaresError("ASSET_TOO_LARGE", "9MB png")
        data_b64 = base64.b64encode(_png_bytes()).decode()
        response = _call("asset.import", {"dataB64": data_b64}, assets=assets)
        assert response["ok"] is False
        assert response["error"]["code"] == "ASSET_TOO_LARGE"


class TestAssetGetDelete:
    def test_get_with_preview(self):
        assets = mock.MagicMock()
        assets.get.return_value = {"id": "a1", "name": "sword.png"}
        assets.png_data_uri.return_value = "data:image/png;base64,AAAA"
        response = _call("asset.get", {"assetId": "a1"}, assets=assets)
        assert response["ok"] is True
        assert response["data"]["preview"].startswith("data:image/png")

    def test_get_not_found(self):
        assets = mock.MagicMock()
        assets.get.return_value = None
        response = _call("asset.get", {"assetId": "ghost"}, assets=assets)
        assert response["ok"] is False
        assert response["error"]["code"] == "ASSET_NOT_FOUND"

    def test_delete_not_found(self):
        assets = mock.MagicMock()
        assets.delete.return_value = False
        response = _call("asset.delete", {"assetId": "ghost"}, assets=assets)
        assert response["ok"] is False
        assert response["error"]["code"] == "ASSET_NOT_FOUND"

    def test_delete_ok(self):
        assets = mock.MagicMock()
        assets.delete.return_value = True
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("asset.delete", {"assetId": "a1"}, assets=assets)
        assert response["ok"] is True
        assert response["data"]["deleted"] is True
        n.assert_called_once()


class TestAssetAssign:
    def test_assign_ok(self):
        assets = mock.MagicMock()
        assets.assign_to_project.return_value = {"path": "assets/minecraft/textures/item/x.png",
                                                 "asset": "x.png", "sha256": "ab"}
        with mock.patch.object(sidecar, "notify"):
            response = _call("asset.assign",
                             {"projectId": "p1", "assetId": "a1",
                              "targetRel": "assets/minecraft/textures/item/x.png"},
                             assets=assets)
        assert response["ok"] is True
        assert response["data"]["assigned"]["asset"] == "x.png"

    def test_assign_invalid_path_from_service(self):
        from core.errors.base import AntaresError

        assets = mock.MagicMock()
        assets.assign_to_project.side_effect = AntaresError("VALIDATION_FAILED", "bad path")
        response = _call("asset.assign",
                         {"projectId": "p1", "assetId": "a1", "targetRel": "etc/passwd"},
                         assets=assets)
        assert response["ok"] is False
        assert response["error"]["code"] == "VALIDATION_FAILED"


class TestResourceProjects:
    def test_list(self):
        studio = mock.MagicMock()
        studio.list.return_value = [{"id": "p1", "name": "My Pack"}]
        response = _call("resource.list", {}, studio=studio)
        assert response["ok"] is True
        assert response["data"]["projects"][0]["name"] == "My Pack"

    def test_get(self):
        studio = mock.MagicMock()
        studio.get.return_value = {"id": "p1", "name": "My Pack"}
        response = _call("resource.get", {"projectId": "p1"}, studio=studio)
        assert response["ok"] is True
        assert response["data"]["project"]["id"] == "p1"

    def test_get_not_found(self):
        from core.errors.base import AntaresError

        studio = mock.MagicMock()
        studio.get.side_effect = AntaresError("RESOURCE_PROJECT_NOT_FOUND", "nope")
        response = _call("resource.get", {"projectId": "ghost"}, studio=studio)
        assert response["ok"] is False
        assert response["error"]["code"] == "RESOURCE_PROJECT_NOT_FOUND"
