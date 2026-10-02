"""Tests cho B12 sidecar runtime.*/packet.* handlers (§12/§121/§122).

Runtime handlers mock `_runtime_service`; packet.* dùng ring buffer thật
(in-memory) — kiểm tra eviction/capture/export đúng §122.
"""

import sys
from pathlib import Path
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parent.parent.parent.parent
sys.path.insert(0, str(REPO_ROOT / "legacy" / "python"))

import sidecar  # noqa: E402


def _call(method: str, params: dict, *, ctx=None, runtime=None):
    ctx = ctx if ctx is not None else mock.MagicMock()
    with mock.patch.object(sidecar, "_legacy_ctx", return_value=ctx), \
         mock.patch.object(sidecar, "_runtime_service",
                           return_value=runtime or mock.MagicMock()):
        return sidecar.handle_request({"id": "t", "method": method, "params": params})


def _fresh_ring():
    """Ring buffer mới cho mỗi test (tránh state leak giữa các test)."""
    ring = sidecar._PacketRing()
    with mock.patch.object(sidecar, "_packet_ring", return_value=ring):
        yield ring


class TestRegistry:
    def test_all_b12_methods_registered(self):
        expected = [
            "runtime.endpoint", "runtime.start", "runtime.sessions",
            "runtime.metrics", "runtime.pairing",
            "packet.ingest", "packet.list", "packet.stats",
            "packet.capture", "packet.clear", "packet.export",
        ]
        for method in expected:
            assert method in sidecar.HANDLERS, f"missing handler: {method}"


class TestRuntime:
    def test_endpoint(self):
        runtime = mock.MagicMock()
        runtime.endpoint.return_value = {
            "url": "http://127.0.0.1:8971/packet", "token": "tok",
            "packetTypes": ["runtime.hello", "runtime.performance"],
        }
        response = _call("runtime.endpoint", {}, runtime=runtime)
        assert response["ok"] is True
        assert response["data"]["endpoint"]["token"] == "tok"

    def test_start(self):
        runtime = mock.MagicMock()
        runtime.start.return_value = {"url": "http://127.0.0.1:8971/packet"}
        response = _call("runtime.start", {}, runtime=runtime)
        assert response["ok"] is True
        assert response["data"]["started"]["url"].startswith("http://127.0.0.1")

    def test_sessions(self):
        runtime = mock.MagicMock()
        runtime.sessions.return_value = [
            {"sessionId": "s1", "instanceId": "i1", "connectedAt": 1.0, "lastSeen": 2.0},
        ]
        response = _call("runtime.sessions", {}, runtime=runtime)
        assert response["ok"] is True
        assert response["data"]["sessions"][0]["sessionId"] == "s1"

    def test_metrics_all(self):
        runtime = mock.MagicMock()
        runtime.metrics.return_value = {"sessions": {"s1": [{"ts": 1.0, "fps": 120}]}}
        response = _call("runtime.metrics", {}, runtime=runtime)
        assert response["ok"] is True
        assert response["data"]["metrics"]["sessions"]["s1"][0]["fps"] == 120
        runtime.metrics.assert_called_once_with(None)

    def test_metrics_by_session(self):
        runtime = mock.MagicMock()
        runtime.metrics.return_value = {"sessions": {"s2": []}}
        _call("runtime.metrics", {"sessionId": "s2"}, runtime=runtime)
        runtime.metrics.assert_called_once_with("s2")

    def test_pairing_ok(self):
        ctx = mock.MagicMock()
        inst = mock.MagicMock()
        inst.get.return_value = {"id": "i1"}
        ctx.get.side_effect = lambda name: {"instances": inst}.get(name, mock.MagicMock())
        runtime = mock.MagicMock()
        runtime.write_pairing_for_instance.return_value = {"file": "/x/companion.json"}
        response = _call("runtime.pairing", {"instanceId": "i1"}, ctx=ctx, runtime=runtime)
        assert response["ok"] is True
        assert response["data"]["pairing"]["file"].endswith("companion.json")
        runtime.write_pairing_for_instance.assert_called_once_with("i1")

    def test_pairing_instance_not_found(self):
        ctx = mock.MagicMock()
        inst = mock.MagicMock()
        inst.get.return_value = None
        ctx.get.side_effect = lambda name: {"instances": inst}.get(name, mock.MagicMock())
        response = _call("runtime.pairing", {"instanceId": "ghost"}, ctx=ctx)
        assert response["ok"] is False
        assert response["error"]["code"] == "INSTANCE_NOT_FOUND"


class TestPacketIngest:
    def test_ingest_ok(self):
        for ring in _fresh_ring():
            packet = {"version": 1, "type": "runtime.performance",
                      "timestamp": 1.0, "payload": {"fps": 120, "frameMs": 8.3}}
            ctx = mock.MagicMock()
            with mock.patch.object(sidecar, "_legacy_ctx", return_value=ctx), \
                 mock.patch.object(sidecar, "_runtime_service") as rt:
                response = sidecar.handle_request(
                    {"id": "t", "method": "packet.ingest", "params": {"packet": packet}})
                rt.return_value._on_packet.assert_called_once()
            assert response["ok"] is True
            assert response["data"]["packetId"] == 1
            entries = ring.list()
            assert entries[0]["name"] == "runtime.performance"
            assert entries[0]["size"] > 0
            # capture OFF → không payload (§121 metadata mode)
            assert "payload" not in entries[0]

    def test_ingest_invalid_version(self):
        for _ in _fresh_ring():
            response = _call("packet.ingest",
                             {"packet": {"version": 2, "type": "x", "payload": {}}})
            assert response["ok"] is False
            assert response["error"]["code"] == "CONFIG_INVALID"

    def test_ingest_missing_payload(self):
        for _ in _fresh_ring():
            response = _call("packet.ingest",
                             {"packet": {"version": 1, "type": "x"}})
            assert response["ok"] is False
            assert response["error"]["code"] == "CONFIG_INVALID"

    def test_ingest_unknown_type_still_recorded(self):
        """Ingest ghi metadata cả type lạ — inspector là công cụ debug."""
        for ring in _fresh_ring():
            packet = {"version": 1, "type": "runtime.future_thing", "payload": {}}
            with mock.patch.object(sidecar, "_runtime_service") as rt:
                response = _call("packet.ingest", {"packet": packet})
            assert response["ok"] is True
            assert ring.list()[0]["name"] == "runtime.future_thing"


class TestPacketRing:
    def test_list_limit(self):
        for ring in _fresh_ring():
            for i in range(10):
                ring.append("runtime.performance", 100 + i, "in")
            response = _call("packet.list", {"limit": 3})
            assert response["ok"] is True
            packets = response["data"]["packets"]
            assert len(packets) == 3
            # mới nhất cuối
            assert packets[-1]["packetId"] == 10
            stats = response["data"]["stats"]
            assert stats["count"] == 10
            assert stats["dropped"] == 0

    def test_eviction_oldest_first(self):
        for ring in _fresh_ring():
            ring.set_cap(100)
            for i in range(150):
                ring.append("runtime.chat", 10, "in")
            stats = ring.stats()
            assert stats["count"] == 100
            assert stats["dropped"] == 50
            entries = ring.list(limit=100)
            # entry cũ nhất còn lại là packetId 51
            assert entries[0]["packetId"] == 51

    def test_capture_payload_on(self):
        for ring in _fresh_ring():
            with mock.patch.object(sidecar, "_packet_ring", return_value=ring):
                _call("packet.capture", {"enabled": True})
                packet = {"version": 1, "type": "runtime.chat",
                          "payload": {"message": "hello"}}
                with mock.patch.object(sidecar, "_runtime_service"):
                    _call("packet.ingest", {"packet": packet})
            entries = ring.list()
            assert entries[0]["payload"]["message"] == "hello"
            assert ring.stats()["capturePayload"] is True

    def test_capture_off_no_payload(self):
        for ring in _fresh_ring():
            with mock.patch.object(sidecar, "_packet_ring", return_value=ring):
                packet = {"version": 1, "type": "runtime.chat",
                          "payload": {"message": "secret"}}
                with mock.patch.object(sidecar, "_runtime_service"):
                    _call("packet.ingest", {"packet": packet})
            assert "payload" not in ring.list()[0]

    def test_clear(self):
        for ring in _fresh_ring():
            ring.append("runtime.chat", 10, "in")
            with mock.patch.object(sidecar, "_packet_ring", return_value=ring):
                response = _call("packet.clear", {})
            assert response["ok"] is True
            assert response["data"]["stats"]["count"] == 0
            assert ring.list() == []

    def test_export_on_request(self):
        for ring in _fresh_ring():
            for i in range(5):
                ring.append("runtime.performance", 50, "in")
            with mock.patch.object(sidecar, "_packet_ring", return_value=ring):
                response = _call("packet.export", {})
            assert response["ok"] is True
            export = response["data"]["export"]
            assert len(export["packets"]) == 5
            assert export["stats"]["count"] == 5

    def test_cap_developer_limit(self):
        for ring in _fresh_ring():
            ring.set_cap(999_999)
            assert ring.cap == sidecar.PACKET_RING_DEV
            ring.set_cap(1)
            assert ring.cap == 100  # floor

    def test_stats_initial(self):
        for ring in _fresh_ring():
            stats = ring.stats()
            assert stats == {"count": 0, "cap": sidecar.PACKET_RING_DEFAULT,
                             "dropped": 0, "bytes": 0, "capturePayload": False}
