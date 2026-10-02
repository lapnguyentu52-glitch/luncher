"""Tests cho legacy sidecar protocol (Milestone 5 — §43/§234)."""

import io
import json
import sys
from pathlib import Path
from unittest import mock

import pytest

REPO_ROOT = Path(__file__).resolve().parent.parent.parent.parent
sys.path.insert(0, str(REPO_ROOT / "legacy" / "python"))

import sidecar  # noqa: E402


class TestHandshake:
    def test_version_returns_protocol_match(self):
        response = sidecar.handle_request({"id": "r1", "method": "health.version", "params": {}})
        assert response["ok"] is True
        assert response["data"]["protocol"] == sidecar.PROTOCOL_VERSION
        assert response["data"]["service"] == "antares-legacy"
        assert "python" in response["data"]

    def test_ping_pong(self):
        response = sidecar.handle_request({"id": "r2", "method": "health.ping", "params": {"ts": 5}})
        assert response["ok"] is True
        assert response["data"]["pong"] is True
        assert response["data"]["ts"] == 5

    def test_contract_has_required_handlers(self):
        sidecar.assert_contract()  # không raise


class TestMethods:
    def test_echo_roundtrip(self):
        params = {"hello": [1, 2, 3]}
        response = sidecar.handle_request({"id": "r3", "method": "app.echo", "params": params})
        assert response["ok"] is True
        assert response["data"] == params

    def test_storage_root_returns_dict(self):
        fake_ctx = mock.MagicMock()
        fake_ctx.paths.data = Path("/tmp/antares-data")
        with mock.patch.object(sidecar, "_legacy_ctx", return_value=fake_ctx):
            response = sidecar.handle_request({"id": "r4", "method": "app.storage_root", "params": {}})
        assert response["ok"] is True
        assert response["data"]["root"] == "/tmp/antares-data"

    def test_method_not_found_is_graceful(self):
        response = sidecar.handle_request({"id": "r5", "method": "no.such", "params": {}})
        assert response["ok"] is False
        assert response["error"]["code"] == "METHOD_NOT_FOUND"

    def test_handler_crash_returns_error_not_raise(self):
        # HANDLERS bind handler reference lúc import — patch thẳng entry trong dict
        with mock.patch.dict(
            sidecar.HANDLERS, {"app.echo": mock.Mock(side_effect=RuntimeError("boom"))}
        ):
            response = sidecar.handle_request({"id": "r6", "method": "app.echo", "params": {}})
        assert response["ok"] is False
        assert response["error"]["code"] == "LEGACY_INTERNAL"
        assert "RuntimeError" in response["error"]["message"]


class TestMainLoop:
    def _run_main(self, stdin_lines: str) -> list[dict]:
        fake_stdin = io.StringIO(stdin_lines + "\n")
        captured = io.StringIO()
        with mock.patch.object(sys, "stdin", fake_stdin), mock.patch.object(
            sys, "stdout", captured
        ):
            exit_code = sidecar.main()
        assert exit_code == 0
        lines = [ln for ln in captured.getvalue().splitlines() if ln.strip()]
        return [json.loads(ln) for ln in lines]

    def test_full_conversation(self):
        messages = self._run_main(
            "\n".join(
                [
                    json.dumps({"id": "a", "method": "health.version", "params": {}}),
                    json.dumps({"id": "b", "method": "health.ping", "params": {}}),
                    json.dumps({"id": "c", "method": "app.echo", "params": {"x": 1}}),
                ]
            )
        )
        # 3 responses + sidecar.started/stopped notifications
        responses = [m for m in messages if "id" in m]
        notifications = [m for m in messages if "event" in m]
        assert [r["id"] for r in responses] == ["a", "b", "c"]
        assert all(r["ok"] for r in responses)
        events = [n["event"] for n in notifications]
        assert events[0] == "sidecar.started"
        assert events[-1] == "sidecar.stopped"

    def test_invalid_json_line_is_skipped(self):
        messages = self._run_main(
            "not-json\n" + json.dumps({"id": "ok1", "method": "health.ping", "params": {}})
        )
        responses = [m for m in messages if "id" in m]
        assert len(responses) == 1
        assert responses[0]["id"] == "ok1"

    def test_non_dict_json_is_skipped(self):
        messages = self._run_main(
            "[1,2,3]\n" + json.dumps({"id": "ok2", "method": "health.ping", "params": {}})
        )
        responses = [m for m in messages if "id" in m]
        assert len(responses) == 1

    def test_oversized_line_is_skipped(self):
        big = "x" * (sidecar.MAX_LINE_BYTES + 10)
        messages = self._run_main(
            json.dumps({"id": "big", "method": "health.ping", "params": {"pad": big}})
            + "\n"
            + json.dumps({"id": "after", "method": "health.ping", "params": {}})
        )
        responses = [m for m in messages if "id" in m]
        # oversized bị skip, request sau vẫn xử lý
        assert [r["id"] for r in responses] == ["after"]


class TestNotifications:
    def test_notify_shape(self):
        captured = io.StringIO()
        with mock.patch.object(sys, "stdout", captured):
            sidecar.notify("custom.event", {"k": 1})
        message = json.loads(captured.getvalue().strip())
        assert message == {"event": "custom.event", "payload": {"k": 1}}
