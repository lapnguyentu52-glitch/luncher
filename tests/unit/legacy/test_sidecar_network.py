"""Tests cho B11 sidecar net.* handlers (§42 — Network Lab).

check_endpoints/tcp_check/mc_ping được mock ở tầng module
`services.diagnostics.net` (tránh network thật); net.probe dùng composition
thật trên tcp_check đã mock; net.dns mock socket.getaddrinfo.
"""

import sys
from pathlib import Path
from unittest import mock

REPO_ROOT = Path(__file__).resolve().parent.parent.parent.parent
sys.path.insert(0, str(REPO_ROOT / "legacy" / "python"))

import sidecar  # noqa: E402


def _call(method: str, params: dict):
    """Call dispatch thật; bridge ctx có mặt (net handlers không cần service ctx)."""
    ctx = mock.MagicMock()
    with mock.patch.object(sidecar, "_legacy_ctx", return_value=ctx):
        return sidecar.handle_request({"id": "t", "method": method, "params": params})


class TestRegistry:
    def test_all_net_methods_registered(self):
        expected = [
            "net.endpoints", "net.tcp", "net.ping", "net.probe", "net.dns",
        ]
        for method in expected:
            assert method in sidecar.HANDLERS, f"missing handler: {method}"


class TestEndpoints:
    def test_endpoints_ok(self):
        fake = [{"id": "mojang", "host": "launchermeta.mojang.com", "port": 443,
                 "ok": True, "ms": 21.5, "error": None}]
        with mock.patch("services.diagnostics.net.check_endpoints",
                        return_value=fake) as m:
            response = _call("net.endpoints", {})
        assert response["ok"] is True
        assert response["data"]["endpoints"][0]["id"] == "mojang"
        m.assert_called_once()

    def test_endpoints_unavailable(self):
        # Patch trực tiếp (không qua _call — tránh double-patch _legacy_ctx)
        with mock.patch.object(sidecar, "_legacy_ctx", return_value=None):
            response = sidecar.handle_request(
                {"id": "t", "method": "net.endpoints", "params": {}})
        assert response["ok"] is False
        assert response["error"]["code"] == "LEGACY_UNAVAILABLE"


class TestTcp:
    def test_tcp_ok(self):
        fake = {"host": "example.com", "port": 443, "ok": True, "ms": 12.3, "error": None}
        with mock.patch("services.diagnostics.net.tcp_check", return_value=fake) as m:
            response = _call("net.tcp", {"host": "example.com", "port": 443})
        assert response["ok"] is True
        assert response["data"]["tcp"]["ok"] is True
        m.assert_called_once_with("example.com", 443, timeout=4.0)

    def test_tcp_timeout_capped(self):
        fake = {"host": "h", "port": 1, "ok": True, "ms": 1.0, "error": None}
        with mock.patch("services.diagnostics.net.tcp_check", return_value=fake) as m:
            _call("net.tcp", {"host": "h", "port": 1, "timeout": 999})
        m.assert_called_once_with("h", 1, timeout=10.0)

    def test_tcp_invalid_host(self):
        response = _call("net.tcp", {"host": "", "port": 443})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"

    def test_tcp_invalid_port(self):
        response = _call("net.tcp", {"host": "h", "port": 99999})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"


class TestPing:
    def test_ping_ok(self):
        fake = {"host": "mc.hypixel.net", "port": 25565, "online": True,
                "motd": "Hypixel", "players": {"online": 100, "max": 200},
                "version": {"name": "1.21", "protocol": 767},
                "latencyMs": 35.2, "connectMs": 12.0, "favicon": True,
                "modinfo": None}
        with mock.patch("services.diagnostics.net.mc_ping", return_value=fake) as m:
            response = _call("net.ping", {"host": "mc.hypixel.net"})
        assert response["ok"] is True
        ping = response["data"]["ping"]
        assert ping["online"] is True
        assert ping["motd"] == "Hypixel"
        assert ping["players"]["max"] == 200
        m.assert_called_once_with("mc.hypixel.net", 25565, timeout=4.0)

    def test_ping_requires_host(self):
        response = _call("net.ping", {"host": " "})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"


class TestProbe:
    def test_probe_stats_and_timeline(self):
        seq = [10.0, 12.0, 14.0, 16.0]

        def fake_tcp(host, port, *, timeout):
            i = seq.pop(0) if seq else 20.0
            return {"host": host, "port": port, "ok": True, "ms": i, "error": None}

        with mock.patch("services.diagnostics.net.tcp_check", side_effect=fake_tcp):
            response = _call("net.probe", {"host": "example.com", "port": 443,
                                           "count": 4})
        assert response["ok"] is True
        probe = response["data"]["probe"]
        assert probe["count"] == 4
        assert probe["ok"] == 4
        stats = probe["stats"]
        assert stats["min"] == 10.0
        assert stats["max"] == 16.0
        assert stats["avg"] == 13.0
        # jitter = avg(|delta|) của [10,12,14,16] = 2.0
        assert stats["jitter"] == 2.0
        assert stats["loss"] == 0.0
        assert len(probe["timeline"]) == 4
        assert probe["timeline"][0]["seq"] == 0

    def test_probe_with_loss(self):
        seq = [10.0, None, 14.0]

        def fake_tcp(host, port, *, timeout):
            v = seq.pop(0) if seq else None
            if v is None:
                return {"host": host, "port": port, "ok": False, "ms": 12.0,
                        "error": "timeout"}
            return {"host": host, "port": port, "ok": True, "ms": v, "error": None}

        with mock.patch("services.diagnostics.net.tcp_check", side_effect=fake_tcp):
            response = _call("net.probe", {"host": "example.com", "port": 443,
                                           "count": 3})
        assert response["ok"] is True
        probe = response["data"]["probe"]
        assert probe["ok"] == 2
        assert probe["stats"]["loss"] == round(100.0 / 3, 1)
        # jitter tính trên mẫu ok liên tiếp [10, 14] = 4.0
        assert probe["stats"]["jitter"] == 4.0

    def test_probe_all_failed(self):
        with mock.patch("services.diagnostics.net.tcp_check",
                        return_value={"host": "h", "port": 1, "ok": False,
                                      "ms": None, "error": "refused"}):
            response = _call("net.probe", {"host": "h", "port": 1, "count": 2})
        assert response["ok"] is True
        stats = response["data"]["probe"]["stats"]
        assert stats["min"] is None
        assert stats["loss"] == 100.0

    def test_probe_count_capped(self):
        calls = []

        def fake_tcp(host, port, *, timeout):
            calls.append(1)
            return {"host": host, "port": port, "ok": True, "ms": 5.0, "error": None}

        with mock.patch("services.diagnostics.net.tcp_check", side_effect=fake_tcp):
            _call("net.probe", {"host": "h", "port": 1, "count": 999})
        assert len(calls) == 30

    def test_probe_requires_host(self):
        response = _call("net.probe", {"host": "", "port": 1})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"


class TestDns:
    def test_dns_ok(self):
        fake = [
            ("", "", "", "", ("93.184.216.34", 0)),
            ("", "", "", "", ("2606:2800:220:1:248:1893:25c8:1946", 0)),
            ("", "", "", "", ("93.184.216.34", 0)),  # duplicate
        ]
        with mock.patch("socket.getaddrinfo", return_value=fake):
            response = _call("net.dns", {"host": "example.com"})
        assert response["ok"] is True
        dns = response["data"]["dns"]
        assert dns["ok"] is True
        assert dns["addresses"] == ["93.184.216.34",
                                    "2606:2800:220:1:248:1893:25c8:1946"]
        assert dns["ms"] >= 0

    def test_dns_nxdomain(self):
        with mock.patch("socket.getaddrinfo",
                        side_effect=OSError("Name or service not known")):
            response = _call("net.dns", {"host": "nope.invalid"})
        assert response["ok"] is True  # DNS fail là data, không phải bridge lỗi
        dns = response["data"]["dns"]
        assert dns["ok"] is False
        assert "Name or service" in dns["error"]

    def test_dns_requires_host(self):
        response = _call("net.dns", {"host": ""})
        assert response["ok"] is False
        assert response["error"]["code"] == "CONFIG_INVALID"
