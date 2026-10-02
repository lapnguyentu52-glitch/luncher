"""Spawn sidecar THẬT (không mock) — bắt regression bootstrap của _legacy_ctx (#3.1).

Trước khi vá, `_legacy_ctx()` gọi `AppContext()` thiếu 4 positional args →
mọi method ngoài `health.*` trả `LEGACY_UNAVAILABLE`. Toàn bộ
`tests/unit/legacy/test_sidecar_*.py` đều mock `_legacy_ctx`, nên suite vẫn
xanh — 3 test này không mock gì, spawn `legacy/python/sidecar.py` thật và
gọi qua stdin/stdout như launcher (Rust) làm.
"""
from __future__ import annotations

import json
import os
import queue
import subprocess
import sys
import threading
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parent.parent.parent.parent
SIDECAR = REPO_ROOT / "legacy" / "python" / "sidecar.py"

READ_TIMEOUT_S = 60.0  # bootstrap nạp ~ toàn bộ services — cho nới hơi tay


class _Sidecar:
    """Questa mini-client: 1 request JSON/line, đọc response theo id."""

    def __init__(self, data_dir: Path) -> None:
        env = os.environ.copy()
        env["ANTARES_ROOT"] = str(REPO_ROOT)
        env["ANTARES_DATA_DIR"] = str(data_dir)
        self._proc = subprocess.Popen(
            [sys.executable, str(SIDECAR)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            env=env,
            cwd=str(REPO_ROOT),
        )
        self._lines: queue.Queue[str | None] = queue.Queue()
        self._stderr: list[str] = []
        self._next_id = 0
        threading.Thread(target=self._pump_stdout, daemon=True).start()
        threading.Thread(target=self._pump_stderr, daemon=True).start()

    def _pump_stdout(self) -> None:
        assert self._proc.stdout is not None
        for line in self._proc.stdout:
            self._lines.put(line)
        self._lines.put(None)  # EOF

    def _pump_stderr(self) -> None:
        assert self._proc.stderr is not None
        for line in self._proc.stderr:
            self._stderr.append(line)

    def call(self, method: str, params: dict | None = None) -> dict:
        self._next_id += 1
        req_id = f"real-{self._next_id}"
        assert self._proc.stdin is not None
        self._proc.stdin.write(
            json.dumps({"id": req_id, "method": method, "params": params or {}}) + "\n"
        )
        self._proc.stdin.flush()
        # Bỏ qua notification (sidecar.started, instances.changed, ...)
        deadline_items: list[str] = []
        while True:
            try:
                line = self._lines.get(timeout=READ_TIMEOUT_S)
            except queue.Empty:
                tail = "".join(self._stderr[-20:])
                raise AssertionError(
                    f"sidecar không trả lời {method} trong {READ_TIMEOUT_S}s;\n"
                    f"stderr:\n{tail}"
                ) from None
            if line is None:
                tail = "".join(self._stderr[-20:])
                raise AssertionError(
                    f"sidecar đã chết trước khi trả lời {method};\nstderr:\n{tail}"
                )
            line = line.strip()
            if not line:
                continue
            try:
                msg = json.loads(line)
            except json.JSONDecodeError:
                continue
            if msg.get("id") == req_id:
                return msg
            deadline_items.append(line)

    def close(self) -> None:
        try:
            if self._proc.stdin is not None:
                self._proc.stdin.close()
        except Exception:  # noqa: BLE001 — teardown không được làm fail test
            pass
        try:
            self._proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self._proc.terminate()
            try:
                self._proc.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self._proc.kill()


@pytest.fixture(scope="module")
def sidecar(tmp_path_factory: pytest.TempPathFactory):
    data_dir = tmp_path_factory.mktemp("antares-data")
    proc = _Sidecar(data_dir)
    try:
        yield proc
    finally:
        proc.close()


class TestRealBootstrap:
    def test_health_version_reports_legacy_available(self, sidecar: _Sidecar) -> None:
        """Bootstrap thật chạy được → legacyAvailable=true (trước vá: false)."""
        resp = sidecar.call("health.version")
        assert resp["ok"] is True, resp
        assert resp["data"]["legacyAvailable"] is True, resp
        assert resp["data"]["service"] == "antares-legacy"

    def test_instances_list_uses_registered_service(self, sidecar: _Sidecar) -> None:
        """ctx.get(\"instances\") phải là InstanceService thật, không phải None.

        Data dir trống → list() = []. Trước vá: LEGACY_UNAVAILABLE
        (AppContext.__init__ lỗi) hoặc LEGACY_INTERNAL (None.list()).
        """
        resp = sidecar.call("instances.list")
        assert resp["ok"] is True, resp
        assert resp["data"]["instances"] == []

    def test_mods_list_unknown_instance_returns_instance_not_found(
        self, sidecar: _Sidecar
    ) -> None:
        """Lỗi nghiệp vụ đúng — không còn lẫn LEGACY_UNAVAILABLE."""
        resp = sidecar.call("mods.list", {"instanceId": "no-such-instance"})
        assert resp["ok"] is False, resp
        assert resp["error"]["code"] == "INSTANCE_NOT_FOUND", resp
