"""NetDiagnostics — TCP check + Minecraft Server List Ping (mục 42).

mc_ping: handshake status query, đọc VarInt properly — hiện MOTD, players,
latency. Phục vụ: kiểm tra server trong profile trước khi Play, điều tra
connection refused/timeout trong Console insights.
"""
from __future__ import annotations

import socket
import struct
import time

from core.logging.setup import get_logger

logger = get_logger("diagnostics.net")

#: Mục đích kiểm tra kết nối launcher — host:port cần cho luồng chính.
ENDPOINTS = {
    "mojang": ("launchermeta.mojang.com", 443),
    "mojang_session": ("sessionserver.mojang.com", 443),
    "ely_auth": ("authserver.ely.by", 443),
    "modrinth": ("api.modrinth.com", 443),
    "curseforge": ("api.curseforge.com", 443),
}

DEFAULT_TIMEOUT = 4.0


def tcp_check(host: str, port: int, *, timeout: float = DEFAULT_TIMEOUT) -> dict:
    """1 kết nối TCP + đo RTT (ms). Không gửi dữ liệu, đóng ngay."""
    host = str(host or "").strip()
    port = int(port)
    if not host or not (0 < port < 65536):
        return {"host": host, "port": port, "ok": False,
                "error": "invalid host/port", "ms": None}
    started = time.perf_counter()
    try:
        with socket.create_connection((host, port), timeout=timeout):
            ms = round((time.perf_counter() - started) * 1000, 1)
            return {"host": host, "port": port, "ok": True, "ms": ms, "error": None}
    except OSError as e:
        ms = round((time.perf_counter() - started) * 1000, 1)
        return {"host": host, "port": port, "ok": False, "ms": ms,
                "error": _short_err(e)}


def check_endpoints(*, timeout: float = DEFAULT_TIMEOUT) -> list[dict]:
    """Kiểm tra song song các endpoint launcher cần (mục 42: đa luồng)."""
    import concurrent.futures
    results = []
    with concurrent.futures.ThreadPoolExecutor(max_workers=len(ENDPOINTS)) as pool:
        futures = {pool.submit(tcp_check, h, p, timeout=timeout): key
                   for key, (h, p) in ENDPOINTS.items()}
        for fut in concurrent.futures.as_completed(futures):
            key = futures[fut]
            r = fut.result()
            r["id"] = key
            results.append(r)
    # Thứ tự ổn định theo tên endpoint
    results.sort(key=lambda r: r["id"])
    return results


def mc_ping(host: str, port: int = 25565, *, timeout: float = DEFAULT_TIMEOUT) -> dict:
    """Server List Ping hiện đại (47+): handshake -> status request -> response.

    Trả: motd, players (online/max), version (tên + protocol), latency ms,
    favicon (data URI base64, có thể rỗng), modinfo (neoforge/forge).
    """
    host = str(host or "").strip()
    port = int(port)
    if not host or not (0 < port < 65536):
        return {"host": host, "port": port, "online": False,
                "error": "invalid host/port"}

    started = time.perf_counter()
    sock = None
    try:
        sock = socket.create_connection((host, port), timeout=timeout)
        latency_connect = (time.perf_counter() - started) * 1000
        sock.settimeout(timeout)

        # Handshake packet: id=0x00, protocol=-1 (status), host, port, next=1
        payload = _pack_varint(0) + _pack_varint(-1) + _pack_str(host) + \
            struct.pack(">H", port) + _pack_varint(1)
        _send_packet(sock, payload)

        # Status request: id=0x00, empty body
        _send_packet(sock, _pack_varint(0))

        # Response: packet id 0x00 + JSON string
        resp_id, body = _recv_packet(sock)
        if resp_id != 0:
            return {"host": host, "port": port, "online": False,
                    "error": f"unexpected packet id {resp_id}"}
        json_len = _read_varint_stream(body)
        raw = body.read(json_len)
        import json
        data = json.loads(raw.decode("utf-8", errors="replace"))

        ms = round((time.perf_counter() - started) * 1000, 1)
        desc = data.get("description", {})
        motd = desc.get("text") if isinstance(desc, dict) else str(desc)
        if not motd and isinstance(desc, dict):
            # Chat component dạng {"extra":[...]} — nối text các phần.
            extra = desc.get("extra") or []
            motd = "".join(str(x.get("text", "")) if isinstance(x, dict) else str(x)
                           for x in extra)
        players = data.get("players", {}) or {}
        version = data.get("version", {}) or {}
        return {
            "host": host, "port": port, "online": True,
            "motd": motd or "",
            "players": {"online": int(players.get("online", 0)),
                        "max": int(players.get("max", 0))},
            "version": {"name": str(version.get("name", "?")),
                        "protocol": int(version.get("protocol", -1))},
            "latencyMs": ms,
            "connectMs": round(latency_connect, 1),
            "favicon": bool(data.get("favicon")),
            "modinfo": data.get("modinfo"),
        }
    except OSError as e:
        return {"host": host, "port": port, "online": False, "error": _short_err(e)}
    except Exception as e:
        return {"host": host, "port": port, "online": False, "error": str(e)[:160]}
    finally:
        if sock is not None:
            try:
                sock.close()
            except Exception:
                pass


# ------------------------------------------------------------------
# VarInt / packet primitives — chuẩn Server List Ping
# ------------------------------------------------------------------

def _pack_varint(value: int) -> bytes:
    v = value & 0xFFFFFFFF
    out = bytearray()
    while True:
        b = v & 0x7F
        v >>= 7
        if v:
            out.append(b | 0x80)
        else:
            out.append(b)
            return bytes(out)


def _read_varint_stream(stream) -> int:
    """Đọc VarInt từ stream (bytes buffer) — hỗ trợ số 5-byte chuẩn."""
    num = 0
    for i in range(5):
        byte = stream.read(1)
        if not byte:
            raise ValueError("EOF while reading VarInt")
        b = byte[0]
        num |= (b & 0x7F) << (7 * i)
        if not (b & 0x80):
            return num
    raise ValueError("VarInt too long")


def _pack_str(s: str) -> bytes:
    raw = s.encode("utf-8")
    return _pack_varint(len(raw)) + raw


def _send_packet(sock, payload: bytes) -> None:
    sock.sendall(_pack_varint(len(payload)) + payload)


def _recv_packet(sock) -> tuple[int, bytes]:
    """Đọc 1 packet: length varint + id varint + body. Trả (id, body stream)."""
    length = _read_varint_stream(_SockReader(sock))
    data = _recv_exact(sock, length)
    import io
    buf = io.BytesIO(data)
    pid = _read_varint_stream(buf)
    return pid, buf


def _recv_exact(sock, n: int) -> bytes:
    chunks = bytearray()
    while len(chunks) < n:
        chunk = sock.recv(min(65536, n - len(chunks)))
        if not chunk:
            raise ValueError("connection closed mid-packet")
        chunks.extend(chunk)
    return bytes(chunks)


class _SockReader:
    """Wrapper đọc 1 byte cho VarInt đầu packet (dùng recv trực tiếp)."""

    def __init__(self, sock) -> None:
        self._sock = sock

    def read(self, n: int = 1) -> bytes:
        return self._sock.recv(n)


def _short_err(e: Exception) -> str:
    msg = str(e) or e.__class__.__name__
    return msg[:160]
