# Legacy Bridge — Milestone 5 (§43, §234)

Cầu chuyển đổi giữa Rust core và Python legacy services. Python chỉ tồn tại
trong migration — bridge để các service cũ vẫn hoạt động khi UI đã Tauri.

## Thành phần

```text
crates/antares-bridge/     Rust: spawn/handshake/timeout/restart/shutdown/crash detection
legacy/python/sidecar.py   Python: JSON Lines stdio server (stdlib-only)
src-tauri/commands/legacy.rs  Commands: legacy_status/start/call/restart/shutdown
apps/desktop/src/services/legacyCommands.ts  Typed TS helpers
```

## Protocol (JSON Lines trên stdio)

```json
→ {"id": "req-1", "method": "app.echo", "params": {"x": 1}}
← {"id": "req-1", "ok": true, "data": {"x": 1}}
← {"id": "req-1", "ok": false, "error": {"code": "METHOD_NOT_FOUND", "message": "..."}}
← {"event": "sidecar.started", "payload": {...}}     // notification, không id
```

Rules (§31/§43):
- Oversized line (> 8 MB) → reject dòng, process sống
- Line không parse được JSON → bỏ qua an toàn
- Request không có `id` → bỏ qua
- Handler crash → `LEGACY_INTERNAL` error envelope, không chết process

## Health / Handshake (§234)

```text
Launcher spawn sidecar
  → legacy_call("health.version")  [timeout 10s]
  ← {"protocol": 1, "service": "antares-legacy", "serviceVersion": "...", "python": "..."}
  protocol != 1 → kill + status Failed
  protocol == 1 → status Ready
```

- **Timeout**: mỗi call có timeout (default 10s, custom qua `call_with_timeout`);
  timeout → `IPC_TIMEOUT` (retryable, action RETRY)
- **Restart**: kill cũ → spawn mới → handshake lại (`legacy_restart`)
- **Graceful shutdown**: gửi `health.shutdown`, đợi exit trong grace 3s, kill nếu vượt (`legacy_shutdown`)
- **Crash detection**: `check_health()` dùng `try_wait` — process exit → status Failed,
  crash_count++, mọi pending call nhận `IPC_SESSION_STALE`
- **Dev config**: env `ANTARES_LEGACY_SIDECAR` = command chạy sidecar
  (vd `python3 legacy/python/sidecar.py`). Sau bundle sẽ là Tauri `externalBin`.

## Thêm method adapter mới

1. `legacy/python/sidecar.py`: thêm handler `handle_<domain>_<method>` + entry `HANDLERS`
2. Thêm pytest trong `tests/unit/legacy/test_sidecar_protocol.py`
3. TS: gọi qua `callLegacy<T>('domain.method', params)` — type hoá response nếu dùng nhiều

Quy tắc: sidecar **không** nói chuyện trực tiếp với UI; mọi path qua Rust bridge
(envelope §96 áp dụng trên cả bridge errors).

## Tests

```text
pytest tests/unit/legacy/            — 12 tests protocol sidecar
cargo test -p antares-bridge         — handshake/echo/error/shutdown roundtrip với mock sidecar
vitest legacyCommands.test.ts        — 5 tests typed commands
```
