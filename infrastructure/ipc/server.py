"""IPC server — kênh Companion Minecraft → Launcher (spec 3.0 mục 12, 13).

Lựa chọn kỹ thuật (mục 13): loopback 127.0.0.1, token ngẫu nhiên mỗi phiên.
- Primary: HTTP POST /packet  — stdlib thuần, không dependency mới, đủ cho
  telemetry 2–4 packet/giây (companion không cần stream 60Hz lên launcher).
- Packet model đúng mục 13: {"version":1, "type": "...", "timestamp":0, "payload":{}}.
- Auth: header `X-Antares-Token` so khớp token sinh lúc start; bind 127.0.0.1
  port 0 (OS chọn trống) — không mở ra LAN.

Giữ đơn giản có chủ đích: WebSocket upgrade chỉ khi cần stream dày (giai đoạn
companion mod thật) — giao diện IpcServer không đổi.
"""
from __future__ import annotations

import json
import secrets
import threading
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

from core.logging.setup import get_logger

logger = get_logger("ipc")

PACKET_TYPES = {
    "runtime.hello", "runtime.bye",
    "runtime.performance",      # fps, frametime avg/min/1%low, tick time
    "runtime.game_state",       # screen, world, player pos/health (nếu companion cấp)
    "runtime.chat", "runtime.error",
}

MAX_BODY = 64 * 1024          # packet nhỏ — chặn body khổng lồ (mục 77 spirit)


class IpcServer:
    """HTTP server loopback nhận packet từ companion, đẩy qua callback."""

    def __init__(self, on_packet) -> None:
        self._on_packet = on_packet      # callable(packet: dict, client: str)
        self._token = secrets.token_urlsafe(24)
        self._httpd: ThreadingHTTPServer | None = None
        self._thread: threading.Thread | None = None
        self.url: str | None = None

    # ------------------------------------------------------------------

    def start(self) -> str:
        # Xoay token MỖI lần start — pairing file từ phiên/ lần chạy trước
        # luôn vô hiệu (401) dù port có bị OS tái dùng.
        self._token = secrets.token_urlsafe(24)
        server = self

        class Handler(BaseHTTPRequestHandler):
            def do_POST(self):  # noqa: N802
                if self.path != "/packet":
                    self._reply(404, {"error": "not found"})
                    return
                if self.headers.get("X-Antares-Token") != server._token:
                    self._reply(401, {"error": "unauthorized"})
                    return
                length = int(self.headers.get("Content-Length") or 0)
                if length <= 0 or length > MAX_BODY:
                    self._reply(413, {"error": "bad size"})
                    return
                try:
                    packet = json.loads(self.rfile.read(length).decode("utf-8"))
                except Exception:
                    self._reply(400, {"error": "bad json"})
                    return
                if not server._valid(packet):
                    self._reply(422, {"error": "bad packet"})
                    return
                server._on_packet(packet, self.client_address[0])
                self._reply(200, {"ok": True})

            def _reply(self, code: int, body: dict) -> None:
                data = json.dumps(body).encode("utf-8")
                self.send_response(code)
                self.send_header("Content-Type", "application/json")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def log_message(self, fmt, *args):  # im lặng
                return

        self._httpd = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
        port = self._httpd.server_address[1]
        self.url = f"http://127.0.0.1:{port}/packet"
        self._thread = threading.Thread(target=self._httpd.serve_forever,
                                        name="antares-ipc", daemon=True)
        self._thread.start()
        logger.info("IPC listening on %s (token %s...)", self.url, self._token[:6])
        return self.url

    def stop(self) -> None:
        if self._httpd:
            self._httpd.shutdown()
            self._httpd.server_close()
        self._httpd = None
        self._thread = None
        self.url = None

    # ------------------------------------------------------------------

    def endpoint(self) -> dict:
        """Thông tin endpoint cho UI/companion setup."""
        return {"url": self.url, "token": self._token,
                "packetTypes": sorted(PACKET_TYPES)}

    @staticmethod
    def _valid(packet: dict) -> bool:
        if not isinstance(packet, dict):
            return False
        if packet.get("version") != 1:
            return False
        if packet.get("type") not in PACKET_TYPES:
            return False
        return isinstance(packet.get("payload"), dict)
