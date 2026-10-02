"""Tests cho B6/M7 sidecar profiles.* handlers (§24/§106/§107).

Handlers bọc ProfileService thật — mock `_profiles_service` (trả service fake),
không đụng state file thật của legacy.
"""

import sys
from pathlib import Path
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parent.parent.parent.parent
sys.path.insert(0, str(REPO_ROOT / "legacy" / "python"))

import sidecar  # noqa: E402


def _service() -> mock.MagicMock:
    """Fake ProfileService — mock chuyên biệt, không MagicMock tự sinh attribute lạ."""
    svc = mock.MagicMock()
    return svc


def _call(method: str, params: dict, svc: mock.MagicMock | None = None):
    """Call qua dispatch thật với service fake — kiểm tra cả envelope shape §43."""
    svc = svc or _service()
    with mock.patch.object(sidecar, "_legacy_ctx", return_value=mock.MagicMock()), \
         mock.patch.object(sidecar, "_profiles_service", return_value=svc):
        response = sidecar.handle_request({"id": "t", "method": method, "params": params})
    return response, svc


class TestRegistry:
    def test_all_profile_methods_registered(self):
        expected = [
            "profiles.list", "profiles.get", "profiles.create", "profiles.duplicate",
            "profiles.update", "profiles.delete", "profiles.capture", "profiles.plan",
            "profiles.apply", "profiles.revert", "profiles.export", "profiles.import",
            "profiles.validate",
        ]
        for method in expected:
            assert method in sidecar.HANDLERS, f"missing handler: {method}"

    def test_list_handler_wired_to_real_name(self):
        assert sidecar.HANDLERS["profiles.list"].__name__ == "handle_profiles_list"


class TestCrud:
    def test_list(self):
        svc = _service()
        svc.list.return_value = [{"id": "p1", "name": "A", "active": False}]
        response, _ = _call("profiles.list", {}, svc)
        assert response["ok"] is True
        assert response["data"]["profiles"][0]["id"] == "p1"

    def test_get_found(self):
        svc = _service()
        svc.get.return_value = {"id": "p1", "name": "A"}
        response, _ = _call("profiles.get", {"profileId": "p1"}, svc)
        assert response["ok"] is True
        assert response["data"]["profile"]["name"] == "A"

    def test_get_not_found(self):
        svc = _service()
        svc.get.return_value = None
        response, _ = _call("profiles.get", {"profileId": "ghost"}, svc)
        assert response["ok"] is False
        assert response["error"]["code"] == "PROFILE_NOT_FOUND"

    def test_create_requires_name(self):
        response, _ = _call("profiles.create", {"name": "  "})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_create_rejects_non_dict_spec(self):
        response, _ = _call("profiles.create", {"name": "A", "spec": ["bad"]})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_create_ok(self):
        svc = _service()
        svc.create.return_value = {"id": "p9", "name": "New"}
        with mock.patch.object(sidecar, "notify") as n:
            response, _ = _call("profiles.create",
                                {"name": "New", "spec": {"jvm": {"memory": "4G"}}}, svc)
        assert response["ok"] is True
        assert response["data"]["profile"]["id"] == "p9"
        svc.create.assert_called_once_with("New", {"jvm": {"memory": "4G"}})
        n.assert_called_once_with("profiles.changed",
                                  {"action": "create", "profileId": "p9"})

    def test_duplicate(self):
        svc = _service()
        svc.duplicate.return_value = {"id": "p2", "name": "A copy"}
        with mock.patch.object(sidecar, "notify") as n:
            response, _ = _call("profiles.duplicate", {"profileId": "p1"}, svc)
        assert response["ok"] is True
        assert response["data"]["profile"]["name"] == "A copy"
        n.assert_called_once()

    def test_update_requires_patch(self):
        response, _ = _call("profiles.update", {"profileId": "p1"})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_update_ok(self):
        svc = _service()
        svc.update.return_value = {"id": "p1", "name": "Renamed"}
        with mock.patch.object(sidecar, "notify") as n:
            response, _ = _call("profiles.update",
                                {"profileId": "p1", "patch": {"name": "Renamed"}}, svc)
        assert response["ok"] is True
        assert response["data"]["profile"]["name"] == "Renamed"
        n.assert_called_once_with("profiles.changed",
                                  {"action": "update", "profileId": "p1"})

    def test_delete_requires_confirm(self):
        response, _ = _call("profiles.delete", {"profileId": "p1"})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_delete_with_confirm_not_found(self):
        svc = _service()
        svc.delete.return_value = False
        response, _ = _call("profiles.delete", {"profileId": "ghost", "confirm": True}, svc)
        assert response["ok"] is False
        assert response["error"]["code"] == "PROFILE_NOT_FOUND"

    def test_delete_ok(self):
        svc = _service()
        svc.delete.return_value = True
        with mock.patch.object(sidecar, "notify") as n:
            response, _ = _call("profiles.delete", {"profileId": "p1", "confirm": True}, svc)
        assert response["ok"] is True
        assert response["data"]["deleted"] is True
        n.assert_called_once_with("profiles.changed",
                                  {"action": "delete", "profileId": "p1"})


class TestCapture:
    def test_capture_ok(self):
        svc = _service()
        svc.capture.return_value = {"id": "pc1", "name": "inst snapshot"}
        with mock.patch.object(sidecar, "notify") as n:
            response, _ = _call("profiles.capture",
                                {"instanceId": "i1", "name": "My snap"}, svc)
        assert response["ok"] is True
        svc.capture.assert_called_once_with("i1", "My snap")
        n.assert_called_once()

    def test_capture_without_name_passes_none(self):
        svc = _service()
        svc.capture.return_value = {"id": "pc1", "name": "inst snapshot"}
        response, _ = _call("profiles.capture", {"instanceId": "i1"}, svc)
        assert response["ok"] is True
        svc.capture.assert_called_once_with("i1", None)


class TestPlanApplyRevert:
    """Plan/Apply/Revert — mutation thật nằm ở ProfileService; sidecar chỉ wrap."""

    def test_plan(self):
        svc = _service()
        svc.plan.return_value = {
            "profileId": "p1", "profileName": "Comp", "hasChanges": True,
            "changes": {"account": None,
                        "instance": {"before": "i1", "after": "i2", "afterName": "B"},
                        "jvm": [], "game": [], "launch": []},
        }
        response, _ = _call("profiles.plan", {"profileId": "p1"}, svc)
        assert response["ok"] is True
        assert response["data"]["plan"]["hasChanges"] is True
        assert response["data"]["plan"]["changes"]["instance"]["after"] == "i2"

    def test_apply(self):
        svc = _service()
        svc.apply.return_value = {
            "profileId": "p1", "plan": {"changes": {}, "hasChanges": False},
            "appliedAt": 123.0,
        }
        response, _ = _call("profiles.apply", {"profileId": "p1"}, svc)
        assert response["ok"] is True
        assert response["data"]["applied"]["appliedAt"] == 123.0

    def test_revert_ok(self):
        svc = _service()
        svc.revert.return_value = True
        response, _ = _call("profiles.revert", {}, svc)
        assert response["ok"] is True
        assert response["data"]["reverted"] is True

    def test_revert_nothing_applied_maps_code(self):
        from core.errors.base import AntaresError

        svc = _service()
        svc.revert.side_effect = AntaresError("VALIDATION_FAILED",
                                              "No profile applied to revert")
        response, _ = _call("profiles.revert", {}, svc)
        # AntaresError.to_dict có code/message → envelope giữ đúng code
        assert response["ok"] is False
        assert response["error"]["code"] == "VALIDATION_FAILED"


class TestExportImportValidate:
    def test_export(self):
        svc = _service()
        svc.export_data.return_value = {
            "format": "antares-profile", "version": 1, "name": "A", "spec": {},
        }
        response, _ = _call("profiles.export", {"profileId": "p1"}, svc)
        assert response["ok"] is True
        assert response["data"]["export"]["format"] == "antares-profile"

    def test_import_requires_data_object(self):
        response, _ = _call("profiles.import", {})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_validate_requires_spec(self):
        response, _ = _call("profiles.validate", {})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_validate_returns_issues(self):
        svc = _service()
        svc.validate_spec.return_value = ["game: unknown key xyz"]
        response, _ = _call("profiles.validate", {"spec": {"game": {"xyz": 1}}}, svc)
        assert response["ok"] is True
        assert response["data"]["issues"] == ["game: unknown key xyz"]
