"""Tests cho B7/M7 sidecar mods.* handlers (§11/§162–§164).

Handlers bọc ModService/ModSecurityService thật — mock `_mods_service` /
`_mod_security`, không đụng filesystem/network thật.
"""

import json
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
    services = {
        "instances": inst_svc,
        "http_client": mock.MagicMock(),
    }
    ctx.get.side_effect = lambda name: services[name]
    return ctx


def _call(method: str, params: dict, *, ctx=None, mods=None, security=None):
    """Call qua dispatch thật với services fake — kiểm tra envelope shape §43."""
    ctx = ctx if ctx is not None else mock.MagicMock()
    with mock.patch.object(sidecar, "_legacy_ctx", return_value=ctx), \
         mock.patch.object(sidecar, "_mods_service", return_value=mods or mock.MagicMock()), \
         mock.patch.object(sidecar, "_mod_security", return_value=security or mock.MagicMock()):
        response = sidecar.handle_request({"id": "t", "method": method, "params": params})
    return response


class TestRegistry:
    def test_all_mod_methods_registered(self):
        expected = [
            "mods.list", "mods.search", "mods.install", "mods.remove",
            "mods.health", "mods.scan", "mods.autofix",
            "mods.quarantine.list", "mods.quarantine.restore", "mods.quarantine.delete",
        ]
        for method in expected:
            assert method in sidecar.HANDLERS, f"missing handler: {method}"


class TestModsList:
    def test_list_enriched_with_metadata(self):
        ctx = _ctx_with_instance({"id": "i1", "loader": "fabric", "minecraftVersion": "1.21.11"})
        mods = mock.MagicMock()
        mods.list_installed.return_value = ["a.jar", "b.jar"]
        security = mock.MagicMock()
        security.health_check.return_value = {
            "mods": [
                {"filename": "a.jar", "modId": "a", "name": "ModA", "version": "1.0",
                 "loader": "fabric", "readable": True},
            ],
            "issues": [], "outdated": [],
        }
        response = _call("mods.list", {"instanceId": "i1"}, ctx=ctx, mods=mods, security=security)
        assert response["ok"] is True
        data = response["data"]
        assert [m["filename"] for m in data["mods"]] == ["a.jar", "b.jar"]
        a = data["mods"][0]
        assert a["name"] == "ModA" and a["readable"] is True
        # b.jar không có metadata → vẫn có trong list (plain entry)
        assert "name" not in data["mods"][1]

    def test_list_instance_not_found(self):
        ctx = _ctx_with_instance(None)
        response = _call("mods.list", {"instanceId": "ghost"}, ctx=ctx)
        assert response["ok"] is False
        assert response["error"]["code"] == "INSTANCE_NOT_FOUND"


class TestModsSearch:
    def test_search_ok(self):
        mods = mock.MagicMock()
        mods.search.return_value = [
            {"projectId": "p1", "title": "Sodium", "author": "jellysquid", "downloads": 1000},
        ]
        response = _call("mods.search",
                         {"query": "sodium", "loader": "fabric", "mcVersion": "1.21.11"},
                         mods=mods)
        assert response["ok"] is True
        assert response["data"]["hits"][0]["title"] == "Sodium"
        mods.search.assert_called_once_with("sodium", loader="fabric", mc_version="1.21.11")

    def test_search_requires_query(self):
        response = _call("mods.search", {"query": "  "})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"


class TestModsInstallRemove:
    def test_install_uses_instance_defaults(self):
        ctx = _ctx_with_instance({"id": "i1", "loader": "fabric", "minecraftVersion": "1.21.11"})
        mods = mock.MagicMock()
        mods.install.return_value = "sodium.jar"
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("mods.install",
                             {"projectId": "p1", "instanceId": "i1"}, ctx=ctx, mods=mods)
        assert response["ok"] is True
        assert response["data"]["filename"] == "sodium.jar"
        mods.install.assert_called_once_with("p1", "i1", loader="fabric", mc_version="1.21.11")
        n.assert_called_once_with("mods.changed",
                                  {"action": "install", "instanceId": "i1",
                                   "filename": "sodium.jar"})

    def test_install_params_override_defaults(self):
        ctx = _ctx_with_instance({"id": "i1", "loader": "fabric", "minecraftVersion": "1.21.11"})
        mods = mock.MagicMock()
        mods.install.return_value = "x.jar"
        _call("mods.install",
              {"projectId": "p1", "instanceId": "i1", "loader": "forge",
               "mcVersion": "1.20.1"}, ctx=ctx, mods=mods)
        mods.install.assert_called_once_with("p1", "i1", loader="forge", mc_version="1.20.1")

    def test_remove_path_traversal_rejected(self):
        for bad in ("../evil.jar", "sub/evil.jar", "evil\\jar", ".."):
            response = _call("mods.remove", {"instanceId": "i1", "filename": bad})
            assert response["ok"] is False, bad
            assert response["error"]["code"] == "CONFIG_INVALID", bad

    def test_remove_not_found(self):
        ctx = _ctx_with_instance({"id": "i1", "loader": "fabric", "minecraftVersion": "1.21"})
        mods = mock.MagicMock()
        mods.uninstall.return_value = False
        response = _call("mods.remove", {"instanceId": "i1", "filename": "ghost.jar"},
                         ctx=ctx, mods=mods)
        assert response["ok"] is False
        assert response["error"]["code"] == "MOD_NOT_FOUND"

    def test_remove_ok(self):
        ctx = _ctx_with_instance({"id": "i1", "loader": "fabric", "minecraftVersion": "1.21"})
        mods = mock.MagicMock()
        mods.uninstall.return_value = True
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("mods.remove", {"instanceId": "i1", "filename": "a.jar"},
                             ctx=ctx, mods=mods)
        assert response["ok"] is True
        assert response["data"]["removed"] is True
        n.assert_called_once()


class TestModsHealth:
    def test_health_ok(self):
        ctx = _ctx_with_instance({"id": "i1", "loader": "fabric", "minecraftVersion": "1.21.11"})
        security = mock.MagicMock()
        security.health_check.return_value = {
            "mods": [], "issues": [
                {"mod": "sodium.jar", "kind": "missing_dependency",
                 "detail": "requires fabric-api", "fixable": True,
                 "suggestion": "fabric-api"},
            ], "outdated": [],
        }
        response = _call("mods.health",
                         {"instanceId": "i1", "includeOutdated": False}, ctx=ctx,
                         security=security)
        assert response["ok"] is True
        health = response["data"]["health"]
        assert health["issues"][0]["kind"] == "missing_dependency"
        security.health_check.assert_called_once_with(
            "i1", loader="fabric", mc_version="1.21.11", include_outdated=False)


class TestModsScan:
    def test_scan_returns_report_and_counts(self):
        ctx = _ctx_with_instance({"id": "i1", "loader": "fabric", "minecraftVersion": "1.21"})
        security = mock.MagicMock()
        security.scan_instance.return_value = {
            "results": [{"file": "bad.jar", "verdict": "DANGEROUS",
                         "quarantined": True, "findings": []}],
            "dangerous": 1, "suspicious": 0, "safe": 2,
        }
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("mods.scan", {"instanceId": "i1"}, ctx=ctx, security=security)
        assert response["ok"] is True
        scan = response["data"]["scan"]
        assert scan["dangerous"] == 1
        assert scan["results"][0]["quarantined"] is True
        n.assert_called_once()

    def test_scan_instance_not_found(self):
        ctx = _ctx_with_instance(None)
        response = _call("mods.scan", {"instanceId": "ghost"}, ctx=ctx)
        assert response["ok"] is False
        assert response["error"]["code"] == "INSTANCE_NOT_FOUND"


class TestModsAutofix:
    def test_autofix_ok(self):
        ctx = _ctx_with_instance({"id": "i1", "loader": "fabric", "minecraftVersion": "1.21.11"})
        security = mock.MagicMock()
        security.auto_fix.return_value = {"downloaded": ["fabric-api.jar"], "failed": []}
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("mods.autofix", {"instanceId": "i1"}, ctx=ctx, security=security)
        assert response["ok"] is True
        assert response["data"]["fix"]["downloaded"] == ["fabric-api.jar"]
        n.assert_called_once()


class TestModDetail:
    """B7b — mod.detail đọc metadata jar (read-only)."""

    def _ctx_with_jar(self):
        import zipfile
        import tempfile
        tmp = tempfile.NamedTemporaryFile(suffix=".jar", delete=False)
        tmp.close()
        with zipfile.ZipFile(tmp.name, "w") as zf:
            zf.writestr("fabric.mod.json", json.dumps({
                "id": "sodium", "name": "Sodium", "version": "0.6.0",
                "depends": {"fabricloader": ">=0.16.0", "fabric-api": "*"},
                "breaks": {"optifine": "*"},
            }))
        return Path(tmp.name)

    def test_detail_metadata(self):
        ctx = _ctx_with_instance({"id": "i1", "loader": "fabric", "minecraftVersion": "1.21.11"})
        jar = self._ctx_with_jar()
        ctx.paths.instances = jar.parent  # jar.parent/filename sẽ khớp
        # Ghi jar vào đúng path mà handler đọc: instances/i1/game/mods/<filename>
        mods_dir = jar.parent / "i1" / "game" / "mods"
        mods_dir.mkdir(parents=True, exist_ok=True)
        target = mods_dir / jar.name
        jar.replace(target)
        response = _call("mod.detail", {"instanceId": "i1", "filename": jar.name}, ctx=ctx)
        assert response["ok"] is True
        detail = response["data"]["detail"]
        assert detail["modId"] == "sodium"
        assert detail["loader"] == "fabric"
        assert "fabric-api" in detail["depends"]
        assert "optifine" in detail["breaks"]
        assert detail["readable"] is True

    def test_detail_path_traversal_rejected(self):
        response = _call("mod.detail", {"instanceId": "i1", "filename": "../evil.jar"})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_detail_not_found(self):
        ctx = _ctx_with_instance({"id": "i1", "loader": "fabric", "minecraftVersion": "1.21"})
        ctx.paths.instances = Path("/tmp/fake-instances-b7b-detail")
        response = _call("mod.detail", {"instanceId": "i1", "filename": "ghost.jar"}, ctx=ctx)
        assert response["ok"] is False
        assert response["error"]["code"] == "MOD_NOT_FOUND"


class TestModpack:
    """B7b — modpack.* với Task thật của core (temp file)."""

    def _make_mrpack(self) -> Path:
        import json
        import tempfile
        import zipfile
        tmp = tempfile.NamedTemporaryFile(suffix=".mrpack", delete=False)
        tmp.close()
        with zipfile.ZipFile(tmp.name, "w") as zf:
            zf.writestr("modrinth.index.json", json.dumps({
                "name": "Test Pack", "versionId": "1.0.0",
                "dependencies": {"minecraft": "1.21.11", "fabric-loader": "0.16.9"},
                "files": [],
            }))
        return Path(tmp.name)

    def _tasked_ctx(self, instance: dict):
        from core.tasks.manager import TaskManager

        ctx = _ctx_with_instance(instance)
        ctx.tasks = TaskManager()
        return ctx

    def test_info_preview(self):
        ctx = _ctx_with_instance({"id": "i1"})
        mrpack = self._make_mrpack()
        response = _call("modpack.info", {"mrpackPath": str(mrpack)}, ctx=ctx)
        assert response["ok"] is True
        info = response["data"]["info"]
        assert info["name"] == "Test Pack"
        assert info["minecraftVersion"] == "1.21.11"
        assert info["loader"] == "fabric"

    def test_info_file_not_found(self):
        response = _call("modpack.info", {"mrpackPath": "/tmp/does-not-exist.mrpack"})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_install_returns_task_id(self):
        ctx = self._tasked_ctx({"id": "i1", "loader": "fabric", "minecraftVersion": "1.21.11"})
        mrpack = self._make_mrpack()
        with mock.patch.object(sidecar, "notify"):
            response = _call("modpack.install",
                             {"mrpackPath": str(mrpack), "instanceId": "i1"},
                             ctx=ctx, security=mock.MagicMock())
        assert response["ok"] is True
        task_id = response["data"]["taskId"]
        assert task_id
        # Task đã chạy — background thread có thể hoàn thành nhanh (files=[])
        task = ctx.tasks.get(task_id)
        assert task is not None

    def test_install_requires_existing_instance(self):
        ctx = _ctx_with_instance(None)
        mrpack = self._make_mrpack()
        response = _call("modpack.install",
                         {"mrpackPath": str(mrpack), "instanceId": "ghost"}, ctx=ctx)
        assert response["ok"] is False
        assert response["error"]["code"] == "INSTANCE_NOT_FOUND"

    def test_status_found_and_not_found(self):
        ctx = self._tasked_ctx({"id": "i1"})
        from core.tasks.manager import TaskManager

        tm = ctx.tasks
        assert isinstance(tm, TaskManager)
        task = tm.create("MODPACK_INSTALL", owner="instance:i1")
        response = _call("modpack.status", {"taskId": task.id}, ctx=ctx)
        assert response["ok"] is True
        assert response["data"]["task"]["id"] == task.id

        missing = _call("modpack.status", {"taskId": "nope"}, ctx=ctx)
        assert missing["ok"] is False
        assert missing["error"]["code"] == "TASK_NOT_FOUND"

    def test_cancel(self):
        ctx = self._tasked_ctx({"id": "i1"})
        tm = ctx.tasks
        task = tm.create("MODPACK_INSTALL", owner="instance:i1")
        response = _call("modpack.cancel", {"taskId": task.id}, ctx=ctx)
        assert response["ok"] is True
        assert response["data"]["cancelled"] is True

        missing = _call("modpack.cancel", {"taskId": "nope"}, ctx=ctx)
        assert missing["ok"] is False


class TestQuarantine:
    def test_list(self):
        security = mock.MagicMock()
        security.quarantine_list.return_value = [
            {"quarantineFile": "1_bad.jar", "originalName": "bad.jar",
             "verdict": "DANGEROUS", "score": 95},
        ]
        response = _call("mods.quarantine.list", {}, security=security)
        assert response["ok"] is True
        assert response["data"]["items"][0]["verdict"] == "DANGEROUS"

    def test_restore(self):
        security = mock.MagicMock()
        security.quarantine_restore.return_value = {"originalName": "bad.jar",
                                                    "instanceId": "i1"}
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("mods.quarantine.restore", {"quarantineFile": "1_bad.jar"},
                             security=security)
        assert response["ok"] is True
        assert response["data"]["restored"] == "bad.jar"
        n.assert_called_once()

    def test_delete_requires_confirm(self):
        response = _call("mods.quarantine.delete", {"quarantineFile": "1_bad.jar"})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_delete_not_found(self):
        security = mock.MagicMock()
        security.quarantine_delete.return_value = False
        response = _call("mods.quarantine.delete",
                         {"quarantineFile": "ghost.jar", "confirm": True}, security=security)
        assert response["ok"] is False
        assert response["error"]["code"] == "MOD_NOT_FOUND"

    def test_delete_ok(self):
        security = mock.MagicMock()
        security.quarantine_delete.return_value = True
        response = _call("mods.quarantine.delete",
                         {"quarantineFile": "1_bad.jar", "confirm": True}, security=security)
        assert response["ok"] is True
        assert response["data"]["deleted"] is True
