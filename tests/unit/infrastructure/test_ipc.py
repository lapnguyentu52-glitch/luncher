"""IpcServer — token xoay mỗi lần start, auth, validate packet (mục 12, 13, 74)."""
from __future__ import annotations

import json
import time
import urllib.request

from infrastructure.ipc.server import IpcServer, PACKET_TYPES


def _post(url: str, token: str | None, body: bytes) -> tuple[int, dict]:
    req = urllib.request.Request(url, data=body, method="POST")
    if token is not None:
        req.add_header("X-Antares-Token", token)
    try:
        with urllib.request.urlopen(req, timeout=2) as resp:
            return resp.status, json.loads(resp.read().decode("utf-8"))
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read().decode("utf-8"))


def _packet(ptype: str = "runtime.hello", payload: dict | None = None) -> bytes:
    return json.dumps({
        "version": 1, "type": ptype, "timestamp": time.time(),
        "payload": payload or {"sessionId": "s1"},
    }).encode("utf-8")


def test_token_rotates_each_start():
    seen: list[dict] = []
    srv = IpcServer(lambda p, c: seen.append(p))
    try:
        url1 = srv.start()
        tok1 = srv.endpoint()["token"]
        srv.stop()

        url2 = srv.start()
        tok2 = srv.endpoint()["token"]
        assert tok2 != tok1, "token phải xoay mỗi lần start"
        assert url2.startswith("http://127.0.0.1:")
    finally:
        srv.stop()


def test_stale_token_rejected_after_restart():
    srv = IpcServer(lambda p, c: None)
    try:
        srv.start()
        old_token = srv.endpoint()["token"]
        srv.stop()

        url = srv.start()          # port có thể được OS tái dùng
        code, _ = _post(url, old_token, _packet())
        assert code == 401, "token phiên trước phải bị từ chối (401)"

        code, body = _post(url, srv.endpoint()["token"], _packet())
        assert code == 200 and body.get("ok") is True
    finally:
        srv.stop()


def test_packet_validation_ladder():
    seen: list[dict] = []
    srv = IpcServer(lambda p, c: seen.append(p))
    try:
        url = srv.start()
        tok = srv.endpoint()["token"]

        # sai đường dẫn → 404
        bad_url = url.replace("/packet", "/other")
        assert _post(bad_url, tok, _packet())[0] == 404

        # sai token → 401
        assert _post(url, "wrong-token", _packet())[0] == 401
        assert _post(url, None, _packet())[0] == 401

        # body quá lớn → 413
        assert _post(url, tok, b"x" * (64 * 1024 + 1))[0] == 413

        # json hỏng → 400
        assert _post(url, tok, b"{not-json")[0] == 400

        # sai version/type/payload → 422
        assert _post(url, tok, json.dumps(
            {"version": 2, "type": "runtime.hello", "payload": {}}).encode())[0] == 422
        assert _post(url, tok, json.dumps(
            {"version": 1, "type": "nope", "payload": {}}).encode())[0] == 422

        # đúng → 200, callback nhận đủ 6 loại packet hợp lệ
        for ptype in sorted(PACKET_TYPES):
            code, _ = _post(url, tok, _packet(ptype))
            assert code == 200
        assert {p["type"] for p in seen} == set(PACKET_TYPES)
    finally:
        srv.stop()


def test_pairing_file_written_and_guard(ctx, instance_id):
    """write_pairing_for_instance: None khi server dừng; file hợp lệ khi chạy."""
    from services.runtime.service import RuntimeService
    rt = RuntimeService(ctx)
    ctx.set("runtime", rt)

    # server chưa start → không ghi gì
    assert rt.write_pairing_for_instance(instance_id) is None

    rt.start()
    try:
        r = rt.write_pairing_for_instance(instance_id)
        assert r and r["file"]
        data = json.loads(r["file"] and open(r["file"], encoding="utf-8").read())
        assert data["version"] == 1
        assert data["endpoint"].startswith("http://127.0.0.1:")
        assert data["token"] == rt.endpoint()["token"]
        assert len(data["packetTypes"]) == 6
    finally:
        rt.stop()
