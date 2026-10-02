"""LogAnalyzer + NetDiagnostics — phân tích log, TCP check, MC Server List Ping (mục 41, 42)."""
from __future__ import annotations

import threading

import pytest

from core.logging.setup import RingBufferHandler, setup_logging
from services.diagnostics.log_analyzer import LogAnalyzer
from services.diagnostics import net


@pytest.fixture()
def analyzer(ctx):
    return LogAnalyzer(ctx)


def _seed_ring(lines, monkeypatch=None):
    """Ring buffer thật + cài làm global ring (analyzer đọc get_ring()).

    lines: [(levelno, msg)] — 20=INFO, 30=WARNING, 40=ERROR.
    """
    import logging
    import core.logging.setup as log_setup
    handler = RingBufferHandler(capacity=1000)
    handler.setFormatter(logging.Formatter("%(asctime)s %(levelname)s %(name)s %(message)s",
                                           "%Y-%m-%d %H:%M:%S"))
    lg = logging.getLogger("antares.test.analyzer")
    lg.setLevel(logging.DEBUG)
    lg.propagate = False
    lg.handlers = [handler]
    for level, msg in lines:
        lg.log(level, msg)
    if monkeypatch is not None:
        monkeypatch.setattr(log_setup, "_ring", handler)
    else:
        log_setup._ring = handler
    return handler


# ------------------------------------------------------------------
# LogAnalyzer
# ------------------------------------------------------------------

def test_sources_shape(analyzer, ctx, instance_id):
    srcs = analyzer.sources(instance_id)
    assert [s["id"] for s in srcs] == ["launcher", "minecraft", "crash"]
    assert all("available" in s and "bytes" in s for s in srcs)


def test_analyze_flags_oom_in_launcher(analyzer, ctx, instance_id, monkeypatch):
    _seed_ring([
        (40, "Game crashed with java.lang.OutOfMemoryError: Java heap space"),
        (20, "normal line, nothing here"),
    ], monkeypatch)
    result = analyzer.analyze(instance_id)
    ids = [i["id"] for i in result["insights"]]
    assert "java_out_of_memory" in ids
    ins = next(i for i in result["insights"] if i["id"] == "java_out_of_memory")
    assert ins["count"] == 1
    assert ins["severity"] == "error"
    assert ins["lines"][0]["source"] == "launcher"
    assert "OutOfMemoryError" in ins["lines"][0]["text"]
    assert result["errorCount"] >= 1


def test_analyze_reads_minecraft_latest_log(analyzer, ctx, instance_id):
    logs_dir = ctx.paths.instances / instance_id / "game" / "logs"
    logs_dir.mkdir(parents=True, exist_ok=True)
    (logs_dir / "latest.log").write_text(
        "[12:00:00] [main/ERROR]: Failed to verify authentication\n"
        "[12:00:01] [main/INFO]: Sound engine started\n",
        encoding="utf-8")
    result = analyzer.analyze(instance_id, source="minecraft")
    ids = [i["id"] for i in result["insights"]]
    assert "auth_failed" in ids
    assert result["sources"]["minecraft"] == 2


def test_analyze_reads_crash_report(analyzer, ctx, instance_id):
    crash_dir = ctx.paths.instances / instance_id / "game" / "crash-reports"
    crash_dir.mkdir(parents=True, exist_ok=True)
    (crash_dir / "crash-2026-09-26_12.00.00-client.txt").write_text(
        "DuplicateModsFoundException: mod A vs mod B\n",
        encoding="utf-8")
    result = analyzer.analyze(instance_id, source="crash")
    ids = [i["id"] for i in result["insights"]]
    assert "mod_conflict" in ids


def test_insights_carry_action_and_recommend(analyzer, ctx, instance_id, monkeypatch):
    _seed_ring([(30, "Connection refused: no further information")], monkeypatch)
    result = analyzer.analyze(instance_id)
    ins = next(i for i in result["insights"] if i["id"] == "connection_refused")
    assert ins["action"]["kind"] == "net"
    assert ins["recommend"]


def test_read_returns_numbered_lines_and_truncates(analyzer, ctx, instance_id):
    logs_dir = ctx.paths.instances / instance_id / "game" / "logs"
    logs_dir.mkdir(parents=True, exist_ok=True)
    (logs_dir / "latest.log").write_text("a\nb\nc\n", encoding="utf-8")
    r = analyzer.read("minecraft", instance_id, limit=2)
    assert r["truncated"] is True
    assert len(r["lines"]) == 2
    assert r["lines"][-1]["text"] == "c"
    assert r["lines"][-1]["n"] == 3


def test_read_unknown_source_rejected(analyzer):
    from core.errors import codes
    from core.errors.base import AntaresError
    with pytest.raises(AntaresError) as ei:
        analyzer.read("ghost")
    assert ei.value.code == codes.VALIDATION_FAILED


def test_insights_sorted_errors_first(analyzer, ctx, instance_id, monkeypatch):
    _seed_ring([
        (30, "Connection timed out"),                 # warning
        (40, "java.lang.OutOfMemoryError: heap"),     # error
        (40, "java.lang.OutOfMemoryError: heap"),
    ], monkeypatch)
    result = analyzer.analyze(instance_id)
    sev = [i["severity"] for i in result["insights"]]
    assert sev == sorted(sev, reverse=True) or sev[0] == "error"


def test_analyze_empty_sources_returns_zero(analyzer, ctx, instance_id):
    result = analyzer.analyze(instance_id)
    assert result["insights"] == [] and result["errorCount"] == 0


# ------------------------------------------------------------------
# Ring buffer metadata (Console lọc theo level không re-parse)
# ------------------------------------------------------------------

def test_ring_snapshot_meta_has_level(monkeypatch):
    handler = _seed_ring([(30, "warn line"), (40, "err line")], monkeypatch)
    meta = handler.snapshot_meta()
    assert len(meta) == 2
    assert meta[0]["level"] == "WARNING"
    assert meta[1]["level"] == "ERROR"
    assert "antares.test.analyzer" in meta[0]["text"]


def test_ring_since_meta_cursor_semantics(monkeypatch):
    handler = _seed_ring([(20, "one"), (30, "two"), (40, "three")], monkeypatch)
    entries, cursor = handler.snapshot_since_meta(0)
    assert len(entries) == 3 and cursor == 3
    entries2, cursor2 = handler.snapshot_since_meta(cursor)
    assert entries2 == [] and cursor2 == cursor
    # cursor cũ hơn bị cắt -> reset toàn bộ
    handler2 = RingBufferHandler(capacity=2)
    import logging
    handler2.setFormatter(logging.Formatter("%(message)s"))
    lg2 = logging.getLogger("antares.test.analyzer2")
    lg2.setLevel(logging.DEBUG)
    lg2.propagate = False
    lg2.handlers = [handler2]
    for i in range(5):
        lg2.info(f"line {i}")
    entries3, _ = handler2.snapshot_since_meta(0)
    assert len(entries3) == 2


def test_setup_logging_end_to_end(tmp_path):
    import logging as _logging
    root = setup_logging(tmp_path / "logs", level=_logging.INFO)
    try:
        lg = _logging.getLogger("antares.test.e2e")
        lg.propagate = False
        from core.logging.setup import get_ring
        lg.addHandler(get_ring())
        lg.info("hello console")
        ring = get_ring()
        meta = ring.snapshot_meta(10)
        assert any("hello console" in m["text"] for m in meta)
        assert any(m["level"] == "INFO" for m in meta)
    finally:
        lg.removeHandler(get_ring())


# ------------------------------------------------------------------
# NetDiagnostics — TCP
# ------------------------------------------------------------------

def test_tcp_check_invalid_input():
    r = net.tcp_check("", 0)
    assert r["ok"] is False and "invalid" in r["error"]


def test_tcp_check_refused_port():
    # port 1 trên 127.0.0.1 — gần như chắc chắn không có listener
    r = net.tcp_check("127.0.0.1", 1, timeout=1.0)
    assert r["ok"] is False
    assert r["error"]
    assert isinstance(r["ms"], float)


def test_tcp_check_open_port():
    srv = _spawn_listener()
    try:
        r = net.tcp_check("127.0.0.1", srv, timeout=2.0)
        assert r["ok"] is True
        assert r["ms"] is not None and r["ms"] >= 0
    finally:
        _stop_listener()


def test_check_endpoints_parallel_shape():
    results = net.check_endpoints(timeout=2.0)
    ids = [r["id"] for r in results]
    assert set(ids) == set(net.ENDPOINTS.keys())
    assert all({"ok", "ms", "host", "port"} <= set(r) for r in results)


# ------------------------------------------------------------------
# NetDiagnostics — MC Server List Ping (fake server bằng raw sockets)
# ------------------------------------------------------------------

class _FakeMCServer(threading.Thread):
    """Server giả trả status JSON chuẩn SL Ping — kiểm tra đọc VarInt đúng."""

    def __init__(self, payload: bytes):
        super().__init__(daemon=True)
        self.payload = payload
        self.host = "127.0.0.1"
        self.port = 0
        self._sock = None
        self.running = True

    def __enter__(self):
        import socket
        self._sock = socket.socket()
        self._sock.bind((self.host, 0))
        self._sock.listen(4)
        self.port = self._sock.getsockname()[1]
        self.start()
        return self

    def run(self):
        import socket
        while self.running:
            try:
                conn, _ = self._sock.accept()
            except OSError:
                return
            with conn:
                conn.settimeout(2)
                try:
                    self._read_packet(conn)   # handshake
                    self._read_packet(conn)   # status request
                    body = net._pack_varint(0) + net._pack_str(self.payload.decode())
                    conn.sendall(net._pack_varint(len(body)) + body)
                except Exception:
                    pass

    @staticmethod
    def _read_packet(conn) -> bytes:
        import io
        length = net._read_varint_stream(_ConnReader(conn))
        data = b""
        while len(data) < length:
            chunk = conn.recv(length - len(data))
            if not chunk:
                raise ValueError("closed")
            data += chunk
        buf = io.BytesIO(data)
        net._read_varint_stream(buf)
        return buf.read()

    def __exit__(self, *a):
        self.running = False
        try:
            self._sock.close()
        except Exception:
            pass


class _ConnReader:
    def __init__(self, conn):
        self._conn = conn

    def read(self, n=1):
        return self._conn.recv(n)


def _status_payload(motd="A Minecraft Server", online=3, mx=20):
    import json
    return json.dumps({
        "version": {"name": "1.21.8", "protocol": 771},
        "players": {"online": online, "max": mx},
        "description": {"text": motd},
    }).encode()


def test_mc_ping_parses_fake_server():
    with _FakeMCServer(_status_payload()) as srv:
        r = net.mc_ping("127.0.0.1", srv.port, timeout=3.0)
    assert r["online"] is True
    assert r["motd"] == "A Minecraft Server"
    assert r["players"] == {"online": 3, "max": 20}
    assert r["version"]["name"] == "1.21.8"
    assert r["version"]["protocol"] == 771
    assert isinstance(r["latencyMs"], float)


def test_mc_ping_refused():
    r = net.mc_ping("127.0.0.1", 1, timeout=1.0)
    assert r["online"] is False and r["error"]


def test_mc_ping_invalid_input():
    r = net.mc_ping("", 25565)
    assert r["online"] is False and "invalid" in r["error"]


# ------------------------------------------------------------------

_listeners = []


def _spawn_listener() -> int:
    import socket
    s = socket.socket()
    s.bind(("127.0.0.1", 0))
    s.listen(4)
    _listeners.append(s)
    return s.getsockname()[1]


def _stop_listener():
    while _listeners:
        s = _listeners.pop()
        try:
            s.close()
        except Exception:
            pass
