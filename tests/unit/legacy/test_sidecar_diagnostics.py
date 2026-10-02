"""Tests cho B13 sidecar console.*/repair.*/diagnostic.export (§41/§39/§16).

Handlers bọc LogAnalyzer/RepairService thật — mock ở tầng factory.
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


def _call(method: str, params: dict, *, ctx=None, analyzer=None, repair=None):
    ctx = ctx if ctx is not None else mock.MagicMock()
    with mock.patch.object(sidecar, "_legacy_ctx", return_value=ctx), \
         mock.patch.object(sidecar, "_log_analyzer",
                           return_value=analyzer or mock.MagicMock()), \
         mock.patch.object(sidecar, "_repair_service",
                           return_value=repair or mock.MagicMock()):
        return sidecar.handle_request({"id": "t", "method": method, "params": params})


class TestRegistry:
    def test_all_b13_methods_registered(self):
        expected = [
            "console.sources", "console.read", "console.analyze",
            "console.insights", "repair.actions", "repair.scan",
            "repair.run", "diagnostic.export",
        ]
        for method in expected:
            assert method in sidecar.HANDLERS, f"missing handler: {method}"


class TestConsole:
    def test_sources(self):
        analyzer = mock.MagicMock()
        analyzer.sources.return_value = [
            {"id": "launcher", "available": True, "bytes": 0, "lines": 42},
            {"id": "crash", "available": False, "bytes": 0, "count": 0},
        ]
        response = _call("console.sources", {}, analyzer=analyzer)
        assert response["ok"] is True
        assert response["data"]["sources"][0]["id"] == "launcher"
        analyzer.sources.assert_called_once_with(None)

    def test_sources_with_instance(self):
        analyzer = mock.MagicMock()
        analyzer.sources.return_value = []
        _call("console.sources", {"instanceId": "i1"}, analyzer=analyzer)
        analyzer.sources.assert_called_once_with("i1")

    def test_read_ok(self):
        analyzer = mock.MagicMock()
        analyzer.read.return_value = {
            "source": "launcher", "instanceId": None,
            "lines": [{"n": 1, "text": "INFO antares ready"}], "truncated": False,
        }
        response = _call("console.read", {"source": "launcher", "limit": 100},
                         analyzer=analyzer)
        assert response["ok"] is True
        assert response["data"]["log"]["lines"][0]["text"] == "INFO antares ready"
        analyzer.read.assert_called_once_with("launcher", None, limit=100)

    def test_read_requires_source(self):
        response = _call("console.read", {})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_read_unknown_source_kept(self):
        from core.errors.base import AntaresError

        analyzer = mock.MagicMock()
        analyzer.read.side_effect = AntaresError("VALIDATION_FAILED",
                                                 "Unknown log source: x")
        response = _call("console.read", {"source": "x"}, analyzer=analyzer)
        assert response["ok"] is False
        assert response["error"]["code"] == "VALIDATION_FAILED"

    def test_analyze(self):
        analyzer = mock.MagicMock()
        analyzer.analyze.return_value = {
            "instanceId": None,
            "sources": {"launcher": 100},
            "insights": [{
                "id": "java_out_of_memory", "count": 2, "severity": "error",
                "seed": "oom", "recommend": "profiles.ramMax",
                "action": {"kind": "route", "target": "gameOptimization"},
                "firstSeen": None, "lastSeen": None, "sources": ["launcher"],
                "lines": [{"source": "launcher", "line": 10, "text": "OutOfMemoryError"}],
            }],
            "errorCount": 2, "warnCount": 0, "durationMs": 5,
        }
        response = _call("console.analyze", {}, analyzer=analyzer)
        assert response["ok"] is True
        analysis = response["data"]["analysis"]
        assert analysis["errorCount"] == 2
        assert analysis["insights"][0]["id"] == "java_out_of_memory"
        analyzer.analyze.assert_called_once_with(None, None)

    def test_insights_compact(self):
        analyzer = mock.MagicMock()
        analyzer.analyze.return_value = {
            "insights": [], "errorCount": 0, "warnCount": 1,
            "sources": {}, "durationMs": 1, "instanceId": None,
        }
        response = _call("console.insights", {}, analyzer=analyzer)
        assert response["ok"] is True
        data = response["data"]
        assert data["warnCount"] == 1
        # compact — không kèm sources/duration
        assert "sources" not in data and "durationMs" not in data


class TestRepair:
    def test_actions(self):
        response = _call("repair.actions", {})
        assert response["ok"] is True
        actions = response["data"]["actions"]
        assert "instance_metadata" in actions
        assert "missing_dirs" in actions

    def test_scan_ok(self):
        ctx = _ctx_with_instance({"id": "i1"})
        repair = mock.MagicMock()
        repair.scan.return_value = {
            "action": "missing_dirs", "findings": [], "planned": [],
            "hasIssues": False,
        }
        response = _call("repair.scan", {"action": "missing_dirs", "instanceId": "i1"},
                         ctx=ctx, repair=repair)
        assert response["ok"] is True
        assert response["data"]["scan"]["hasIssues"] is False
        repair.scan.assert_called_once_with("missing_dirs", "i1")

    def test_scan_requires_action(self):
        response = _call("repair.scan", {})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_scan_instance_not_found(self):
        ctx = _ctx_with_instance(None)
        response = _call("repair.scan", {"action": "missing_dirs", "instanceId": "ghost"},
                         ctx=ctx)
        assert response["ok"] is False
        assert response["error"]["code"] == "INSTANCE_NOT_FOUND"

    def test_scan_unknown_action_kept(self):
        from core.errors.base import AntaresError

        repair = mock.MagicMock()
        repair.scan.side_effect = AntaresError("VALIDATION_FAILED",
                                               "Unknown repair action: nope")
        response = _call("repair.scan", {"action": "nope"}, repair=repair)
        assert response["ok"] is False
        assert response["error"]["code"] == "VALIDATION_FAILED"

    def test_run_ok(self):
        ctx = _ctx_with_instance({"id": "i1"})
        repair = mock.MagicMock()
        repair.run.return_value = {"done": ["created mods/"], "trashed": 0}
        with mock.patch.object(sidecar, "notify") as n:
            response = _call("repair.run",
                             {"action": "missing_dirs", "instanceId": "i1"},
                             ctx=ctx, repair=repair)
        assert response["ok"] is True
        assert response["data"]["result"]["done"] == ["created mods/"]
        n.assert_called_once()

    def test_run_requires_action(self):
        response = _call("repair.run", {})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"


class TestDiagnosticExport:
    def test_export_shape(self):
        analyzer = mock.MagicMock()
        analyzer.analyze.return_value = {
            "insights": [], "errorCount": 0, "warnCount": 0,
            "sources": {}, "durationMs": 1, "instanceId": None,
        }
        analyzer.sources.return_value = [{"id": "launcher", "available": False,
                                          "bytes": 0, "lines": 0}]
        repair = mock.MagicMock()
        repair.scan.return_value = {"action": "x", "findings": [], "planned": [],
                                    "hasIssues": False}
        response = _call("diagnostic.export", {}, analyzer=analyzer, repair=repair)
        assert response["ok"] is True
        export = response["data"]["export"]
        assert export["appVersion"] == sidecar.SERVICE_VERSION
        assert "generatedAt" in export
        assert export["sources"][0]["id"] == "launcher"
        assert len(export["repairScans"]) == 3

    def test_export_repair_failure_tolerated(self):
        analyzer = mock.MagicMock()
        analyzer.analyze.return_value = {
            "insights": [], "errorCount": 0, "warnCount": 0,
            "sources": {}, "durationMs": 1, "instanceId": None,
        }
        analyzer.sources.return_value = []
        repair = mock.MagicMock()
        repair.scan.side_effect = RuntimeError("boom")
        response = _call("diagnostic.export", {}, analyzer=analyzer, repair=repair)
        assert response["ok"] is True
        assert response["data"]["export"]["repairScans"] == []
