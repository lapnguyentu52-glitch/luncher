"""RuntimeService — companion sessions + game telemetry (spec 3.0 mục 12).

Trách nhiệm launcher (đúng mục 12): nhận telemetry từ companion qua IPC,
coalesce latest, đẩy event xuống UI, giữ history ngắn trong RAM.
KHÔNG làm HUD/crosshair runtime — đó là việc của companion mod.
"""
from __future__ import annotations

import threading
import time
from pathlib import Path

from core.config.writer import write_json_atomic
from core.events import names as ev
from core.logging.setup import get_logger
from infrastructure.ipc.server import IpcServer

logger = get_logger("runtime")

MAX_METRIC_POINTS = 600      # ~5 phút ở 2Hz, ring RAM (mục 62: memory only)


class RuntimeService:
    def __init__(self, ctx) -> None:
        self._ctx = ctx
        self._lock = threading.Lock()
        self._sessions: dict[str, dict] = {}       # sessionId -> info
        self._metrics: dict[str, list[dict]] = {}  # sessionId -> history
        self._server = IpcServer(self._on_packet)

    # ------------------------------------------------------------------
    # Vòng đời
    # ------------------------------------------------------------------

    def start(self) -> dict:
        url = self._server.start()
        self._ctx.set("ipc_endpoint", self._server.endpoint())
        return {"url": url}

    def stop(self) -> None:
        self._server.stop()

    def endpoint(self) -> dict:
        return self._server.endpoint()

    # ------------------------------------------------------------------
    # Packet ingest (thread IPC server)
    # ------------------------------------------------------------------

    def _on_packet(self, packet: dict, client: str) -> None:
        try:
            self._handle(packet, client)
        except Exception:
            logger.exception("packet handle failed")

    def _handle(self, packet: dict, client: str) -> None:
        ptype = packet["type"]
        payload = packet.get("payload") or {}
        ts = packet.get("timestamp") or time.time()

        if ptype == "runtime.hello":
            sid = str(payload.get("sessionId") or f"sess-{int(ts)}")
            with self._lock:
                self._sessions[sid] = {
                    "sessionId": sid, "client": client,
                    "instanceId": payload.get("instanceId"),
                    "connectedAt": ts, "lastSeen": ts,
                }
                self._metrics.setdefault(sid, [])
            self._ctx.events.publish(ev.RUNTIME_CONNECTED, {"sessionId": sid})
            return

        if ptype == "runtime.bye":
            sid = str(payload.get("sessionId") or "")
            with self._lock:
                self._sessions.pop(sid, None)
                self._metrics.pop(sid, None)
            self._ctx.events.publish(ev.RUNTIME_DISCONNECTED, {"sessionId": sid})
            return

        sid = str(payload.get("sessionId") or "")
        with self._lock:
            sess = self._sessions.get(sid)
            if sess is None:
                # packet trước hello — bỏ (companion phải handshake trước)
                return
            sess["lastSeen"] = ts

        event_name = {
            "runtime.performance": ev.RUNTIME_PERFORMANCE,
            "runtime.game_state": ev.RUNTIME_GAME_STATE,
            "runtime.chat": ev.RUNTIME_CHAT,
            "runtime.error": ev.RUNTIME_ERROR,
        }.get(ptype)
        if event_name:
            self._ctx.events.publish(event_name, {**payload, "sessionId": sid})

        if ptype == "runtime.performance":
            point = {"ts": ts,
                     "fps": payload.get("fps"),
                     "frameMs": payload.get("frameMs"),
                     "low1": payload.get("low1")}
            with self._lock:
                hist = self._metrics.setdefault(sid, [])
                hist.append(point)
                if len(hist) > MAX_METRIC_POINTS:
                    del hist[:len(hist) - MAX_METRIC_POINTS]

    # ------------------------------------------------------------------
    # API cho bridge
    # ------------------------------------------------------------------

    def sessions(self) -> list[dict]:
        with self._lock:
            return [dict(s) for s in self._sessions.values()]

    def metrics(self, session_id: str | None = None) -> dict:
        with self._lock:
            if session_id and session_id in self._metrics:
                return {"sessions": {session_id: list(self._metrics[session_id])}}
            return {"sessions": {k: list(v) for k, v in self._metrics.items()}}

    # ------------------------------------------------------------------
    # Auto-pairing: ghi endpoint+token vào instance khi launch (mục 12)
    # Companion mod đọc file này lúc khởi tạo — không cần nhập tay.
    # ------------------------------------------------------------------

    def write_pairing_for_instance(self, instance_id: str) -> dict | None:
        """Ghi `companion.json` vào game dir nếu server IPC đang chạy."""
        if not self._server.url:
            return None
        ep = self._server.endpoint()
        game_dir = Path(self._ctx.paths.instances) / instance_id / "game"
        try:
            game_dir.mkdir(parents=True, exist_ok=True)
            write_json_atomic(game_dir / "companion.json", {
                "version": 1,
                "endpoint": ep["url"],
                "token": ep["token"],
                "packetTypes": ep["packetTypes"],
                "writtenAt": time.time(),
            })
            logger.info("Pairing written for instance %s", instance_id)
            return {"file": str(game_dir / "companion.json")}
        except Exception:
            logger.exception("write pairing failed for %s", instance_id)
            return None
