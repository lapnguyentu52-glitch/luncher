# Antares 4.0 Migration

Tài liệu migration theo `remake.md`. Thứ tự milestone không được đảo (§227).

## Trạng thái

Xem `BASELINE.md` — baseline test 254/254, Milestone 1–3 scaffold đã hoàn thành.

## Thứ tự bắt buộc

```text
1.  Freeze baseline          — ĐÃ XONG (docs/migration/BASELINE.md)
2.  Clean source tree        — ĐÃ XONG (.gitignore, xoá pycache/dist zip)
3.  Tauri/Vue shell          — ĐÃ XONG (apps/desktop + src-tauri scaffold)
4.  Typed contracts          — ĐÃ XONG (Batch 3 — docs/protocol/README.md)
5.  Rust core skeleton       — ĐÃ XONG (Milestone 4 — crates/antares-storage + antares-core)
6.  Python bridge adapter    — ĐÃ XONG (Milestone 5 — crates/antares-bridge + legacy/python/sidecar.py)
7+. Migrate từng domain theo §227 — M6 Core user flows ĐÃ XONG, Batch 6 Profiles ĐÃ XONG,
    Batch 7a Mods ĐÃ XONG (mods.* bridge + Installed/Discover/Security UI),
    Batch 7b Modpack/Detail ĐÃ XONG (.mrpack async install + mod detail panel),    Batch 8a Asset Library ĐÃ XONG (asset.* bridge + editor shell 3 cột),
    Batch 8b Resource Studio lifecycle ĐÃ XONG (resource.* bridge: wizard/validate/
    build/install/uninstall/layer.* + Packs tab),
    Batch 9 Visual Studio ĐÃ XONG (visual.* bridge: crosshair/totem/HUD/FX editors
    + export pack qua pipeline Resource Studio),
    Batch 9b Three.js Totem 3D ĐÃ XONG (voxel engine domain-independent + WebGL
    viewport + dispose lifecycle §131 + static fallback),
    Batch 10 Optimization ĐÃ XONG (game optimization plan-first/snapshot/rollback
    §3/§70 + system overview/cleanup undo-able/power §4/§35–§37),
    Batch 11 Network Lab ĐÃ XONG (net.* bridge: endpoints/MC ping/RTT probe jitter
    timeline/DNS §42),
    Batch 12 Runtime/Packet ĐÃ XONG (runtime.* companion endpoint/sessions/metrics/
    pairing + packet.* inspector ring buffer §121/§122),
    Batch 13 Diagnostics ĐÃ XONG (console.* log center + blueprint insights §41,
    repair.* scan-first §39, diagnostic.export evidence) — xem BASELINE.md
21. Remove Python legacy     — chỉ khi parity 100%
```

## Quy tắc khi làm việc

- Mỗi batch 5–10 production files (§228)
- Sau mỗi batch: typecheck → lint → unit → build (§59)
- Không phá 254 Python tests trong migration (§190)
- Cutover chỉ khi critical flows + baseline tests + benchmark đều xanh (§242)
