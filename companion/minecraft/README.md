# Antares Companion (Fabric mod skeleton)

Mod client tùy chọn gửi FPS/frametime telemetry tới Antares Launcher qua local
IPC — hoàn thành vòng lặp **Performance → Minecraft runtime** (spec 3.0 mục 12).

> **Skeleton**: build + handshake + sampling FPS/frametime hoạt động; HUD/crosshair
> runtime hiển thị (mục 11) là bước tiếp theo.

## Flow

```
Antares Launcher launch instance
  → ghi <game dir>/companion.json  (endpoint + token — auto-pairing)
  → Minecraft chạy, mod đọc companion.json lúc khởi tạo
  → vào world → runtime.hello
  → mỗi 500ms → runtime.performance {fps, frameMs, low1}
  → rời world/tắt game → runtime.bye
  → Performance Center (launcher) hiện LIVE panel trong tab Chơi
```

## Build

```bash
cd companion/minecraft
gradle build          # output: build/libs/antares-companion-3.0.0.jar
```

Cài: copy jar vào `mods/` của instance (như mod thường). Mod **im lặng hoàn
toàn** nếu không thấy `companion.json` — an toàn khi chơi không launcher.

## An toàn

- Chỉ gọi endpoint `http://127.0.0.1:*` (PairingReader chặn khác loopback)
- Token riêng mỗi lần launcher khởi động; sai token → HTTP 401, mod bỏ qua
- Toàn bộ gửi chạy trên thread daemon riêng — không block render/game thread
- Mọi lỗi network nuốt im lặng (game không bao giờ crash vì launcher)
