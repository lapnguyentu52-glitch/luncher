# Antares Typed Protocol — Batch 3

Contract giữa Vue UI và Rust core. Hai phía phải đồng bộ:

```text
TS:   apps/desktop/src/types/protocol.ts  +  types/commands.ts
Rust: src-tauri/src/protocol/{error,response,event}.rs  +  commands/app.rs
```

## Response envelope (§96)

Mọi command trả `AntaresResponse<T>` — không throw raw stack trace vào UI:

```json
{ "ok": true,  "data": { ... }, "warnings": [] }
{ "ok": false, "error": { "code": "INSTANCE_LOCKED", "message": "...", "retryable": true, "action": "OPEN_INSTANCE" }, "warnings": [] }
```

TS side có 2 cách gọi (`services/ipc.ts`):

```ts
const payload = await invokeCommand('app_ping')        // throw IpcError nếu !ok
const envelope = await requestCommand('app_ping')      // không throw, trả envelope
```

Ngoài Tauri (browser dev/test), đăng ký mock qua `registerBrowserFallback(command, fn)`.

## Error taxonomy (§117)

Catalog tập trung — không hardcode string lẻ trong UI:

```text
TS:   ErrorCodes / ErrorDomain trong types/protocol.ts
Rust: codes::* (nguồn `antares-app::error::codes`, re-export trong
      protocol/error.rs — test `ts_error_codes_are_all_in_rust_catalog` bắt
      code có ở TS mà thiếu ở Rust ngay khi cargo test)
```

Codes hiện có: `APP_INTERNAL`, `APP_NOT_READY`, `CONFIG_INVALID`, `STORAGE_WRITE_FAILED`,
`NETWORK_UNAVAILABLE`, `IPC_SESSION_STALE`, `INSTANCE_LOCKED`, `JAVA_NOT_FOUND`,
`MC_VERSION_UNKNOWN`, `LEGACY_DISABLED` (M5 — `ANTA_RUST_ONLY=1` chặn lệnh legacy,
UI nhận code này để chuyển flow native), `INSTANCE_NAME_INVALID` (Batch 05 —
`instances.create` tên không hợp lệ), `AUTH_FAILED` + `VALIDATION_FAILED` (Batch 06 —
`accounts.select` / `versions_list` loader lạ), `INSTANCE_NOT_FOUND` (Batch 07a —
`play_preflight` instance không tồn tại, parity sidecar),
`MINECRAFT_VERSION_NOT_FOUND` + `LOADER_INSTALL_FAILED` (Batch 07c — version
không có trong Mojang manifest / cài loader fabric-forge thất bại, parity sidecar).
Rust-side catalog thêm (chưa cần phía UI): `STORAGE_PATH_ESCAPED`, `PROCESS_NOT_FOUND`,
`PROCESS_STILL_RUNNING`, `PROCESS_EXECUTABLE_NOT_FOUND`, `PROCESS_SPAWN_FAILED`.

Thêm code mới = thêm cả 2 phía + test.

## Event envelope + QoS (§88.3, §32, §93)

Rust emit lên channel `antares://event` với shape:

```json
{
  "id": "evt_...",
  "schema": 1,
  "topic": "runtime | download | diagnostics | notification | telemetry | app",
  "qos": "latest | coalesce | batched | lossless",
  "name": "runtime.fps",
  "timestampMs": 0,
  "correlationId": null,
  "payload": {}
}
```

Pipeline UI (§93):

```text
Rust → listen('antares://event') → ingestRaw (validate)
     → QoS buffers → requestAnimationFrame flush → subscribers
```

- `latest` — FPS/CPU: chỉ giữ giá trị mới nhất theo `name`
- `coalesce` — download progress: gộp theo `payload.key`
- `batched` — console lines: gom nhiều, flush 1 lần
- `lossless` — critical error: không bao giờ drop
- Schema lệch (envelope.schema > CURRENT) → reject an toàn, đếm vào `qosDropped` (§31)

## Command registry

Thêm command mới theo 4 bước:

1. Rust: handler trong `src-tauri/src/commands/<domain>.rs` trả `AntaresResponse<T>`
   (kết quả service typed §117 → `AntaresResponse::from_result`; service lấy từ
   composition root `antares-app::AppServices` — không tự new trong handler)
2. Rust: đăng ký trong `lib.rs` `invoke_handler`
3. TS: entry trong `types/commands.ts` `CommandSchema`
4. TS: helper trong `services/<domain>Commands.ts` + browser fallback nếu cần dev offline

Quy tắc: command handler chỉ map request → service → response, không chứa business logic (§88.2).

## Tests

```text
apps/desktop/tests/services/eventIngestion.test.ts  — QoS pipeline (8 tests)
apps/desktop/tests/services/ipc.test.ts             — typed ipc + catalog (7 tests)
src-tauri/src/protocol/*.rs                          — #[cfg(test)] unit tests (cargo test)
```
