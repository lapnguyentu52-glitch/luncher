"""ServerService — server lifecycle (mục 20, 516).

Port từ Spark: tải jar đa nguồn, properties, eula, process, console.
Software được tách thành installer riêng trong services/servers/software/.
"""
from __future__ import annotations

import re
import shutil
import time
import uuid
from pathlib import Path

from app.context import AppContext
from core.errors import codes
from core.errors.base import ServerError
from core.events import names as ev
from core.tasks.manager import Task
from core.utils.paths import is_safe_name
from infrastructure.fs.locks import ResourceLock
from infrastructure.process.manager import ProcessManager

_PLAYER_JOIN = re.compile(r"([A-Za-z0-9_]{3,16}) joined the game")
_PLAYER_LEFT = re.compile(r"([A-Za-z0-9_]{3,16})(?: left the game| lost connection| has disconnected)")

#: Chu kỳ publish SERVER_METRICS (mục 22) — 2s đủ mượt, không đốt CPU.
METRICS_INTERVAL_S = 2.0


class ServerService:
    def __init__(self, ctx: AppContext, processes: ProcessManager) -> None:
        self._ctx = ctx
        self._processes = processes
        self._players: dict[str, set[str]] = {}

    def _dir(self, server_id: str) -> Path:
        return self._ctx.paths.servers / server_id

    def list(self) -> list[dict]:
        out = []
        for p in self._ctx.paths.servers.iterdir() if self._ctx.paths.servers.exists() else ():
            meta = p / "server_meta.json"
            if p.is_dir() and meta.exists():
                import json
                try:
                    item = json.loads(meta.read_text(encoding="utf-8"))
                except Exception:
                    continue
                # Badge running là state BACKEND (is_running) — frontend không tự đoán,
                # đúng sau restart app / khi server được start từ nơi khác.
                item["running"] = self._processes.is_running(f"server:{item.get('id')}")
                out.append(item)
        return out

    def create(self, name: str, software: str, version: str, *,
               ram_mb: int = 2048, port: int = 25565,
               max_players: int = 20, online_mode: bool = True,
               jar_source: str | None = None) -> dict:
        if not is_safe_name(name):
            raise ServerError(codes.INSTANCE_NAME_INVALID, f"Invalid server name: {name}")
        server_id = uuid.uuid4().hex[:12]
        meta = {
            "id": server_id,
            "name": name,
            "software": software,
            "version": version,
            "ramMb": ram_mb,
            "port": port,
            "maxPlayers": max_players,
            "onlineMode": online_mode,
            "jarSource": jar_source,
        }
        from core.config.writer import write_json_atomic
        self._dir(server_id).mkdir(parents=True, exist_ok=True)
        write_json_atomic(self._dir(server_id) / "server_meta.json", meta)
        return meta

    def install(self, server_id: str, task: Task) -> None:
        """Tải + verify server jar theo software (mục 63 — chỉ ready sau verify)."""
        meta = self._get_meta(server_id)
        from services.servers.software.installer import install_server_jar
        install_server_jar(self._ctx, meta, task)
        self._write_props(server_id)
        task.message = "Server installed"

    def start(self, server_id: str, task: Task) -> dict:
        meta = self._get_meta(server_id)
        d = self._dir(server_id)
        jar = d / "server.jar"
        if not jar.exists():
            task.message = "Installing server..."
            self.install(server_id, task)

        self._write_props(server_id)
        eula = d / "eula.txt"
        if not eula.exists():
            # Mục 65: EULA phải minh bạch — ghi kèm note trong console
            eula.write_text("eula=true\n", encoding="utf-8")
            self._console(server_id, "eula.txt accepted (Antares default)")

        ram = int(meta.get("ramMb", 2048))
        cmd = ["java", f"-Xmx{ram}M", f"-Xms{max(512, ram // 2)}M"]
        jvm = meta.get("jvm") or ""
        if jvm:
            cmd += str(jvm).split()
        cmd += ["-jar", "server.jar", "nogui"]

        self._players[server_id] = set()
        self._ctx.events.publish(ev.SERVER_STARTING, {"serverId": server_id})
        proc = self._processes.spawn(f"server:{server_id}", cmd, cwd=str(d),
                                     on_line=lambda line: self._on_line(server_id, line),
                                     on_exit=lambda code: self._on_exit(server_id, code))
        self._start_metrics_loop(server_id, proc.pid)
        return {"pid": proc.pid}

    def stop(self, server_id: str) -> None:
        """Graceful stop: stdin 'stop' -> wait -> kill (mục 444)."""
        self._processes.stop(f"server:{server_id}", graceful_stdin="stop")

    def send_command(self, server_id: str, command: str) -> None:
        """Chỉ gửi text line qua stdin — không shell (mục 445)."""
        self._processes.send_stdin(f"server:{server_id}", command + "\n")

    def is_running(self, server_id: str) -> bool:
        return self._processes.is_running(f"server:{server_id}")

    def players(self, server_id: str) -> list[str]:
        return sorted(self._players.get(server_id, set()))

    def open_folder(self, server_id: str) -> str:
        return str(self._dir(server_id))

    def delete(self, server_id: str, *, confirm: bool = False) -> bool:
        if not confirm:
            raise ServerError(codes.VALIDATION_FAILED, "Delete requires confirmation")
        if self.is_running(server_id):
            raise ServerError(codes.SERVER_START_FAILED, "Stop server before deleting")
        shutil.rmtree(self._dir(server_id), ignore_errors=True)
        return True

    # -- internals --
    def _start_metrics_loop(self, server_id: str, pid: int | None) -> None:
        """Thread publish SERVER_METRICS định kỳ (mục 22) đến khi process tắt.

        EventBridge xếp SERVER_METRICS vào nhóm 'latest' (chỉ giữ bản mới nhất
        theo serverId) nên dù publish mỗi 2s, UI chỉ nhận 1 gói mỗi flush —
        không được nhấn chìm kênh event.
        """
        import threading

        def loop() -> None:
            import psutil

            try:
                proc = psutil.Process(pid) if pid else None
            except Exception:
                proc = None
            while self._processes.is_running(f"server:{server_id}"):
                payload: dict = {"serverId": server_id, "players": self.players(server_id)}
                if proc is not None:
                    try:
                        payload["cpuPercent"] = round(proc.cpu_percent(interval=None), 1)
                        mem = proc.memory_info().rss
                        payload["memoryMb"] = mem >> 20
                        ram = int(self._get_meta(server_id).get("ramMb", 2048))
                        payload["memoryPercent"] = round(mem / max(1, ram << 20) * 100, 1)
                    except Exception:
                        pass                      # process chết giữa chừng — bỏ qua nhịp này
                try:
                    self._ctx.events.publish(ev.SERVER_METRICS, payload)
                except Exception:
                    pass
                time.sleep(METRICS_INTERVAL_S)

        threading.Thread(target=loop, name=f"srv-metrics-{server_id}",
                         daemon=True).start()

    def _console(self, server_id: str, text: str) -> None:
        self._ctx.events.publish(ev.SERVER_OUTPUT,
                                 {"serverId": server_id, "line": f"[Antares] {text}"})
    def _get_meta(self, server_id: str) -> dict:
        import json
        meta = self._dir(server_id) / "server_meta.json"
        if not meta.exists():
            raise ServerError(codes.INSTANCE_NOT_FOUND, f"Server '{server_id}' not found")
        return json.loads(meta.read_text(encoding="utf-8"))

    def _on_line(self, server_id: str, line: str) -> None:
        self._ctx.events.publish(ev.SERVER_OUTPUT, {"serverId": server_id, "line": line})
        if "Done (" in line:
            self._ctx.events.publish(ev.SERVER_STARTED, {"serverId": server_id})
        players = self._players.setdefault(server_id, set())
        m = _PLAYER_JOIN.search(line)
        if m:
            players.add(m.group(1))
        m = _PLAYER_LEFT.search(line)
        if m:
            players.discard(m.group(1))

    def _on_exit(self, server_id: str, code: int) -> None:
        self._players.pop(server_id, None)
        self._ctx.events.publish(ev.SERVER_STOPPED,
                                 {"serverId": server_id, "exitCode": code})
        if code != 0:
            # UI nghe server.failed để xoá badge running + báo lỗi (mục 15.2)
            self._ctx.events.publish(ev.SERVER_FAILED,
                                     {"serverId": server_id, "exitCode": code})

    def _write_props(self, server_id: str) -> None:
        """Sửa server.properties giữ lại key lạ (port từ Spark _srv_write_props)."""
        meta = self._get_meta(server_id)
        p = self._dir(server_id) / "server.properties"
        managed = {
            "online-mode": "true" if meta.get("onlineMode", True) else "false",
            "max-players": str(meta.get("maxPlayers", 20)),
            "server-port": str(meta.get("port", 25565)),
            "motd": f"{meta.get('name', 'Antares')} — Antares server",
        }
        lines: list[str] = []
        if p.exists():
            lines = p.read_text(encoding="utf-8", errors="replace").splitlines()
        seen: set[str] = set()
        out: list[str] = []
        for ln in lines:
            key = ln.split("=", 1)[0] if "=" in ln else None
            if key in managed:
                out.append(f"{key}={managed[key]}")
                seen.add(key)
            else:
                out.append(ln)
        for k, v in managed.items():
            if k not in seen:
                out.append(f"{k}={v}")
        p.write_text("\n".join(out) + "\n", encoding="utf-8")
