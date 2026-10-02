"""Tests cho M6 sidecar flows (instances/accounts/java/play/dashboard)."""

import json
import sys
from pathlib import Path
from unittest import mock

import pytest

REPO_ROOT = Path(__file__).resolve().parent.parent.parent.parent
sys.path.insert(0, str(REPO_ROOT / "legacy" / "python"))

import sidecar  # noqa: E402


class TestRegistry:
    def test_all_m6_methods_registered(self):
        expected = [
            "instances.list", "instances.get", "instances.create", "instances.select",
            "accounts.list", "accounts.select",
            "java.list", "versions.list",
            "play.preflight", "play.launch",
            "dashboard.summary",
        ]
        for method in expected:
            assert method in sidecar.HANDLERS, f"missing handler: {method}"

    def test_unknown_method_envelope(self):
        response = sidecar.handle_request({"id": "x", "method": "instances.unknown", "params": {}})
        assert response["ok"] is False
        assert response["error"]["code"] == "METHOD_NOT_FOUND"


class TestRequiredJavaMajor:
    def test_mapping(self):
        assert sidecar._required_java_major("1.8.9") == 8
        assert sidecar._required_java_major("1.17") == 16
        assert sidecar._required_java_major("1.18.2") == 17
        assert sidecar._required_java_major("1.20.1") == 21
        assert sidecar._required_java_major("1.21.11") == 21
        assert sidecar._required_java_major("24w14a") == 21  # snapshot → newest


class TestLegacyUnavailable:
    def test_handler_without_ctx_returns_error_envelope(self):
        with mock.patch.object(sidecar, "_legacy_ctx", return_value=None):
            response = sidecar.handle_request({"id": "y", "method": "instances.list", "params": {}})
        assert response["ok"] is False
        assert response["error"]["code"] == "LEGACY_UNAVAILABLE"


class TestPreflight:
    """Preflight logic với fake ctx — không đụng filesystem thật."""

    def _make_ctx(self, instance: dict, javas: list[dict], account: dict | None):
        ctx = mock.MagicMock()
        instances = mock.MagicMock()
        instances.get.return_value = instance
        ctx.get.side_effect = lambda name: {
            "instances": instances,
            "accounts": mock.MagicMock(get_current=mock.Mock(return_value=account)),
            "java_homes": None,
        }[name]
        ctx.paths.instances = Path("/tmp/fake-instances")
        return ctx

    def _instance(self, version: str = "1.21.11") -> dict:
        return {"id": "abc123", "name": "Test", "minecraftVersion": version, "loader": "fabric"}

    def test_all_pass_with_java_and_account(self):
        javas = [{"path": "/j", "exe": "/j/bin/java", "major": 21, "name": "jdk21"}]
        ctx = self._make_ctx(self._instance(), javas, account={"id": "a1", "displayName": "Stever"})
        with mock.patch.object(sidecar, "_legacy_ctx", return_value=ctx), \
             mock.patch("services.java.discovery.scan_system_java", return_value=[Path("/j")]), \
             mock.patch("services.java.discovery.java_info", side_effect=lambda h: javas[0]):
            response = sidecar.handle_request({"id": "p1", "method": "play.preflight",
                                               "params": {"instanceId": "abc123"}})
        assert response["ok"] is True
        data = response["data"]
        assert data["canPlay"] is True
        assert data["blockers"] == 0
        ids = {c["id"] for c in data["checks"]}
        assert {"java", "version", "account", "disk", "mods"} <= ids

    def test_missing_java_is_blocker(self):
        ctx = self._make_ctx(self._instance(), [], account=None)
        # scan_system_java trả [] → không có Java
        with mock.patch.object(sidecar, "_legacy_ctx", return_value=ctx), \
             mock.patch("services.java.discovery.scan_system_java", return_value=[]):
            response = sidecar.handle_request({"id": "p2", "method": "play.preflight",
                                               "params": {"instanceId": "abc123"}})
        data = response["data"]
        java_check = next(c for c in data["checks"] if c["id"] == "java")
        assert java_check["status"] == "error"
        assert data["canPlay"] is False
        assert data["blockers"] >= 1

    def test_no_account_is_warning_not_blocker(self):
        javas = [{"path": "/j", "exe": "/j/bin/java", "major": 21, "name": "jdk21"}]
        ctx = self._make_ctx(self._instance(), javas, account=None)
        with mock.patch.object(sidecar, "_legacy_ctx", return_value=ctx):
            response = sidecar.handle_request({"id": "p3", "method": "play.preflight",
                                               "params": {"instanceId": "abc123"}})
        data = response["data"]
        account_check = next(c for c in data["checks"] if c["id"] == "account")
        assert account_check["status"] == "warning"
        assert data["canPlay"] is True  # warning không chặn (§8: Play Anyway)

    def test_instance_not_found(self):
        ctx = self._make_ctx(None, [], None)
        with mock.patch.object(sidecar, "_legacy_ctx", return_value=ctx):
            response = sidecar.handle_request({"id": "p4", "method": "play.preflight",
                                               "params": {"instanceId": "ghost"}})
        assert response["ok"] is False
        assert response["error"]["code"] == "INSTANCE_NOT_FOUND"


class TestDashboard:
    def test_summary_with_fake_ctx(self):
        instances = [
            {"id": "i1", "name": "A", "lastPlayedAt": 100},
            {"id": "i2", "name": "B", "lastPlayedAt": 300},
        ]
        ctx = mock.MagicMock()
        inst_svc = mock.MagicMock()
        inst_svc.list.return_value = instances
        ctx.get.side_effect = lambda name: {
            "instances": inst_svc,
            "accounts": mock.MagicMock(get_current=mock.Mock(return_value={"id": "a", "displayName": "P"})),
        }[name]
        ctx.config.get.return_value = "i1"
        with mock.patch.object(sidecar, "_legacy_ctx", return_value=ctx):
            response = sidecar.handle_request({"id": "d1", "method": "dashboard.summary", "params": {}})
        data = response["data"]
        assert data["instanceCount"] == 2
        assert data["recentInstanceId"] == "i2"
        assert data["selectedInstanceId"] == "i1"
        assert data["account"]["displayName"] == "P"


class TestInstancesList:
    def test_list_with_fake_ctx(self):
        ctx = mock.MagicMock()
        inst_svc = mock.MagicMock()
        inst_svc.list.return_value = [{"id": "i1", "name": "A"}]
        ctx.get.side_effect = lambda name: {"instances": inst_svc}[name]
        with mock.patch.object(sidecar, "_legacy_ctx", return_value=ctx):
            response = sidecar.handle_request({"id": "l1", "method": "instances.list", "params": {}})
        assert response["ok"] is True
        assert response["data"]["instances"][0]["id"] == "i1"
