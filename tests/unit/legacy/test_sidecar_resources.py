"""Tests cho B8b sidecar resource.* handlers (§33/§126–§128/§173 — pack lifecycle).

Handlers bọc ResourceStudioService thật — mock `_resource_studio`, không đụng
filesystem thật. build_task dùng TaskManager thật (core.tasks.manager).
"""

import sys
from pathlib import Path
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parent.parent.parent.parent
sys.path.insert(0, str(REPO_ROOT / "legacy" / "python"))

import sidecar  # noqa: E402


def _ctx_with_instance(instance: dict | None) -> mock.MagicMock:
    ctx = mock.MagicMock()
    inst_svc = mock.MagicMock()
    inst_svc.get.return_value = instance
    services = {"instances": inst_svc}
    ctx.get.side_effect = lambda name: services[name]
    return ctx


def _call(method: str, params: dict, *, ctx=None, studio=None):
    ctx = ctx if ctx is not None else mock.MagicMock()
    with mock.patch.object(sidecar, "_legacy_ctx", return_value=ctx), \
         mock.patch.object(sidecar, "_resource_studio",
                           return_value=studio or mock.MagicMock()):
        response = sidecar.handle_request({"id": "t", "method": method, "params": params})
    return response


class TestRegistry:
    def test_all_b8b_methods_registered(self):
        expected = [
            "resource.wizard_info", "resource.create", "resource.update",
            "resource.delete", "resource.generate", "resource.validate",
            "resource.build", "resource.build_task", "resource.build_cancel",
            "resource.builds", "resource.install", "resource.installed",
            "resource.uninstall", "resource.layer.get", "resource.layer.set",
            "resource.layer.move",
        ]
        for method in expected:
            assert method in sidecar.HANDLERS, f"missing handler: {method}"


class TestWizardInfo:
    def test_wizard_info(self):
        studio = mock.MagicMock()
        studio.wizard_info.return_value = {
            "templates": [{"id": "minimal", "labelKey": "minimal",
                           "descKey": "minimalDesc",
                           "modules": {"crosshair": True, "items": False, "hud": False}}],
            "versions": ["1.21.11", "1.21.4"],
            "defaultVersion": "1.21.11",
        }
        response = _call("resource.wizard_info", {}, studio=studio)
        assert response["ok"] is True
        info = response["data"]
        assert info["defaultVersion"] == "1.21.11"
        assert info["templates"][0]["id"] == "minimal"


class TestProjectCrud:
    def test_create_ok(self):
        studio = mock.MagicMock()
        studio.create.return_value = {"id": "p1", "name": "My Pack",
                                      "minecraft": {"version": "1.21.4"}}
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("resource.create",
                             {"name": "My Pack", "mcVersion": "1.21.4",
                              "template": "pvp", "description": "desc"},
                             studio=studio)
        assert response["ok"] is True
        assert response["data"]["project"]["id"] == "p1"
        studio.create.assert_called_once_with("My Pack", "1.21.4",
                                              template="pvp", description="desc")
        n.assert_called_once_with("resources.changed",
                                  {"action": "create", "projectId": "p1"})

    def test_create_requires_name_and_version(self):
        response = _call("resource.create", {"name": "  ", "mcVersion": ""})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_create_service_validation_kept(self):
        from core.errors.base import AntaresError

        studio = mock.MagicMock()
        studio.create.side_effect = AntaresError("VALIDATION_FAILED", "bad template")
        response = _call("resource.create",
                         {"name": "X", "mcVersion": "1.21"}, studio=studio)
        assert response["ok"] is False
        assert response["error"]["code"] == "VALIDATION_FAILED"

    def test_update_ok(self):
        studio = mock.MagicMock()
        studio.update.return_value = {"id": "p1", "name": "Renamed"}
        with mock.patch.object(sidecar, "notify"):
            response = _call("resource.update",
                             {"projectId": "p1", "patch": {"name": "Renamed"}},
                             studio=studio)
        assert response["ok"] is True
        assert response["data"]["project"]["name"] == "Renamed"

    def test_update_requires_patch_object(self):
        response = _call("resource.update", {"projectId": "p1", "patch": "nope"})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_delete_requires_confirm(self):
        response = _call("resource.delete", {"projectId": "p1"})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_delete_ok(self):
        studio = mock.MagicMock()
        studio.delete.return_value = True
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("resource.delete", {"projectId": "p1", "confirm": True},
                             studio=studio)
        assert response["ok"] is True
        assert response["data"]["deleted"] is True
        studio.delete.assert_called_once_with("p1")
        n.assert_called_once()

    def test_delete_missing_project_code_kept(self):
        from core.errors.base import AntaresError

        studio = mock.MagicMock()
        studio.delete.side_effect = AntaresError("FILE_NOT_FOUND", "Project not found")
        response = _call("resource.delete", {"projectId": "ghost", "confirm": True},
                         studio=studio)
        assert response["ok"] is False
        assert response["error"]["code"] == "FILE_NOT_FOUND"


class TestGenerateValidateBuild:
    def test_generate_ok(self):
        studio = mock.MagicMock()
        studio.generate.return_value = {"files": 5, "mcVersion": "1.21.4",
                                        "packFormat": 46, "warnings": []}
        with mock.patch.object(sidecar, "notify"):
            response = _call("resource.generate", {"projectId": "p1"}, studio=studio)
        assert response["ok"] is True
        assert response["data"]["generated"]["packFormat"] == 46

    def test_validate_returns_findings(self):
        studio = mock.MagicMock()
        studio.validate.return_value = {
            "ok": False,
            "findings": [{"code": "invalid_json", "severity": "ERROR",
                          "path": "pack.mcmeta", "detail": "expecting value"}],
            "packFormat": 46,
        }
        response = _call("resource.validate", {"projectId": "p1"}, studio=studio)
        assert response["ok"] is True
        result = response["data"]
        assert result["ok"] is False
        assert result["findings"][0]["severity"] == "ERROR"
        assert result["packFormat"] == 46

    def test_build_returns_manifest(self):
        studio = mock.MagicMock()
        studio.build.return_value = {
            "project": "p1", "name": "My Pack", "mcVersion": "1.21.4",
            "packFormat": 46, "file": "my-pack-1760000.zip",
            "sha256": "ab" * 32, "bytes": 1024, "files": 7,
            "warnings": [], "builtAt": 1760000,
        }
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("resource.build", {"projectId": "p1"}, studio=studio)
        assert response["ok"] is True
        manifest = response["data"]["manifest"]
        assert manifest["file"].endswith(".zip")
        assert manifest["packFormat"] == 46
        n.assert_called_once()

    def test_build_validation_error_kept(self):
        from core.errors.base import AntaresError

        studio = mock.MagicMock()
        studio.build.side_effect = AntaresError(
            "VALIDATION_FAILED", "Pack validation failed: wrong_path at x")
        response = _call("resource.build", {"projectId": "p1"}, studio=studio)
        assert response["ok"] is False
        assert response["error"]["code"] == "VALIDATION_FAILED"

    def test_builds_list(self):
        studio = mock.MagicMock()
        studio.list_builds.return_value = [
            {"file": "my-pack-2.zip", "builtAt": 2},
            {"file": "my-pack-1.zip", "builtAt": 1},
        ]
        response = _call("resource.builds", {"projectId": "p1"}, studio=studio)
        assert response["ok"] is True
        assert len(response["data"]["builds"]) == 2


class TestBuildTask:
    """resource.build_task — build dưới TaskManager thật (§17)."""

    def _tasked_ctx(self):
        from core.tasks.manager import TaskManager

        ctx = mock.MagicMock()
        ctx.tasks = TaskManager()
        return ctx

    def test_build_task_returns_manifest(self):
        ctx = self._tasked_ctx()
        studio = mock.MagicMock()
        studio.build_task.return_value = {
            "manifest": {"file": "my-pack-3.zip", "sha256": "cd" * 32,
                         "bytes": 2048, "files": 9},
        }
        response = _call("resource.build_task", {"projectId": "p1"},
                         ctx=ctx, studio=studio)
        assert response["ok"] is True
        assert response["data"]["manifest"]["file"] == "my-pack-3.zip"
        studio.build_task.assert_called_once_with("p1")

    def test_build_task_service_error_kept(self):
        from core.errors.base import AntaresError

        ctx = self._tasked_ctx()
        studio = mock.MagicMock()
        studio.build_task.side_effect = AntaresError("VALIDATION_FAILED", "broken pack")
        response = _call("resource.build_task", {"projectId": "p1"},
                         ctx=ctx, studio=studio)
        assert response["ok"] is False
        assert response["error"]["code"] == "VALIDATION_FAILED"

    def test_build_cancel_ok(self):
        ctx = self._tasked_ctx()
        from core.tasks.manager import TaskManager

        task = ctx.tasks.create("resource_build", owner="resource_studio")
        response = _call("resource.build_cancel", {"taskId": task.id}, ctx=ctx)
        assert response["ok"] is True
        assert response["data"]["cancelled"] is True

    def test_build_cancel_not_found(self):
        ctx = self._tasked_ctx()
        response = _call("resource.build_cancel", {"taskId": "nope"}, ctx=ctx)
        assert response["ok"] is False
        assert response["error"]["code"] == "TASK_NOT_FOUND"


class TestInstallUninstall:
    def test_install_requires_instance(self):
        ctx = _ctx_with_instance(None)
        response = _call("resource.install",
                         {"projectId": "p1", "instanceId": "ghost"}, ctx=ctx)
        assert response["ok"] is False
        assert response["error"]["code"] == "INSTANCE_NOT_FOUND"

    def test_install_ok(self):
        ctx = _ctx_with_instance({"id": "i1", "loader": "fabric"})
        studio = mock.MagicMock()
        studio.install.return_value = {
            "file": "my-pack.zip", "bytes": 1024, "sha256": "ab",
            "backup": None, "installedAt": 1760000,
        }
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("resource.install",
                             {"projectId": "p1", "instanceId": "i1",
                              "overwrite": True},
                             ctx=ctx, studio=studio)
        assert response["ok"] is True
        assert response["data"]["installed"]["file"] == "my-pack.zip"
        studio.install.assert_called_once_with("p1", "i1", overwrite=True)
        n.assert_called_once()

    def test_install_service_error_kept(self):
        from core.errors.base import AntaresError

        ctx = _ctx_with_instance({"id": "i1"})
        studio = mock.MagicMock()
        studio.install.side_effect = AntaresError(
            "VALIDATION_FAILED", "'x.zip' already exists (overwrite=True to replace)")
        response = _call("resource.install",
                         {"projectId": "p1", "instanceId": "i1"}, ctx=ctx,
                         studio=studio)
        assert response["ok"] is False
        assert response["error"]["code"] == "VALIDATION_FAILED"

    def test_installed_list(self):
        studio = mock.MagicMock()
        studio.installed.return_value = [{"file": "my-pack.zip", "bytes": 1024}]
        response = _call("resource.installed", {"instanceId": "i1"}, studio=studio)
        assert response["ok"] is True
        assert response["data"]["packs"][0]["file"] == "my-pack.zip"

    def test_uninstall_path_traversal_rejected(self):
        for bad in ("../evil.zip", "sub/evil.zip", "evil\\zip", ".."):
            response = _call("resource.uninstall",
                             {"instanceId": "i1", "filename": bad})
            assert response["ok"] is False, bad
            assert response["error"]["code"] == "CONFIG_INVALID", bad

    def test_uninstall_not_found(self):
        ctx = _ctx_with_instance({"id": "i1"})
        studio = mock.MagicMock()
        studio.uninstall.return_value = False
        response = _call("resource.uninstall",
                         {"instanceId": "i1", "filename": "ghost.zip"},
                         ctx=ctx, studio=studio)
        assert response["ok"] is False
        assert response["error"]["code"] == "FILE_NOT_FOUND"

    def test_uninstall_ok(self):
        ctx = _ctx_with_instance({"id": "i1"})
        studio = mock.MagicMock()
        studio.uninstall.return_value = True
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("resource.uninstall",
                             {"instanceId": "i1", "filename": "my-pack.zip"},
                             ctx=ctx, studio=studio)
        assert response["ok"] is True
        assert response["data"]["removed"] is True
        studio.uninstall.assert_called_once_with("i1", "my-pack.zip")
        n.assert_called_once()


class TestLayers:
    """resource.layer.* — §173 deterministic order + preview."""

    def test_layer_get(self):
        studio = mock.MagicMock()
        studio.layers.get_order.return_value = ["a.zip", "b.zip"]
        studio.layers._preview_for.return_value = {
            "order": ["a.zip", "b.zip"],
            "assets": {"assets/minecraft/textures/item/x.png":
                       {"pack": "b.zip", "priority": 1}},
            "conflicts": [],
        }
        response = _call("resource.layer.get", {"instanceId": "i1"}, studio=studio)
        assert response["ok"] is True
        assert response["data"]["order"] == ["a.zip", "b.zip"]
        assert response["data"]["preview"]["conflicts"] == []
        studio.layers._preview_for.assert_called_once_with("i1", ["a.zip", "b.zip"])

    def test_layer_set_ok(self):
        studio = mock.MagicMock()
        studio.layers.set_order.return_value = ["b.zip", "a.zip"]
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("resource.layer.set",
                             {"instanceId": "i1", "order": ["b.zip", "a.zip"]},
                             studio=studio)
        assert response["ok"] is True
        assert response["data"]["order"] == ["b.zip", "a.zip"]
        studio.layers.set_order.assert_called_once_with("i1", ["b.zip", "a.zip"])
        n.assert_called_once()

    def test_layer_set_requires_list(self):
        response = _call("resource.layer.set", {"instanceId": "i1", "order": "b.zip"})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_layer_set_service_validation_kept(self):
        from core.errors.base import AntaresError

        studio = mock.MagicMock()
        studio.layers.set_order.side_effect = AntaresError(
            "FILE_NOT_FOUND", "Pack chưa install: b.zip")
        response = _call("resource.layer.set",
                         {"instanceId": "i1", "order": ["b.zip"]}, studio=studio)
        assert response["ok"] is False
        assert response["error"]["code"] == "FILE_NOT_FOUND"

    def test_layer_move(self):
        studio = mock.MagicMock()
        studio.layers.move.return_value = ["b.zip", "a.zip"]
        with mock.patch.object(sidecar, "notify"):
            response = _call("resource.layer.move",
                             {"instanceId": "i1", "filename": "b.zip", "delta": -1},
                             studio=studio)
        assert response["ok"] is True
        assert response["data"]["order"] == ["b.zip", "a.zip"]
        studio.layers.move.assert_called_once_with("i1", "b.zip", -1)

    def test_layer_move_instance_error_kept(self):
        from core.errors.base import AntaresError

        studio = mock.MagicMock()
        studio.layers.move.side_effect = AntaresError("INSTANCE_NOT_FOUND",
                                                      "Instance not found")
        response = _call("resource.layer.move",
                         {"instanceId": "ghost", "filename": "a.zip", "delta": 1},
                         studio=studio)
        assert response["ok"] is False
        assert response["error"]["code"] == "INSTANCE_NOT_FOUND"
