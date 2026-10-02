"""Tests cho B10 sidecar optimization.* / system.* handlers (§3/§4/§70/§35–§37).

Handlers bọc GameOptimizationService/SystemOptimizationService thật — mock ở
tầng factory `_optimization_service` / `_system_optimization`.
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


def _call(method: str, params: dict, *, ctx=None, opt=None, system=None):
    ctx = ctx if ctx is not None else mock.MagicMock()
    with mock.patch.object(sidecar, "_legacy_ctx", return_value=ctx), \
         mock.patch.object(sidecar, "_optimization_service",
                           return_value=opt or mock.MagicMock()), \
         mock.patch.object(sidecar, "_system_optimization",
                           return_value=system or mock.MagicMock()):
        response = sidecar.handle_request({"id": "t", "method": method, "params": params})
    return response


class TestRegistry:
    def test_all_b10_methods_registered(self):
        expected = [
            "optimization.scan", "optimization.plan", "optimization.apply",
            "optimization.rollback", "optimization.snapshot_info",
            "system.overview", "system.cleanup.scan", "system.cleanup.clean",
            "system.cleanup.undo", "system.cleanup.empty_trash",
            "system.power.status", "system.power.set_plan",
        ]
        for method in expected:
            assert method in sidecar.HANDLERS, f"missing handler: {method}"


class TestGameOptimization:
    def test_scan_without_instance(self):
        opt = mock.MagicMock()
        opt.scan.return_value = {
            "hardware": {"ramTotalMb": 16384, "cpuThreads": 12},
            "profiles": [{"id": "balanced", "labelKey": "balanced"}],
        }
        response = _call("optimization.scan", {}, opt=opt)
        assert response["ok"] is True
        assert response["data"]["scan"]["hardware"]["ramTotalMb"] == 16384
        opt.scan.assert_called_once_with(None)

    def test_scan_with_instance(self):
        ctx = _ctx_with_instance({"id": "i1"})
        opt = mock.MagicMock()
        opt.scan.return_value = {"hardware": {}, "currentProfile": None}
        response = _call("optimization.scan", {"instanceId": "i1"}, ctx=ctx, opt=opt)
        assert response["ok"] is True
        opt.scan.assert_called_once_with("i1")

    def test_scan_instance_not_found(self):
        ctx = _ctx_with_instance(None)
        response = _call("optimization.scan", {"instanceId": "ghost"}, ctx=ctx)
        assert response["ok"] is False
        assert response["error"]["code"] == "INSTANCE_NOT_FOUND"

    def test_plan_ok(self):
        ctx = _ctx_with_instance({"id": "i1"})
        opt = mock.MagicMock()
        opt.plan.return_value = {
            "instanceId": "i1", "profileId": "performance",
            "jvm": [{"field": "memory", "before": {"maxMb": 2048}, "after": {"maxMb": 4096}}],
            "minecraft": [{"field": "renderDistance", "before": "12", "after": "8"}],
            "hasChanges": True,
        }
        response = _call("optimization.plan",
                         {"instanceId": "i1", "profileId": "performance"}, ctx=ctx, opt=opt)
        assert response["ok"] is True
        plan = response["data"]["plan"]
        assert plan["hasChanges"] is True
        assert plan["jvm"][0]["field"] == "memory"
        opt.plan.assert_called_once_with("i1", "performance")

    def test_plan_requires_params(self):
        response = _call("optimization.plan", {"instanceId": "i1", "profileId": ""})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_plan_unknown_profile_kept(self):
        from core.errors.base import AntaresError

        ctx = _ctx_with_instance({"id": "i1"})
        opt = mock.MagicMock()
        opt.plan.side_effect = AntaresError("VALIDATION_FAILED", "Unknown profile: nope")
        response = _call("optimization.plan",
                         {"instanceId": "i1", "profileId": "nope"}, ctx=ctx, opt=opt)
        assert response["ok"] is False
        assert response["error"]["code"] == "VALIDATION_FAILED"

    def test_apply_ok(self):
        ctx = _ctx_with_instance({"id": "i1"})
        opt = mock.MagicMock()
        opt.apply.return_value = {
            "snapshot": {"file": "opt-123.json", "appliedAt": 123.0},
            "plan": {"jvm": [], "minecraft": []},
            "profile": "performance",
        }
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("optimization.apply",
                             {"instanceId": "i1", "profileId": "performance"},
                             ctx=ctx, opt=opt)
        assert response["ok"] is True
        assert response["data"]["applied"]["snapshot"]["file"] == "opt-123.json"
        opt.apply.assert_called_once_with("i1", "performance")
        n.assert_called_once()

    def test_rollback_ok(self):
        ctx = _ctx_with_instance({"id": "i1"})
        opt = mock.MagicMock()
        opt.rollback.return_value = {"restored": True}
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("optimization.rollback", {"instanceId": "i1"},
                             ctx=ctx, opt=opt)
        assert response["ok"] is True
        assert response["data"]["restored"] is True
        opt.rollback.assert_called_once_with("i1")
        n.assert_called_once()

    def test_rollback_no_snapshot_kept(self):
        from core.errors.base import AntaresError

        ctx = _ctx_with_instance({"id": "i1"})
        opt = mock.MagicMock()
        opt.rollback.side_effect = AntaresError(
            "VALIDATION_FAILED", "No optimization snapshot to restore")
        response = _call("optimization.rollback", {"instanceId": "i1"}, ctx=ctx, opt=opt)
        assert response["ok"] is False
        assert response["error"]["code"] == "VALIDATION_FAILED"

    def test_snapshot_info(self):
        opt = mock.MagicMock()
        opt.snapshot_info.return_value = {
            "file": "opt-123.json", "appliedAt": 123.0,
            "profile": "performance", "instanceFields": ["memory"],
        }
        response = _call("optimization.snapshot_info", {"instanceId": "i1"}, opt=opt)
        assert response["ok"] is True
        assert response["data"]["snapshot"]["profile"] == "performance"

    def test_snapshot_info_none(self):
        opt = mock.MagicMock()
        opt.snapshot_info.return_value = None
        response = _call("optimization.snapshot_info", {"instanceId": "i1"}, opt=opt)
        assert response["ok"] is True
        assert response["data"]["snapshot"] is None


class TestSystemOverview:
    def test_overview(self):
        system = mock.MagicMock()
        system.overview.return_value = {
            "hardware": {"ramTotalMb": 16384, "ramPercent": 57.4},
            "power": {"plan": "balanced"},
            "cleanupPreview": {"items": [], "totalBytes": 0},
        }
        response = _call("system.overview", {}, system=system)
        assert response["ok"] is True
        overview = response["data"]["overview"]
        assert overview["power"]["plan"] == "balanced"
        assert "cleanupPreview" in overview


class TestSystemCleanup:
    def test_scan(self):
        system = mock.MagicMock()
        system.cleanup_scan.return_value = {
            "items": [{"path": "logs", "bytes": 1024}], "totalBytes": 1024,
        }
        response = _call("system.cleanup.scan", {}, system=system)
        assert response["ok"] is True
        assert response["data"]["preview"]["totalBytes"] == 1024

    def test_clean_ok(self):
        system = mock.MagicMock()
        system.cleanup_clean.return_value = {"bytes": 2048, "cleanId": "c-1"}
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("system.cleanup.clean",
                             {"paths": ["logs", "cache"]}, system=system)
        assert response["ok"] is True
        assert response["data"]["clean"]["cleanId"] == "c-1"
        system.cleanup_clean.assert_called_once_with(["logs", "cache"])
        n.assert_called_once()

    def test_clean_requires_paths(self):
        for bad in ({}, {"paths": []}, {"paths": "logs"}, {"paths": [1, 2]}):
            response = _call("system.cleanup.clean", bad)
            assert response["ok"] is False, bad
            assert response["error"]["code"] == "CONFIG_INVALID", bad

    def test_undo_ok(self):
        system = mock.MagicMock()
        system.cleanup_undo.return_value = {"restored": 3}
        with mock.patch.object(sidecar, "notify"):
            response = _call("system.cleanup.undo", {"cleanId": "c-1"}, system=system)
        assert response["ok"] is True
        assert response["data"]["undo"]["restored"] == 3

    def test_undo_requires_clean_id(self):
        response = _call("system.cleanup.undo", {"cleanId": ""})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_empty_trash(self):
        system = mock.MagicMock()
        system.cleanup_empty_trash.return_value = 5
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("system.cleanup.empty_trash", {}, system=system)
        assert response["ok"] is True
        assert response["data"]["removed"] == 5
        n.assert_called_once()


class TestSystemPower:
    def test_status(self):
        system = mock.MagicMock()
        system.power_status.return_value = {
            "plan": "balanced", "plans": [{"id": "balanced", "labelKey": "balanced"}],
        }
        response = _call("system.power.status", {}, system=system)
        assert response["ok"] is True
        assert response["data"]["power"]["plan"] == "balanced"

    def test_set_plan_ok(self):
        system = mock.MagicMock()
        system.power_set_plan.return_value = True
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("system.power.set_plan",
                             {"planId": "high_performance"}, system=system)
        assert response["ok"] is True
        assert response["data"]["set"] is True
        system.power_set_plan.assert_called_once_with("high_performance")
        n.assert_called_once()

    def test_set_plan_failed(self):
        system = mock.MagicMock()
        system.power_set_plan.return_value = False
        response = _call("system.power.set_plan", {"planId": "nope"}, system=system)
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_set_plan_requires_plan_id(self):
        response = _call("system.power.set_plan", {})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"
