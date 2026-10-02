# ANTARES LAUNCHER — DEEP REMAKE / UI + MINECRAFT CORE MASTER PLAN
## Tauri 2 + Vue 3 + TypeScript + Rust + Minecraft Companion
### Windows-first • Codespaces-first development • Performance-first • Glassmorphism + controlled Neumorphism

> **Mục tiêu:** tái cấu trúc Antares Launcher hiện tại thành một launcher Minecraft desktop hiện đại, nhẹ, ổn định và có khả năng debug/quan sát sâu.
>
> **Không rewrite mù:** giữ lại các service đã tốt trong source hiện tại, thay lớp UI/desktop shell và bổ sung một Minecraft Runtime/Network layer thực sự có cấu trúc.
>
> **Target chính:** Windows 10/11 x64. GitHub Codespaces là môi trường phát triển/build orchestration; Windows CI là môi trường xác nhận EXE/installer cuối.

---

# 0. EXECUTIVE DECISION

## 0.1 Stack mới

| Layer | Công nghệ | Vai trò |
|---|---|---|
| Desktop shell | **Tauri 2.x** | Window, native integration, permissions, updater, installer |
| UI | **Vue 3 + TypeScript** | Toàn bộ UI mới |
| Build UI | **Vite** | HMR, code splitting, production bundling |
| State | **Pinia** | App/runtime/profile state |
| Routing | Vue Router | Tab + nested workspace |
| Styling | CSS Modules / scoped CSS + design tokens | Không phụ thuộc UI framework nặng |
| Icons | Lucide hoặc icon set vendored | Consistent iconography |
| Charts | Canvas/WebGL-based lightweight charts | Performance telemetry |
| 3D | **Three.js vendored** | Resource Studio/Totem 3D |
| Native/core | **Rust** | Process, filesystem, downloads, IPC, telemetry, network primitives |
| Legacy bridge | Python sidecar trong migration | Giữ service Python hiện tại hoạt động |
| Minecraft game integration | **Fabric companion mod** | Runtime telemetry/game state |
| Minecraft launch | Rust orchestration + existing minecraft-launcher-lib compatibility layer | Version/loader/runtime |
| Data | JSON + SQLite khi cần query/history | Profile/config/catalog/history |
| Security | Tauri capabilities + allowlists + sandboxed plugin API | Giới hạn quyền |
| CI | GitHub Actions | Linux validation + Windows EXE/installer |
| Packaging | Tauri NSIS | Windows installer + portable |

## 0.2 Vì sao bỏ pywebview

Pywebview hiện tại đã giúp source có UI HTML/CSS/JS, nhưng nó đang là lớp desktop abstraction không còn phù hợp với mục tiêu mới:

- DevTools/debug workflow không đủ tốt và không đồng nhất.
- Frontend hiện tại là ES modules thủ công, thiếu typed contract.
- Native integration nằm rải giữa Python + webview.
- Build/runtime phụ thuộc Python GUI backend.
- UI state lớn dần sẽ khó kiểm soát.
- Muốn tiến tới Rust core + Minecraft telemetry + high-frequency runtime events thì Tauri phù hợp hơn.

**Tauri không có nghĩa là bỏ web UI.** Ngược lại, Vue chạy trong WebView2, còn Rust xử lý native/core.

Tauri 2 dùng WebView2 trên Windows; DevTools có thể mở bằng Inspect Element/Ctrl+Shift+I. Tauri cũng hỗ trợ frontend framework như Vue/Vite và external sidecar binaries.

---

# 1. NGUYÊN TẮC REMAKE

## 1.1 Không phá thứ đang tốt

Source hiện tại đã có nhiều subsystem đáng giữ:

- `AppContext`
- `ConfigManager`
- `EventBus`
- `EventBridge`
- `TaskManager`
- `ProcessManager`
- `HttpClient`
- atomic writes
- file locks
- resumable downloads
- checksum
- Java discovery
- loader registry
- Minecraft launch orchestration
- crash analyzer
- Modrinth integration
- mod scanner/quarantine
- profiles
- backups
- repair
- optimization
- performance telemetry
- Resource Studio
- Visual Studio
- skins
- companion IPC
- plugin manifest/loader
- i18n
- virtual list/log
- notification center

## 1.2 Nguyên tắc mới

```text
UI ≠ business logic
UI ≠ filesystem
UI ≠ process management
UI ≠ Minecraft protocol implementation

Vue
  ↓
Typed Tauri API
  ↓
Rust Core
  ↓
Services / IPC / Storage
  ↓
Minecraft / Java / Companion / Network
```

Python chỉ tồn tại trong migration hoặc service chưa port.

## 1.3 Không làm

- Không Electron.
- Không tiếp tục mở rộng pywebview.
- Không nhồi toàn bộ backend vào Rust ngay trong một batch.
- Không dùng CSS blur khắp màn hình.
- Không animate backdrop-filter.
- Không ép priority/high priority cho Minecraft mặc định.
- Không sửa registry Windows hàng loạt.
- Không kill process người dùng một cách tự động.
- Không hardcode Minecraft version.
- Không hardcode pack format.
- Không hardcode packet schema ở UI.
- Không để UI poll 100–1000 lần/giây.

---

# 2. AUDIT SOURCE HIỆN TẠI

## 2.1 Cấu trúc hiện tại

```text
antares-src/
├── app/
├── api/
├── core/
├── domain/
├── infrastructure/
├── services/
├── frontend/
├── companion/
├── tests/
├── packaging/
└── docs/
```

Đây là nền service-oriented tốt.

## 2.2 UI hiện tại

Hiện có:

```text
frontend/
├── app/
├── i18n/
├── state/
├── styles/
└── tabs/
```

Có nhiều tab đã tồn tại:

```text
Dashboard
Play
Accounts
Instances
Mods
Profiles
Downloads
Servers
Performance
Game Optimization
System Optimization
Diagnostics
Logs
Resource Studio
Visual Studio
Skins
Backups
Repair
Plugins
Security
Settings
```

### Vấn đề

1. JavaScript thuần tăng nhanh sẽ khó refactor.
2. API contract không có generated TypeScript types.
3. State lifecycle không được component hóa mạnh.
4. Router/state/view chưa có domain ownership rõ.
5. DevTools/debug phụ thuộc WebView runtime.
6. UI hiện tại có glass nhưng chưa có spatial hierarchy mạnh.
7. Resource Studio và Visual Studio cần một workspace/editor architecture riêng.
8. Network/packet runtime chưa phải subsystem chính thức.
9. Companion mới gửi performance/game events, chưa phải runtime telemetry bus đầy đủ.
10. Minecraft version support cần một abstraction thống nhất hơn.

---

# 3. TARGET ARCHITECTURE

```text
┌─────────────────────────────────────────────────────────────┐
│                    ANTARES DESKTOP                          │
├─────────────────────────────────────────────────────────────┤
│ Tauri 2 Shell                                               │
│  ├─ Windows window                                          │
│  ├─ WebView2                                                 │
│  ├─ DevTools                                                 │
│  ├─ tray / notifications                                    │
│  ├─ updater                                                  │
│  └─ filesystem/process permissions                          │
├─────────────────────────────────────────────────────────────┤
│ Vue 3 + TypeScript                                          │
│  ├─ App Shell                                               │
│  ├─ Dashboard                                               │
│  ├─ Play Workspace                                          │
│  ├─ Library / Instances                                     │
│  ├─ Mods                                                    │
│  ├─ Resource Studio                                         │
│  ├─ Visual Studio                                           │
│  ├─ Optimization                                            │
│  ├─ Network Lab                                             │
│  ├─ Runtime Monitor                                         │
│  ├─ Diagnostics                                             │
│  └─ Settings                                                │
├─────────────────────────────────────────────────────────────┤
│ Typed Command/Event Contract                                │
├─────────────────────────────────────────────────────────────┤
│ Rust Core                                                   │
│  ├─ app                                                     │
│  ├─ config                                                  │
│  ├─ storage                                                 │
│  ├─ downloads                                               │
│  ├─ process                                                 │
│  ├─ minecraft                                               │
│  ├─ network                                                 │
│  ├─ telemetry                                               │
│  ├─ optimization                                            │
│  ├─ resources                                               │
│  ├─ profiles                                                │
│  ├─ diagnostics                                             │
│  ├─ security                                                │
│  └─ plugin runtime                                          │
├─────────────────────────────────────────────────────────────┤
│ Compatibility / Migration                                   │
│  └─ Python service sidecar                                  │
├─────────────────────────────────────────────────────────────┤
│ Minecraft Companion                                        │
│  └─ Fabric client mod                                       │
└─────────────────────────────────────────────────────────────┘
```

---

# 4. TARGET FOLDER STRUCTURE

```text
antares/
├── apps/
│   ├── desktop/
│   │   ├── src/
│   │   │   ├── App.vue
│   │   │   ├── main.ts
│   │   │   ├── router/
│   │   │   ├── stores/
│   │   │   ├── layouts/
│   │   │   ├── components/
│   │   │   ├── features/
│   │   │   ├── composables/
│   │   │   ├── services/
│   │   │   ├── types/
│   │   │   └── styles/
│   │   └── public/
│   │
│   └── companion/
│       └── minecraft/
│
├── crates/
│   ├── antares-core/
│   ├── antares-storage/
│   ├── antares-minecraft/
│   ├── antares-network/
│   ├── antares-downloads/
│   ├── antares-telemetry/
│   ├── antares-optimization/
│   ├── antares-resources/
│   ├── antares-diagnostics/
│   ├── antares-security/
│   └── antares-plugin-api/
│
├── legacy/
│   └── python/
│
├── src-tauri/
│   ├── src/
│   │   ├── commands/
│   │   ├── events/
│   │   ├── state/
│   │   ├── windows/
│   │   └── main.rs
│   ├── capabilities/
│   ├── icons/
│   └── tauri.conf.json
│
├── docs/
│   ├── architecture/
│   ├── ui/
│   ├── minecraft/
│   ├── network/
│   ├── resource-studio/
│   ├── optimization/
│   ├── storage/
│   ├── plugins/
│   ├── migration/
│   └── release/
│
├── scripts/
├── tests/
└── .github/workflows/
```

---

# 5. UI DESIGN SYSTEM

## 5.1 Design language

Tên nội bộ:

> **Antares Spatial Glass**

Kết hợp:

- Glassmorphism: surface nổi.
- Neumorphism: depth rất nhẹ ở control.
- Minimal gaming UI.
- Cyber-dark.
- Premium launcher.
- Không lạm dụng glow.
- Không dùng gradient cho mọi card.

## 5.2 Palette

```css
--bg-0: #07080c;
--bg-1: #0b0e14;
--surface-1: #11151d;
--surface-2: #161b24;
--surface-3: #1b222d;

--text-1: #f5f7fb;
--text-2: #aeb7c5;
--text-3: #6f7887;

--accent: #ff5c47;
--accent-soft: rgba(255, 92, 71, .14);
--accent-glow: rgba(255, 92, 71, .22);

--success: #50d890;
--warning: #f5bd4f;
--danger: #ff5c66;
--info: #62a8ff;
```

Logo/PNG người dùng cung cấp sẽ trở thành:

```text
Brand asset
  ├─ launcher logo
  ├─ splash mark
  ├─ tray icon
  ├─ installer icon
  ├─ empty-state mark
  └─ loading animation source
```

Không nhúng logo bằng text/SVG giả nếu PNG chính thức đã có.

## 5.3 Glass rule

Chỉ dùng glass cho:

- topbar
- floating sidebar
- modal
- command palette
- inspector
- notification center
- media/preview overlay

Card dữ liệu thông thường:

```text
solid surface + border + shadow
```

Không:

```css
backdrop-filter: blur(40px);
```

Budget:

```text
normal glass: 8–14px
heavy overlay: <= 18px
low-end: blur disabled
```

## 5.4 Neumorphism

Chỉ dùng cho:

- toggle
- segmented control
- compact button
- slider
- small icon button

Không dùng neumorphism cho:

- text-heavy panels
- large cards
- diagnostics tables
- console
- mod lists

Lý do: contrast/readability và hierarchy.

---

# 6. APP SHELL

## 6.1 Layout

```text
┌──────────────────────────────────────────────────────────────┐
│ Logo │ Search / Command │ Runtime │ Download │ Bell │ Avatar │
├───────────────┬──────────────────────────────────────────────┤
│               │                                              │
│ Dashboard     │                                              │
│ Play          │               Main Workspace                 │
│ Instances     │                                              │
│ Mods          │                                              │
│ Resource      │                                              │
│ Visual        │                                              │
│ Optimization  │                                              │
│ Network       │                                              │
│ Runtime       │                                              │
│ Diagnostics   │                                              │
│               │                                              │
├───────────────┴──────────────────────────────────────────────┤
│ Status │ Instance │ Minecraft │ Java │ Network │ CPU │ RAM  │
└──────────────────────────────────────────────────────────────┘
```

## 6.2 Sidebar

Có 3 mode:

```text
Expanded
Compact
Auto
```

Keyboard:

```text
Ctrl+K     Command palette
Ctrl+1..9  Section shortcuts
Ctrl+P     Quick Play
Ctrl+Shift+P  Profile switcher
F8         Runtime monitor
F9         Diagnostics
```

---

# 7. DASHBOARD

Dashboard không còn là card grid đơn thuần.

## Hero

```text
┌──────────────────────────────────────────────────────┐
│ Minecraft 26.x / Fabric                              │
│ Performance Profile                                  │
│                                                      │
│              [ PLAY ]                                │
│                                                      │
│ FPS --   Ping --   RAM --   Instance --              │
└──────────────────────────────────────────────────────┘
```

## Panels

```text
Recent Instances
Downloads
Mod Updates
Performance
Network status
Last crash
Notifications
```

---

# 8. PLAY WORKSPACE

## Chức năng

- chọn account
- chọn instance
- chọn Minecraft version
- chọn loader
- chọn profile
- JVM memory
- advanced launch arguments
- server quick-connect
- game directory
- resource pack profile
- mod profile
- Java runtime
- launch diagnostics

## Preflight

Trước khi Play:

```text
Java ✓
Version ✓
Loader ✓
Assets ✓
Mods ✓
Disk ✓
Memory ✓
Account ✓
Network ✓
Profile ✓
```

Không hợp lệ:

```text
Play disabled
```

Hoặc:

```text
Play Anyway
```

nếu lỗi là warning chứ không phải blocker.

---

# 9. INSTANCE SYSTEM

Instance là đơn vị quản lý chính.

```text
Instance
├── Minecraft version
├── Loader
├── Java
├── account
├── mods
├── resource packs
├── shader packs
├── screenshots
├── saves
├── options
├── runtime profile
└── optimization profile
```

## Instance detail

Tabs:

```text
Overview
Mods
Resource Packs
Shaders
Worlds
Screenshots
Logs
Performance
Network
Files
Settings
```

---

# 10. VERSION SYSTEM — HỖ TRỢ VERSION CAO NHẤT

## Không hardcode

Tạo:

```text
MinecraftVersionRegistry
```

Nguồn:

```text
Mojang version manifest
Fabric metadata
Forge metadata
NeoForge metadata
Quilt metadata
```

## Version object

```ts
interface MinecraftVersion {
  id: string
  type: "release" | "snapshot" | "old_beta" | "old_alpha"
  releaseTime: string
  javaMajor: number
  protocol?: number
  loaderSupport: LoaderSupport[]
  assetsIndex?: string
}
```

## Support policy

```text
Latest stable
Latest supported loader
Snapshots
Historical versions
```

Nếu version mới chưa được verify:

```text
Detected
Experimental
```

Không tự giả định compatibility.

---

# 11. MOD MANAGER

## Tab

```text
Mods
```

### Sections

```text
Installed
Discover
Updates
Compatibility
Security
Collections
```

## Mod card

```text
Icon
Name
Author
Version
MC versions
Loader
Dependencies
Client/Server/Both
Installed
Update
Security
```

## Mod compatibility graph

```text
Mod A
 ├─ requires Fabric API
 ├─ compatible MC 1.21.11
 └─ conflicts Mod C
```

## Actions

```text
Install
Update
Disable
Remove
Quarantine
Open page
Open file
Check dependencies
```

## Security

Không execute JAR.

Scanner:

```text
metadata
manifest
entrypoints
dependencies
known signatures
suspicious strings
network/API indicators
filesystem indicators
```

Kết quả:

```text
Safe-looking
Review
Suspicious
Blocked
```

Không tuyên bố "100% safe".

---

# 12. RESOURCE STUDIO — TÁI CẤU TRÚC THÀNH EDITOR THỰC

## Tab

```text
Resource Studio
```

## Workspace

```text
┌──────────────┬─────────────────────────────┬───────────────┐
│ Assets       │          Canvas             │ Inspector     │
│              │                             │               │
│ textures     │     2D / 3D preview        │ dimensions    │
│ models       │                             │ UV            │
│ sounds       │                             │ path          │
│ fonts        │                             │ metadata      │
│ particles    │                             │               │
├──────────────┴─────────────────────────────┴───────────────┤
│ Layers / Timeline / Build / Validation                      │
└─────────────────────────────────────────────────────────────┘
```

## Create pack wizard

```text
1. Name
2. Minecraft version
3. Template
4. Resolution
5. Assets
6. Crosshair
7. HUD
8. Totem
9. Models
10. Sounds
11. Language
12. Preview
13. Validate
14. Build
15. Install
```

## Asset library

Content-addressed:

```text
sha256
```

Catalog:

```text
id
name
category
tags
hash
dimensions
mime
createdAt
updatedAt
```

## Supported asset types

```text
PNG
JPEG (import only where appropriate)
JSON
MCMeta
OGG
WAV
TTF/OTF where valid
```

## Security limits

```text
PNG <= 8 MB
PNG <= 4096x4096
ZIP <= 256 MB uncompressed
<= 4096 entries
single file <= 64 MB
```

## ZIP safety

Mandatory:

```text
ZIP Slip protection
absolute path rejection
symlink rejection
bomb detection
size limits
entry limits
duplicate path detection
MIME validation
```

---

# 13. TOTEM 3D STUDIO

## Renderer

```text
Three.js
```

Vendored/offline.

## Features

```text
Orbit
Pan
Zoom
Reset
Perspective
Orthographic
Grid
Axes
Wireframe
Lighting
Material preview
Texture preview
```

## Voxel model

```text
Voxel
├── position
├── size
├── faces
├── UV
├── texture
└── material
```

## Limits

```text
<= 32 cubes recommended
33–64 warning
>64 reject by default
```

## Export

```text
item definition
model JSON
texture PNG
atlas
pack.mcmeta
```

Version-aware.

---

# 14. CROSSHAIR STUDIO

Presets:

```text
Dot
Plus
Cross
Gap
T
Circle
Minimal
Custom
```

Editor:

```text
size
thickness
gap
outline
opacity
color
center
dynamic
```

Live preview:

```text
Minecraft HUD mockup
```

Export path is version-aware.

---

# 15. HUD STUDIO

Widgets:

```text
FPS
CPS
Ping
Coordinates
Armor
Potions
Keystrokes
Target HUD
Session time
Memory
Server
Biome
Direction
Clock
```

Layout:

```text
drag
snap
resize
lock
visibility
opacity
scale
```

Save as:

```text
HUD Profile
```

---

# 16. OPTIMIZATION CENTER

Hai tab lớn:

```text
Game Optimization
System Optimization
```

## Game Optimization

### Scan

```text
Hardware
Java
Minecraft
Loader
Mods
Render settings
JVM
Disk
Network
```

### Profiles

```text
Balanced
Performance
Competitive
Low-End
Visual
Battery
Streaming
Custom
```

## Plan-first

```text
SCAN
 ↓
PLAN
 ↓
SHOW DIFF
 ↓
BACKUP
 ↓
APPLY
 ↓
VERIFY
 ↓
BENCHMARK
```

Không:

```text
ONE CLICK → thay đổi mù
```

## Benchmark

Không chỉ đo average FPS.

Đo:

```text
average FPS
1% low
0.1% low
frame time
frame time variance
GC pause
CPU usage
RAM
GPU usage
chunk generation time
server ping
```

---

# 17. SYSTEM OPTIMIZATION

## Safe-only mặc định

```text
Launcher cache
download temp
old logs
orphaned artifacts
stale crash reports
```

## Windows

Thông tin:

```text
Power plan
CPU
GPU
RAM
Disk
Network adapter
Windows build
```

### Không tự động

```text
Registry hacks
Services disable
Defender disable
Firewall disable
Driver modifications
BIOS changes
```

## Process control

Chỉ process do Antares sở hữu.

```text
Normal
Above Normal
High
```

Không mặc định High.

---

# 18. NETWORK LAB — SUBSYSTEM MỚI

Đây là phần source hiện tại cần nâng cấp sâu nhất.

## Mục tiêu

Không chỉ:

```text
net_check()
```

mà có:

```text
Network Diagnostics
Minecraft Server Ping
Runtime Network Telemetry
Packet Inspector
Connection Timeline
DNS diagnostics
Download network metrics
```

## Network pipeline

```text
┌─────────────┐
│ DNS         │
├─────────────┤
│ TCP connect │
├─────────────┤
│ MC handshake│
├─────────────┤
│ status ping  │
├─────────────┤
│ runtime      │
└─────────────┘
```

## Metrics

```text
DNS latency
TCP connect
server response
MC handshake
RTT
jitter
timeout count
disconnect count
reconnect count
download throughput
download errors
packet counters
```

---

# 19. PACKET SYSTEM

## Không decode tất cả ở UI

Packet pipeline:

```text
Minecraft / Companion
        ↓
Packet Capture Layer
        ↓
Packet Decoder
        ↓
Normalizer
        ↓
Ring Buffer
        ↓
Event Aggregator
        ↓
UI
```

## Packet object

```ts
interface PacketEvent {
  timestamp: number
  direction: "inbound" | "outbound"
  protocolState: "handshake" | "status" | "login" | "play"
  packetId: number
  packetName?: string
  size: number
  latencySample?: number
  payloadPreview?: string
}
```

## UI

```text
Packets
────────────────────────────
IN   KeepAlive        42 B
OUT  PlayerMove       31 B
IN   ChunkData        812 KB
IN   EntityUpdate     93 B
```

## Filters

```text
Inbound
Outbound
Packet name
Packet ID
Size
Time
Protocol state
```

## Performance

```text
Ring buffer only
Default 10k records
Optional 100k
Sampling mode
Payload preview disabled by default
```

Không ghi full packet payload vô hạn vào disk.

---

# 20. MINECRAFT RUNTIME MONITOR

## Companion mod nâng cấp

Hiện companion mới có:

```text
hello
bye
performance
```

Target:

```text
runtime.performance
runtime.game_state
runtime.chat
runtime.error
runtime.network
runtime.world
runtime.player
runtime.resource
runtime.packet
```

## Player state

```text
health
food
saturation
position
dimension
yaw
pitch
velocity
selected slot
XP
```

## Runtime

```text
FPS
frame time
1% low
memory
heap
GC
loaded chunks
entities
render distance
simulation distance
```

## Network

```text
server
ping
jitter
connection state
packet counters
```

---

# 21. RUNTIME DASHBOARD

```text
┌──────────────────────────────────────────────────────┐
│ Minecraft Runtime                                    │
├───────────┬──────────┬──────────┬───────────────────┤
│ FPS       │ Ping     │ RAM      │ Frame time        │
│ 144       │ 32 ms    │ 3.2 GB   │ 6.9 ms            │
├───────────┴──────────┴──────────┴───────────────────┤
│ FPS / Frametime graph                                │
├───────────────────────────┬──────────────────────────┤
│ CPU/GPU                   │ Network                  │
│                           │ RTT / jitter / loss      │
└───────────────────────────┴──────────────────────────┘
```

---

# 22. DIAGNOSTICS CENTER

## Unified diagnostic pipeline

```text
Launcher logs
Minecraft latest.log
Crash reports
Java stderr
Mod metadata
Network diagnostics
Runtime telemetry
```

## Error fingerprint

```text
fingerprint
category
severity
firstSeen
lastSeen
count
instance
version
mods
java
recommendation
```

## Categories

```text
JAVA
MOD
LOADER
AUTH
NETWORK
DISK
GPU
OPENGL
ASSET
RESOURCE_PACK
PERMISSION
CONFIG
UNKNOWN
```

## UI

```text
Problem
Why it happened
Evidence
Affected instance
Suggested fix
Preview
Apply
Open logs
Create backup
```

---

# 23. LOG CONSOLE

Console phải là terminal-grade.

Features:

```text
virtualized
search
regex
level filter
source filter
timestamp
copy
export
pause
follow
clear view
jump to error
```

Không render 100k DOM nodes.

---

# 24. PROFILE SYSTEM — NÂNG CẤP

Một profile phải chứa:

```json
{
  "id": "...",
  "name": "Competitive",
  "account": "...",
  "instance": "...",
  "minecraft": {
    "version": "...",
    "loader": "fabric"
  },
  "java": {
    "runtime": "...",
    "xms": "...",
    "xmx": "..."
  },
  "mods": {
    "collection": "..."
  },
  "resourcePack": "...",
  "hud": "...",
  "optimization": "...",
  "network": {
    "diagnostics": true
  },
  "launch": {
    "jvmArgs": [],
    "gameArgs": []
  }
}
```

## Profile operations

```text
Create
Duplicate
Rename
Apply
Capture
Export
Import
Validate
Delete
Rollback
```

---

# 25. USER DATA SYSTEM

## Storage root

```text
%APPDATA%\Antares\
```

hoặc portable:

```text
Antares\
├── data\
├── cache\
├── logs\
├── instances\
├── profiles\
├── assets\
└── backups\
```

## Data classes

```text
config
user state
credentials
cache
game data
diagnostics
history
```

Không trộn.

## Credentials

```text
Windows Credential Manager / OS secure storage
```

Không lưu refresh token plain JSON.

---

# 26. DATABASE STRATEGY

Không đưa SQLite vào mọi thứ.

## JSON

Dùng cho:

```text
settings
profiles
small configs
project manifests
```

## SQLite

Dùng khi cần:

```text
notifications
download history
diagnostic history
packet metadata history
performance sessions
asset catalog lớn
mod cache
server history
```

## Rule

```text
small + human-readable → JSON
query-heavy/history → SQLite
large binary → filesystem
```

---

# 27. DOWNLOAD ENGINE

## Pipeline

```text
Resolve
 ↓
HEAD/metadata
 ↓
cache check
 ↓
resume
 ↓
stream
 ↓
hash
 ↓
atomic rename
 ↓
verify
 ↓
publish event
```

## Metrics

```text
bytes
speed
ETA
retries
HTTP status
source
checksum
```

## Download UI

```text
Global Download Center
Per-instance downloads
Per-resource downloads
```

---

# 28. JAVA RUNTIME MANAGER

## Detect

```text
PATH
JAVA_HOME
Mojang runtime
Antares-managed runtime
Custom runtime
```

## Validate

```text
major version
architecture
vendor
executable
memory
compatibility
```

## Recommended runtime

Không tự động thay Java nếu user đã có Java hợp lệ.

---

# 29. MINECRAFT LAUNCH PIPELINE

```text
Profile
 ↓
Instance
 ↓
Minecraft version
 ↓
Loader
 ↓
Java
 ↓
Libraries
 ↓
Assets
 ↓
Natives
 ↓
Mods
 ↓
Resource packs
 ↓
Arguments
 ↓
Companion pairing
 ↓
Process spawn
 ↓
Runtime attach
 ↓
Game session
 ↓
Exit analyzer
```

---

# 30. LAUNCH PREFLIGHT

```text
[✓] Java
[✓] Minecraft JSON
[✓] Libraries
[✓] Assets
[✓] Mods
[✓] Resource pack
[✓] Memory
[✓] Disk
[✓] Account
[✓] Network
[✓] Companion
```

Mỗi check trả:

```text
PASS
WARNING
ERROR
```

---

# 31. COMPANION IPC V2

## Current

```text
HTTP loopback
token
hello/bye/performance
```

## Target

```text
Loopback IPC
authenticated session
versioned schema
heartbeat
backpressure
sequence number
ack
compression for large messages
```

## Message

```json
{
  "schema": 2,
  "session": "...",
  "sequence": 1234,
  "type": "runtime.performance",
  "timestamp": 0,
  "payload": {}
}
```

## Rules

```text
sequence gap → diagnostic
invalid schema → reject
wrong token → 401
unknown packet → ignore safely
oversized payload → reject
```

---

# 32. EVENT BUS V2

```text
Core EventBus
   ├─ UI event stream
   ├─ telemetry stream
   ├─ diagnostic stream
   ├─ runtime stream
   ├─ download stream
   └─ notification stream
```

## Event policies

```text
LATEST
APPEND
COALESCE
THROTTLED
LOSSLESS
```

Ví dụ:

```text
FPS → LATEST
CPU → LATEST
download progress → COALESCE
console lines → APPEND
critical error → LOSSLESS
packet counters → COALESCE
```

---

# 33. UI PERFORMANCE

## Target

```text
Cold start < 2.5s on normal Windows SSD
Interactive shell < 1.5s
Navigation < 100ms perceived
No full-screen rerender
No 10k DOM nodes
```

## Vue rules

- Composition API.
- TypeScript strict.
- `shallowRef` cho large datasets.
- Virtual list.
- Lazy route loading.
- Component-level async loading.
- Avoid deep reactive objects for telemetry.
- Batch high-frequency events.
- `requestAnimationFrame` cho visual sampling.
- Worker cho CPU-heavy frontend transformations.

---

# 34. LOW-END MODE

Auto-detect:

```text
RAM
CPU cores
GPU class
DPI
WebView2 capability
```

Disable/reduce:

```text
heavy blur
ambient animation
3D preview quality
chart update frequency
shadow layers
glow
particle UI
```

Không disable functional features.

---

# 35. ACCESSIBILITY

```text
Keyboard navigation
Focus ring
Reduced motion
Reduced transparency
High contrast
Font scaling
Tooltips
ARIA
Screen-reader labels
```

---

# 36. DEBUG MODE

## Dev

```text
ANTARES_DEV=1
```

Features:

```text
DevTools
event inspector
state inspector
network diagnostics
IPC logs
command timing
render timing
memory stats
```

## Command palette

```text
> open devtools
> reload ui
> inspect event
> dump state
> test notification
> test download
> test packet
> benchmark ui
> clear cache
```

---

# 37. WEB DEBUGGING

Tauri/WebView2 sẽ cho phép:

```text
Inspect Element
Console
Network
Sources
Application
Performance
Memory
```

Debug flow:

```text
Vue component
 ↓
Browser DevTools
 ↓
Tauri command
 ↓
Rust log
 ↓
service
```

Không cần mở Chrome riêng để debug UI.

---

# 38. RUST CORE DEBUGGING

```text
RUST_BACKTRACE=1
RUST_LOG=antares=debug
```

Structured log:

```json
{
  "timestamp": 0,
  "level": "INFO",
  "target": "minecraft.launch",
  "event": "process_started",
  "instance": "...",
  "pid": 1234
}
```

---

# 39. RESOURCE PACK VERSIONING

Giữ logic tốt từ Resource Studio hiện tại.

Một API duy nhất:

```text
versioning.parse()
versioning.info()
versioning.pack_meta()
versioning.pack_format()
versioning.item_model_mode()
versioning.crosshair_path()
```

Không hardcode format trong UI.

## Version handling

```text
known → exact
future → clamp + warning
unknown → safe warning
```

---

# 40. MOD + RESOURCE PACK + PROFILE RELATIONSHIP

```text
Profile
 ├── Instance
 ├── Mod Collection
 ├── Resource Pack Project
 ├── HUD
 ├── Crosshair
 ├── Totem
 ├── Optimization
 └── Java
```

Một click:

```text
Apply Competitive Profile
```

→ mọi subsystem tạo plan.

---

# 41. BACKUP SYSTEM

Trước mutation:

```text
Optimization
Profile Apply
Resource install
Mod bulk update
Repair
Migration
```

Tạo snapshot:

```text
snapshot id
timestamp
reason
files
hashes
size
```

Rollback phải atomic.

---

# 42. MIGRATION STRATEGY

## Phase 1

```text
Vue/Tauri shell
      ↓
Python legacy backend
```

## Phase 2

```text
Vue/Tauri
      ↓
Rust API
      ↓
Python adapter
```

## Phase 3

```text
Vue/Tauri
      ↓
Rust Core
      ↓
Minecraft / Java / filesystem
```

## Phase 4

```text
Python legacy removed
```

Không xóa Python trước khi Rust parity đạt 100%.

---

# 43. PYTHON COMPATIBILITY BRIDGE

Tạm giữ:

```text
legacy/python/
```

Sidecar:

```text
antares-legacy.exe
```

Protocol:

```text
JSON Lines / local IPC
```

Ví dụ:

```json
{
  "id": "req-123",
  "method": "resource.build",
  "params": {}
}
```

Response:

```json
{
  "id": "req-123",
  "ok": true,
  "data": {}
}
```

---

# 44. PLUGIN SYSTEM

## Plugin không được quyền full system mặc định.

Manifest:

```json
{
  "id": "example",
  "name": "Example",
  "version": "1.0.0",
  "permissions": [
    "ui.tab",
    "minecraft.read",
    "instance.read"
  ]
}
```

Permissions:

```text
ui.tab
ui.command
minecraft.read
minecraft.runtime
instance.read
instance.write
network.diagnostics
filesystem.read
filesystem.write
```

Không:

```text
arbitrary process execute
arbitrary registry
arbitrary network
```

trừ khi explicit elevated permission.

---

# 45. NOTIFICATION CENTER

Realtime:

```text
Download complete
Mod update
Crash detected
Network warning
Resource build complete
Optimization applied
Backup created
Game started
Game exited
Server offline
```

Mỗi notification:

```text
id
severity
timestamp
title
message
action
source
read
```

---

# 46. SEARCH SYSTEM

Global search:

```text
Instances
Mods
Profiles
Resource projects
Servers
Settings
Commands
Diagnostics
```

`Ctrl+K`.

Search phải fuzzy + indexed, không scan toàn filesystem mỗi keypress.

---

# 47. SETTINGS

```text
Appearance
Performance
Minecraft
Downloads
Network
Java
Accounts
Privacy
Notifications
Plugins
Developer
Storage
Backup
```

## Appearance

```text
Theme
Accent
Density
Transparency
Animation
Font scale
Sidebar mode
```

---

# 48. LOADING / SPLASH

PNG logo người dùng cung cấp.

Flow:

```text
Splash
 ↓
Load config
 ↓
Initialize core
 ↓
Load cached state
 ↓
Connect event bus
 ↓
Initialize Minecraft registry
 ↓
Initialize UI
 ↓
Fade to dashboard
```

Nếu lỗi:

```text
Core initialization failed
Diagnostic ID
Open logs
Safe mode
Retry
```

Không để splash treo vô hạn.

---

# 49. ERROR BOUNDARY

Mỗi feature:

```text
FeatureErrorBoundary
```

UI crash không được làm crash toàn app.

Ví dụ:

```text
Resource Studio crashed
```

nhưng:

```text
Dashboard
Play
Instances
```

vẫn hoạt động.

---

# 50. SECURITY MODEL

## Tauri capabilities

Chỉ cấp:

```text
filesystem paths cần thiết
process commands cần thiết
notification
window
shell sidecar cần thiết
```

Không cấp wildcard nếu không cần.

## Path sandbox

Mọi path:

```text
normalize
canonicalize
validate root
reject traversal
```

## Network

Backend request đi qua Rust service.

UI không tự gọi arbitrary external endpoints cho business-critical operations.

---

# 51. NETWORK OPTIMIZATION — THỰC TẾ

Không tuyên bố "giảm ping" bằng registry hack.

Antares chỉ tối ưu phần launcher kiểm soát được:

```text
connection diagnostics
DNS diagnostics
download concurrency
HTTP keep-alive
connection reuse
download chunking
retry/backoff
server status probing
runtime measurement
```

Minecraft gameplay latency phụ thuộc:

```text
server
routing
ISP
distance
server tick
client workload
```

UI phải hiển thị nguyên nhân có thể đo được thay vì hứa giảm ping.

---

# 52. DOWNLOAD OPTIMIZATION

```text
parallel downloads
connection pool
resume
range
checksum
cache
deduplication
```

Giới hạn concurrency:

```text
Low-End: 2
Normal: 4
Fast: 6–8
```

Tự điều chỉnh theo network.

---

# 53. MINECRAFT PERFORMANCE DEEP OPTIMIZATION

## Client

```text
JVM
render distance
simulation distance
particles
entity distance
mipmap
vsync
fps cap
```

## Mod recommendations

Không tự cài nếu conflict.

Flow:

```text
Detect
 → compatibility
 → recommendation
 → user approval
 → backup
 → install
 → validate
 → benchmark
```

## Runtime

```text
FPS
frametime
GC
heap
chunk generation
entities
network
```

---

# 54. PERFORMANCE SESSION RECORDING

Session:

```text
session id
instance
MC version
loader
mods hash
profile
Java
hardware
start
end
metrics
```

Cho phép compare:

```text
Before
vs
After
```

Không dùng score "mang tính tuyệt đối".

---

# 55. UI COMPONENT LIBRARY

Tạo nội bộ:

```text
AButton
AIconButton
ACard
AGlass
ANeumorphControl
ATabs
AInput
ASelect
ASlider
AToggle
ABadge
AProgress
ASkeleton
ADataTable
AVirtualList
AChart
ACommandPalette
AModal
ADrawer
AToast
AInspector
AConsole
AStat
AEmptyState
AErrorState
```

Mỗi component:

```text
typed props
events
states
keyboard behavior
loading state
disabled state
dark mode
reduced motion
```

---

# 56. RESOURCE STUDIO COMPONENTS

```text
AssetTree
AssetGrid
AssetPreview
TextureCanvas
ModelViewport
LayerPanel
InspectorPanel
PackManifestEditor
PackVersionSelector
BuildPanel
ValidationPanel
InstallPanel
```

---

# 57. NETWORK LAB COMPONENTS

```text
ConnectionTimeline
PingGraph
JitterGraph
PacketTable
PacketInspector
DNSCheck
TCPCheck
ServerStatus
DownloadNetworkChart
```

---

# 58. DIAGNOSTICS COMPONENTS

```text
IssueList
IssueDetail
EvidencePanel
LogViewer
CrashStack
ModConflictGraph
RepairPlan
RepairPreview
```

---

# 59. FILE ORGANIZATION RULE

Không tạo file 2000 dòng.

Rule:

```text
component <= ~400 lines
service <= ~500 lines
large domain → split
```

Một batch sửa:

```text
5–10 files
```

Sau mỗi batch:

```text
typecheck
lint
unit
smoke
build
```

---

# 60. TESTING MATRIX

## Frontend

```text
vue-tsc
eslint
unit
component
router smoke
store smoke
```

## Rust

```text
cargo fmt --check
cargo clippy
cargo test
```

## Integration

```text
Tauri command roundtrip
sidecar startup
filesystem
download
profile
Minecraft launch
```

## Minecraft

```text
version matrix
loader matrix
Java matrix
companion handshake
runtime events
```

---

# 61. PERFORMANCE TESTS

## UI

```text
10k logs
10k mods
10k assets
1000 notifications
high-frequency telemetry
```

## Core

```text
download throughput
hashing
JSON migration
SQLite queries
event throughput
IPC latency
packet decode
```

## Target

```text
No unbounded queue
No event storm
No memory leak
No 100% CPU idle
No full UI rerender on telemetry
```

---

# 62. MEMORY BUDGET

Launcher idle target:

```text
Tauri shell + Vue: low overhead
Rust core: bounded
Telemetry: ring buffer
Logs: bounded
Packet capture: bounded
```

Không dùng mục tiêu RAM tuyệt đối bất khả thi trên mọi Windows máy; thay vào đó đo:

```text
idle
dashboard
mods list
resource studio
runtime monitor
game running
10k logs
```

---

# 63. CODESPACES WORKFLOW

## Development

```bash
npm install
npm run dev
```

Rust:

```bash
cargo check
cargo test
```

Frontend:

```bash
npm run typecheck
npm run lint
npm run build
```

Tauri:

```bash
npm run tauri dev
```

## Codespaces rule

Codespaces dùng để:

```text
code
typecheck
unit test
lint
Linux validation
docs
CI orchestration
```

Windows runner dùng để:

```text
Tauri Windows build
WebView2 integration
NSIS
EXE smoke test
installer test
```

---

# 64. GITHUB ACTIONS

## Workflow

```text
push
 ↓
frontend-check
 ↓
rust-check
 ↓
python-legacy-check
 ↓
unit tests
 ↓
integration
 ↓
Linux build
 ↓
Windows build
 ↓
EXE smoke
 ↓
artifact
```

## Windows runner

```text
windows-latest
```

Build:

```text
Antares.exe
Antares-Setup.exe
portable artifact
```

---

# 65. PACKAGING

## Tauri

```text
NSIS
```

Artifacts:

```text
Antares-Setup-x64.exe
Antares-Portable-x64.zip
```

## Portable

```text
Antares.exe
data/
```

Không ghi user state cạnh EXE nếu user đang cài vào Program Files.

---

# 66. UPDATER

```text
check
 ↓
show release notes
 ↓
download
 ↓
verify signature
 ↓
install
 ↓
restart
```

Không update nếu signature verification fail.

---

# 67. RESOURCE / MOD CACHE

Cache key:

```text
URL + expected hash + version
```

Không cache vô hạn.

GC policy:

```text
LRU
max size
age
pinned assets
```

---

# 68. OFFLINE MODE

Nếu mất mạng:

```text
Dashboard
Instances
Profiles
Installed Mods
Resource Studio
Visual Studio
Diagnostics
```

vẫn hoạt động.

Online-only:

```text
Mod discovery
version metadata refresh
account auth
remote downloads
```

---

# 69. ACCOUNT SYSTEM

Hỗ trợ architecture:

```text
Microsoft
Offline
```

Không lưu password.

Account object:

```text
id
type
displayName
uuid
lastUsed
metadata
credentialRef
```

Microsoft token lưu OS secure storage.

---

# 70. SERVER MANAGEMENT

Giữ server subsystem hiện tại nhưng đưa vào workspace riêng:

```text
Servers
├── Dashboard
├── Console
├── Players
├── Files
├── Config
├── Backups
└── Performance
```

Console virtualized.

---

# 71. SERVER NETWORK

```text
port check
server ping
MOTD
players
version
latency
```

Không nhầm:

```text
server status latency
```

với:

```text
player gameplay ping
```

---

# 72. RESOURCE PACK + INSTANCE LAYERING

Layer order:

```text
Vanilla
 ↓
Instance base
 ↓
Profile
 ↓
User pack
 ↓
Temporary preview
```

Preview phải deterministic.

---

# 73. VISUAL ASSET PIPELINE

```text
Import
 ↓
Validate
 ↓
Hash
 ↓
Catalog
 ↓
Preview
 ↓
Assign
 ↓
Build
 ↓
Hash
 ↓
Install
 ↓
Backup
```

---

# 74. PNG PIPELINE

Validate:

```text
signature
IHDR
dimensions
bit depth
color type
file size
decode safety
```

Không tin extension.

---

# 75. LOGGING

Structured logging:

```text
trace
debug
info
warn
error
```

Fields:

```text
timestamp
target
event
instance
profile
task
session
errorCode
```

---

# 76. CRASH REPORTING LOCAL-FIRST

Không upload log người dùng mặc định.

Local:

```text
crash fingerprint
report
diagnostic bundle
```

User chọn:

```text
Export diagnostic ZIP
```

ZIP phải sanitize secrets.

---

# 77. DIAGNOSTIC EXPORT

Trước khi export:

```text
Remove:
tokens
refresh tokens
passwords
machine identifiers
private paths where possible
```

Cho user preview file list.

---

# 78. RESOURCE STUDIO DEBUG

Developer panel:

```text
Project schema
Build graph
Asset hash
Version resolver
Validator findings
Export files
Build duration
```

---

# 79. PACKET DEBUG SAFETY

Packet inspector mặc định:

```text
metadata only
```

Payload:

```text
manual reveal
```

Sensitive data:

```text
redact
```

Capture:

```text
memory ring
```

không tự lưu toàn bộ session vào disk.

---

# 80. OBSERVABILITY

Global developer overlay:

```text
FPS UI
render time
event rate
command latency
IPC latency
Rust CPU
Rust memory
queue depth
WebView memory if available
```

Toggle:

```text
Ctrl+Shift+D
```

---

# 81. FEATURE FLAGS

```text
resourceStudio
totem3d
networkLab
packetInspector
runtimeMonitor
rustCore
pythonLegacy
newProfiles
```

Mỗi feature có:

```text
enabled
experimental
fallback
```

---

# 82. MIGRATION BATCHES

## Batch 0 — Audit
- [ ] Freeze current source.
- [ ] Record test baseline.
- [ ] Record existing commands/events.
- [ ] Record data schemas.
- [ ] Record screenshots.
- [ ] Record benchmark baseline.

## Batch 1 — Tauri shell
- [ ] Create Tauri app.
- [ ] Vue + TypeScript + Vite.
- [ ] WebView2.
- [ ] DevTools.
- [ ] window state.
- [ ] splash.
- [ ] PNG branding.

## Batch 2 — Design system
- [ ] tokens.
- [ ] spatial surfaces.
- [ ] glass.
- [ ] neumorphic controls.
- [ ] responsive layout.
- [ ] dark mode.
- [ ] reduced motion.

## Batch 3 — Typed API
- [ ] command schema.
- [ ] event schema.
- [ ] TypeScript types.
- [ ] Rust command layer.
- [ ] error codes.

## Batch 4 — Legacy bridge
- [ ] Python sidecar.
- [ ] compatibility adapter.
- [ ] profile.
- [ ] resource.
- [ ] mods.
- [ ] optimization.

## Batch 5 — Dashboard/Play
- [ ] Dashboard.
- [ ] Play workspace.
- [ ] Instance selector.
- [ ] Preflight.
- [ ] runtime status.

## Batch 6 — Instances/Profiles
- [ ] instance UI.
- [ ] profile UI.
- [ ] profile diff.
- [ ] rollback.
- [ ] import/export.

## Batch 7 — Mods
- [ ] Mod manager.
- [ ] dependency graph.
- [ ] compatibility.
- [ ] security scan.
- [ ] quarantine.

## Batch 8 — Resource Studio
- [ ] editor shell.
- [ ] asset library.
- [ ] pack wizard.
- [ ] validator.
- [ ] build/install.

## Batch 9 — 3D/Visual
- [ ] Three.js.
- [ ] Totem editor.
- [ ] Crosshair.
- [ ] HUD.
- [ ] FX.

## Batch 10 — Optimization
- [ ] game optimization.
- [ ] system optimization.
- [ ] benchmark.
- [ ] rollback.

## Batch 11 — Network Lab
- [ ] DNS.
- [ ] TCP.
- [ ] MC status ping.
- [ ] RTT.
- [ ] jitter.
- [ ] network timeline.

## Batch 12 — Runtime/Packet
- [ ] companion schema v2.
- [ ] runtime events.
- [ ] packet metadata.
- [ ] ring buffer.
- [ ] inspector.

## Batch 13 — Diagnostics
- [ ] log center.
- [ ] crash analyzer.
- [ ] evidence.
- [ ] repair.
- [ ] diagnostic export.

## Batch 14 — Rust migration
- [ ] downloads. (phase 1 skeleton: state machine §103 + checksum + dedup — crate
  `antares-downloads`; phase 2: sha1/sha256 + atomic commit; phase 3: HTTP/1.1 engine
  + PartInfo resume/range + pipeline download() parity DownloadManager; phase 4:
  streaming get_stream 64KB + sha incremental + TLS seam)
- [x] storage. (crate `antares-storage` — scoped roots/sandbox/atomic từ Milestone 4)
- [ ] process. (phase 1 skeleton: record + cleanup policy §113 — crate `antares-process`;
  phase 2: spawn/wait/stop thật std::process)
- [ ] Minecraft launch. (phase 1 skeleton: session §112 + preflight + argument builder —
  crate `antares-launch`; phase 2: JavaResolver + ArtifactResolver §104; phase 3:
  planner required_java_major §108 + plan_launch một bước; phase 4: ExitAnalyzer §116
  + CompanionPairing parity write_pairing_for_instance)
- [ ] Java manager. (phase 1 skeleton: model §110 + resolve order + parse version — crate
  `antares-java`; phase 3: discovery scan dirs theo OS + JAVA_HOME/PATH + detect_major
  `java -showversion` parity discovery.py)
- [ ] network. (phase 1 skeleton: rtt stats parity + PacketRing §122 — crate `antares-net`;
  phase 2: MC Server List Ping §119; phase 3: tcp_check/probe/dns/check_endpoints
  parity handle_net_*; phase 4: Mojang manifest + DiskCache TTL parity ManifestService)
- [ ] profiles. (phase 1 skeleton: GAME_KEYS/coerce parity + diff §107 — crate
  `antares-profiles`; phase 2: write_options_merged parity _write_options; phase 4:
  ProfileStore CRUD qua antares-storage + sanitize/validate spec — crate đã vào
  workspace members)

## Batch 15 — Legacy removal
- [ ] parity check.
- [ ] disable Python.
- [ ] remove bridge.
- [ ] cleanup.
- [ ] benchmark.

## Batch 16 — Release
- [ ] Windows build.
- [ ] NSIS.
- [ ] portable.
- [ ] updater.
- [ ] signing.
- [ ] clean-machine test.

---

# 83. DEFINITION OF DONE

Feature chỉ được đánh `[x]` khi:

```text
Code
Tests
UI
Error state
Loading state
Empty state
i18n
Accessibility
Performance
Security
Rollback
Docs
```

đã có.

---

# 84. RELEASE GATE

## Core

- [ ] no unbounded event queue
- [ ] no blocking UI thread
- [ ] no memory leak
- [ ] no orphan process
- [ ] no stale IPC session
- [ ] no unsafe path write

## Minecraft

- [ ] latest supported release launches
- [ ] at least one current Fabric loader works
- [ ] Java auto-detection works
- [ ] mods install/remove
- [ ] resource pack build/install
- [ ] companion connects
- [ ] crash analyzer works

## UI

- [ ] WebView2
- [ ] DevTools
- [ ] keyboard navigation
- [ ] reduced motion
- [ ] low-end mode
- [ ] no console errors in normal flow

## Packaging

- [ ] portable
- [ ] installer
- [ ] uninstall
- [ ] update
- [ ] clean machine

---

# 85. FINAL TARGET TREE

```text
Antares
│
├── Dashboard
├── Play
├── Instances
├── Mods
├── Resource Studio
│   ├── Packs
│   ├── Assets
│   ├── Textures
│   ├── Models
│   ├── Totem 3D
│   └── Build
│
├── Visual Studio
│   ├── Crosshair
│   ├── HUD
│   ├── FX
│   └── Presets
│
├── Optimization
│   ├── Game
│   ├── System
│   └── Benchmark
│
├── Network Lab
│   ├── Connection
│   ├── Server Ping
│   ├── Packet Inspector
│   └── Download Network
│
├── Runtime
│   ├── FPS
│   ├── Frame time
│   ├── Player
│   ├── World
│   └── Network
│
├── Diagnostics
│   ├── Logs
│   ├── Crashes
│   ├── Mod Conflicts
│   └── Repair
│
├── Profiles
├── Servers
├── Backups
├── Plugins
└── Settings
```

---

# 86. KẾT LUẬN KIẾN TRÚC

## Chốt

**UI mới:**

```text
Tauri 2
+
Vue 3
+
TypeScript
+
Vite
```

**Core mới:**

```text
Rust
```

**Minecraft runtime:**

```text
Fabric Companion
```

**Migration:**

```text
Python legacy sidecar
```

**3D:**

```text
Three.js
```

**Storage:**

```text
JSON + SQLite + filesystem
```

**Build:**

```text
GitHub Codespaces
+
GitHub Actions Windows
```

**UI style:**

```text
Spatial Glass
+
Controlled Neumorphism
+
Antares PNG branding
```

**Tư duy tối ưu:**

```text
Measure
→ Diagnose
→ Plan
→ Backup
→ Apply
→ Verify
→ Benchmark
→ Rollback
```

Đây là điểm quan trọng nhất của bản remake: Antares không nên chỉ là một launcher có giao diện đẹp. Nó phải trở thành một **Minecraft Control Center**: launcher + instance manager + mod manager + resource editor + optimization center + runtime monitor + network diagnostics + packet observability + repair system trong cùng một kiến trúc có type, event, rollback và performance budget.


---

# ANTARES LAUNCHER 4.0 — DEEP RESTRUCTURE / STABILITY MASTER

> **Mục tiêu của phần 4.0:** biến kế hoạch 3.x thành một blueprint triển khai thực tế, trong đó mọi subsystem đều có owner, lifecycle, data contract, threading model, failure mode, recovery strategy, performance budget, test gate và migration path.
>
> **Nguyên tắc:** không rewrite toàn bộ trong một lần. Trước tiên giữ nguyên behavior hiện tại, tạo boundary mới, migrate từng domain, benchmark trước/sau, sau đó mới loại bỏ legacy.

---

# 87. KẾT QUẢ AUDIT THỰC TẾ CỦA SOURCE HIỆN TẠI

## 87.1 Baseline hiện tại

Source archive hiện tại có:

```text
Python files                     178
Compiled __pycache__ files       175
Frontend JS files                 51
Frontend CSS files                 6
Markdown docs                     18
Java companion files               3
```

Test suite hiện tại:

```text
254 tests collected
254 passed
8.76 seconds
```

Đây là một lợi thế rất lớn: **không được phá behavioral baseline này**.

## 87.2 Artifact hygiene cần sửa ngay

Source archive hiện tại còn chứa:

```text
__pycache__/
*.pyc
dist/Antares-1.0.0-source.zip
```

Tái cấu trúc phải thêm `.gitignore`/release ignore:

```gitignore
**/__pycache__/
*.py[cod]
*.pyd
.pytest_cache/
.mypy_cache/
.ruff_cache/
node_modules/
dist/
build/
coverage/
*.log
*.tmp
```

`dist/Antares-1.0.0-source.zip` không được nằm trong source tree chính.

## 87.3 Hotspot file quá lớn

Các điểm nóng cần tách trước khi thêm tính năng:

```text
api/bridge/api.py                         ~61 KB
services/profiles/service.py              ~23 KB
services/mods/scanner/jar_reader.py      ~18 KB
services/skins/service.py                ~18 KB
services/mods/scanner/scanner.py         ~17 KB
services/resources/importer.py           ~16 KB
services/visuals/service.py              ~14 KB
services/repair/service.py               ~13 KB
services/plugins/sandbox.py              ~13 KB
services/optimization/service.py         ~13 KB
services/resources/assets.py             ~12 KB
services/plugins/loader.py               ~12 KB
```

Mục tiêu không phải “chia file cho nhỏ bằng mọi giá”. Mục tiêu là mỗi file chỉ có **một lý do thay đổi**.

---

# 88. CÁC VẤN ĐỀ KIẾN TRÚC THỰC SỰ CẦN GIẢI QUYẾT

## 88.1 `AppContext._extra` là service locator

Hiện tại pattern:

```python
ctx.set("runtime", runtime)
ctx.set("profiles", profiles)
ctx.set("mods", mods)
```

giúp bootstrap nhanh nhưng về dài hạn có nhược điểm:

```text
string key
↓
runtime lookup
↓
None / typo / wrong type
```

### Kiến trúc mới

Rust Core sẽ dùng state container typed:

```rust
AppState {
    config: ConfigStore,
    events: EventHub,
    tasks: TaskRegistry,
    minecraft: MinecraftService,
    profiles: ProfileService,
    downloads: DownloadService,
    resources: ResourceService,
    diagnostics: DiagnosticsService,
}
```

Mỗi service có trait rõ ràng.

---

## 88.2 `AntaresApi` đang quá lớn

Không để một API class 60 KB làm cổng cho toàn bộ app.

Chia thành:

```text
commands/
├── app.rs
├── accounts.rs
├── instances.rs
├── profiles.rs
├── mods.rs
├── downloads.rs
├── minecraft.rs
├── resources.rs
├── visuals.rs
├── optimization.rs
├── network.rs
├── diagnostics.rs
├── servers.rs
├── plugins.rs
└── settings.rs
```

Mỗi command group chỉ map request → domain service → response.

Không để command handler chứa business logic.

---

## 88.3 EventBus hiện tại cần typed envelope + QoS

Event hiện tại có:

```text
name
payload: dict
event_version
timestamp
request_id
task_id
```

Nhưng `payload: dict` khiến contract yếu.

Kiến trúc mới:

```text
EventEnvelope<T>
├── id
├── schema
├── topic
├── timestamp
├── correlation_id
├── session_id
├── source
├── priority
├── qos
└── payload<T>
```

QoS:

```text
LATEST
COALESCE
BATCHED
LOSSLESS
DURABLE
```

---

# 89. THREADED / ASYNC MODEL MỚI

Không để một kiểu concurrency xử lý mọi việc.

## 89.1 Main UI thread

Chỉ:

```text
Vue rendering
input
navigation
animation scheduling
small state updates
```

Không:

```text
hashing
ZIP extraction
JAR scanning
large JSON parsing
Minecraft install
HTTP downloads
```

## 89.2 Rust async runtime

Dùng cho:

```text
HTTP
metadata
downloads
IPC
network diagnostics
parallel I/O
```

## 89.3 Blocking worker pool

Dành cho:

```text
SHA-256 lớn
ZIP build
JAR scan
image processing
resource atlas
filesystem traversal
```

## 89.4 Dedicated process supervisor

Dành cho:

```text
Minecraft Java
Python migration sidecar
server processes
helper utilities
```

Sơ đồ:

```text
Vue UI
  │
  ▼
Tauri commands
  │
  ├── async executor ── network / downloads / IPC
  ├── CPU pool ───────── hash / scan / build
  └── supervisor ─────── Java / sidecars
```

---

# 90. TASK SYSTEM 2.0

Mọi operation dài phải trở thành task.

```text
Task
├── id
├── kind
├── parent_id
├── state
├── progress
├── message
├── started_at
├── updated_at
├── finished_at
├── cancellable
├── retryable
├── result
└── error
```

State machine:

```text
QUEUED
  ↓
RUNNING
  ├── CANCEL_REQUESTED → CANCELLED
  ├── PAUSED → RUNNING
  ├── RETRYING → RUNNING
  ├── FAILED
  └── COMPLETED
```

Không dùng `thread.join()` từ UI.

---

# 91. TASK QUEUE / PRIORITY

Queue:

```text
P0 CRITICAL
P1 USER ACTION
P2 BACKGROUND
P3 PREFETCH
```

Khi Minecraft đang chạy:

```text
P0/P1 vẫn hoạt động
P2 giảm tốc
P3 pause hoặc cancel
```

Ví dụ:

```text
Minecraft launch      P1
Download required mod P1
Crash analysis        P1
Version metadata      P2
Thumbnail prefetch    P3
Catalog rebuild       P3
```

---

# 92. CPU / IO BUDGET

## Launcher idle

Mục tiêu:

```text
CPU average: near-idle
No continuous busy loop
No timer storm
No 1-second full filesystem scan
```

## Trong Minecraft session

Launcher chuyển sang:

```text
background-light mode
```

Telemetry default:

```text
System samples: 2–5 s
Game telemetry: companion decides safe cadence
UI chart: 4–10 visual updates/s maximum
```

Raw event rate có thể cao hơn nhưng **UI update rate không được cao theo raw event rate**.

---

# 93. UI EVENT INGESTION PIPELINE

```text
Rust events
   ↓
Event multiplexer
   ↓
QoS policy
   ↓
Per-topic buffer
   ↓
Frame scheduler
   ↓
Pinia patch
   ↓
Vue render
```

Ví dụ FPS 60 samples/s:

```text
60 samples
↓
ring buffer
↓
window aggregation
↓
UI receives ~8 points/s
```

Không commit Pinia 60 lần/s nếu người dùng chỉ nhìn thấy một chart.

---

# 94. PINIA STATE ARCHITECTURE

Không có `one giant store`.

```text
stores/
├── app.store.ts
├── navigation.store.ts
├── session.store.ts
├── notifications.store.ts
├── downloads.store.ts
├── instances.store.ts
├── profiles.store.ts
├── mods.store.ts
├── runtime.store.ts
├── diagnostics.store.ts
├── resource-studio.store.ts
├── visual-studio.store.ts
├── network.store.ts
└── settings.store.ts
```

Nguyên tắc:

```text
server state ≠ UI state
persistent state ≠ transient state
telemetry ≠ settings
```

---

# 95. DATA FLOW CHUẨN

## 95.1 Read

```text
Vue
 ↓
store action
 ↓
Tauri command
 ↓
Rust service
 ↓
storage/cache
 ↓
DTO
 ↓
store
```

## 95.2 Write

```text
UI form
 ↓
validation
 ↓
command
 ↓
plan
 ↓
confirmation
 ↓
mutation
 ↓
verify
 ↓
event
 ↓
store patch
```

Không cho frontend sửa file trực tiếp.

---

# 96. REQUEST / RESPONSE CONTRACT

Chuẩn hóa:

```json
{
  "requestId": "req_x",
  "command": "profile.apply",
  "schema": 1,
  "payload": {}
}
```

Response:

```json
{
  "requestId": "req_x",
  "ok": true,
  "data": {},
  "warnings": []
}
```

Error:

```json
{
  "requestId": "req_x",
  "ok": false,
  "error": {
    "code": "INSTANCE_LOCKED",
    "message": "...",
    "retryable": true,
    "action": "OPEN_INSTANCE"
  }
}
```

Không throw raw stack trace vào UI.

---

# 97. CORRELATION / TRACE ID

Mọi operation dài có:

```text
requestId
taskId
sessionId
instanceId
profileId
```

Ví dụ launch:

```text
request req_123
 task task_456
  instance survival
  session game_789
```

Khi crash:

```text
game session
→ launch task
→ downloaded file
→ diagnostic report
```

đều truy ngược được.

---

# 98. STORAGE LAYER 2.0

## 98.1 Không để service tự mở file lung tung

Tất cả qua:

```text
StorageService
```

API:

```text
read_json
write_json_atomic
read_bytes
write_bytes_atomic
move_atomic
copy_safe
remove_safe
list_scoped
```

## 98.2 Scoped roots

```text
AppDataRoot
InstancesRoot
CacheRoot
LogsRoot
ProfilesRoot
ResourceRoot
PluginRoot
```

Service chỉ được cấp root mà nó cần.

---

# 99. TRANSACTION / ATOMIC MUTATION

Các operation như:

```text
profile apply
mod update
resource install
optimization apply
repair
```

đều phải có transaction:

```text
preflight
→ snapshot
→ mutate
→ verify
→ commit
```

Nếu fail:

```text
rollback
→ verify rollback
→ emit recovery event
```

---

# 100. CONFIGURATION 2.0

Chia:

```text
settings.json
profile/*.json
instance/instance.json
runtime/session.json
```

Version:

```json
{
  "schema": 4,
  "data": {}
}
```

Migration:

```text
v1 → v2 → v3 → v4
```

Không migrate trực tiếp mọi version nếu làm migration graph quá phức tạp; tạo đường tuần tự có test.

---

# 101. CACHE ARCHITECTURE

Ba tầng:

```text
L1 memory
L2 disk
L3 remote
```

Ví dụ version manifest:

```text
memory 30–120s
↓
disk TTL
↓
remote
```

## Cache key phải deterministic

```text
provider
resource
version
locale
hash
```

## Cache cleanup

```text
LRU
TTL
size limit
pinned entries
```

---

# 102. HTTP ENGINE 2.0

Một shared client thay vì mỗi service tự tạo client.

```text
HttpClient
├── connection pool
├── timeout policy
├── proxy
├── retry policy
├── backoff
├── user agent
├── cache hooks
└── metrics
```

Retry chỉ cho:

```text
connection reset
429
5xx
```

Không retry mù cho:

```text
401
403
404
invalid payload
```

---

# 103. DOWNLOAD ENGINE 2.0

## State machine

```text
DISCOVER
→ PREPARE
→ DOWNLOAD
→ VERIFY
→ COMMIT
```

Lỗi:

```text
DOWNLOAD
 ├── retry
 ├── pause
 └── cancel
```

Có:

```text
resume
range
checksum
partial file
atomic rename
source fallback
```

## Download deduplication

Nếu hai instance cần cùng file:

```text
request A
request B
     ↓
shared artifact
     ↓
one physical download
```

---

# 104. MINECRAFT ARTIFACT STORE

Tách:

```text
Global artifacts
 ├── versions
 ├── libraries
 ├── assets
 ├── natives
 └── loaders
```

với:

```text
Instance mutable state
```

Không duplicate hàng trăm MB cho mỗi instance nếu có thể dùng shared immutable artifacts.

---

# 105. INSTANCE STORAGE DESIGN

```text
instances/
  <id>/
    instance.json
    game/
      saves/
      mods/
      resourcepacks/
      shaderpacks/
      logs/
      config/
```

Không để cache global trong game directory.

---

# 106. PROFILE LAYERING MODEL

Profile không copy cả instance.

```text
Base Instance
   + Account
   + JVM preset
   + Mod collection
   + Resource collection
   + Visual preset
   + Network preference
   + Launch target
```

Profile apply = patch, không clone.

---

# 107. PROFILE DIFF ENGINE

```text
current
vs
profile desired
```

Output:

```text
+ JVM max memory 4096 → 6144
+ resource pack X
- mod Y
~ render distance 12 → 16
```

UI có:

```text
Preview changes
Apply
Cancel
```

---

# 108. MINECRAFT VERSION RESOLVER

Không để `if version >= ...` xuất hiện trong 20 file.

Tạo:

```text
MinecraftVersionCapability
```

Ví dụ:

```rust
struct VersionCapabilities {
    java_major: u8,
    pack_format_mode: PackFormatMode,
    item_model_mode: ItemModelMode,
    crosshair_path: CrosshairPath,
    loader_support: LoaderSupport,
}
```

Service hỏi capability registry thay vì tự parse.

---

# 109. LOADER ABSTRACTION

```text
Loader
├── Vanilla
├── Fabric
├── NeoForge
├── Forge
└── Quilt
```

Mỗi loader:

```text
metadata source
version resolver
installer
launch patcher
compatibility
```

UI chỉ thấy interface chung.

---

# 110. JAVA RUNTIME MODEL

```text
JavaRuntime
├── source
├── path
├── major
├── minor
├── architecture
├── vendor
├── verified
└── capabilities
```

Resolution:

```text
instance explicit
↓
profile explicit
↓
managed runtime
↓
Mojang runtime
↓
system Java
```

---

# 111. MINECRAFT LAUNCHER ORCHESTRATOR 2.0

Tách thành:

```text
LaunchPlanner
LaunchPreflight
ArtifactResolver
JavaResolver
ArgumentBuilder
CompanionPairing
ProcessSupervisor
LaunchSession
ExitAnalyzer
```

Không để `orchestrator.py` tự làm tất cả.

---

# 112. LAUNCH SESSION STATE MACHINE

```text
IDLE
 ↓
PREFLIGHT
 ↓
RESOLVING
 ↓
DOWNLOADING
 ↓
INSTALLING
 ↓
PAIRING
 ↓
SPAWNING
 ↓
RUNNING
 ↓
EXITING
 ↓
ANALYZING
 ↓
COMPLETED / CRASHED
```

UI progress lấy từ state machine này.

---

# 113. PROCESS SUPERVISOR 2.0

Mỗi child process có:

```text
pid
owner
instance
started_at
command fingerprint
stdout handle
stderr handle
exit state
```

Supervisor phải đảm bảo:

```text
launcher close
→ child cleanup policy
```

Nhưng không kill process user-owned ngoài scope.

---

# 114. STDOUT/STDERR PIPELINE

Không gửi từng dòng trực tiếp lên Vue.

```text
process pipe
 ↓
line decoder
 ↓
ring buffer
 ↓
log classifier
 ↓
batcher
 ↓
UI
```

UI log batching:

```text
max 100 lines/frame batch
```

---

# 115. LOG STORAGE

Giữ:

```text
ring buffer memory
```

và:

```text
rotating files
```

Không giữ infinite memory log.

---

# 116. CRASH ANALYZER 2.0

Input:

```text
latest.log
crash-report
launcher stderr
JVM stderr
runtime telemetry
mod list
version
loader
```

Pipeline:

```text
ingest
→ normalize
→ fingerprint
→ classify
→ rank evidence
→ recommendation
```

Không kết luận nếu evidence yếu.

---

# 117. ERROR TAXONOMY

```text
AUTH_*
JAVA_*
MC_*
LOADER_*
MOD_*
RESOURCE_*
NETWORK_*
DISK_*
PROCESS_*
IPC_*
UI_*
CONFIG_*
SECURITY_*
PLUGIN_*
```

Mỗi error có:

```text
code
severity
retryable
user_action
technical_action
safe_message
```

---

# 118. NETWORK STACK 2.0

Chia:

```text
network/
├── dns
├── tcp
├── http
├── minecraft_status
├── runtime_metrics
├── packet_capture
├── packet_decode
└── policy
```

Không trộn launcher download network với Minecraft gameplay network.

---

# 119. MINECRAFT STATUS PROBE

Flow:

```text
DNS
 ↓
TCP
 ↓
Handshake
 ↓
Status request
 ↓
Response
 ↓
Parse
```

Metrics:

```text
DNS ms
connect ms
status ms
total ms
```

---

# 120. RUNTIME NETWORK TELEMETRY

Companion gửi:

```text
connected
server
rtt
jitter
packet counts
connection state
disconnect reason
```

Không gửi payload packet đầy đủ ở chế độ realtime mặc định.

---

# 121. PACKET INSPECTOR 2.0

Ba mode:

```text
OFF
METADATA
DEBUG PAYLOAD
```

Metadata:

```text
timestamp
direction
packetId
name
size
```

Payload mode:

```text
manual start
manual stop
memory bounded
redaction
```

---

# 122. PACKET RING BUFFER

```text
10k default
100k developer
```

Eviction:

```text
oldest first
```

Không ghi disk liên tục.

Export chỉ khi user yêu cầu.

---

# 123. RESOURCE STUDIO 2.0 WORKSPACE

```text
┌────────────────────────────────────────────────────────────┐
│ Project / version / build status                           │
├───────────────┬──────────────────────────┬─────────────────┤
│ Asset Explorer│ Preview / Editor         │ Inspector       │
│               │                          │                 │
│ folders       │ texture / model / 3D     │ metadata        │
│ tags          │                          │ validation      │
├───────────────┴──────────────────────────┴─────────────────┤
│ Build / Validation / Tasks / Problems                       │
└────────────────────────────────────────────────────────────┘
```

---

# 124. RESOURCE PROJECT STATE

```ts
interface ResourceProject {
  id: string
  name: string
  targetVersion: string
  schema: number
  assets: AssetRef[]
  layers: PackLayer[]
  build: BuildSettings
  visual: VisualSettings
  updatedAt: number
}
```

Draft phải autosave.

---

# 125. RESOURCE ASSET STORE

Không copy duplicate cùng PNG 20 lần.

Dùng content-addressed:

```text
assets/sha256/ab/cd/abcdef...
```

Metadata DB:

```text
id
hash
mime
size
width
height
tags
created_at
```

---

# 126. RESOURCE BUILD GRAPH

```text
Project
 ↓
Resolve assets
 ↓
Resolve version capabilities
 ↓
Generate metadata
 ↓
Generate models
 ↓
Generate atlas
 ↓
Validate
 ↓
Package
 ↓
Hash
 ↓
Artifact
```

Build node phải incremental.

Nếu chỉ sửa một texture:

```text
không rebuild toàn bộ metadata nếu không cần
```

---

# 127. RESOURCE VALIDATION LEVELS

```text
ERROR
WARNING
INFO
```

Error ví dụ:

```text
path traversal
invalid JSON
broken PNG
invalid model
invalid pack metadata
```

Warning:

```text
future version
nonstandard texture size
unknown asset path
```

---

# 128. ZIP IMPORT HARDENING

Bắt buộc:

```text
normalize path
reject ..
reject absolute path
reject drive letter
reject symlink
entry count limit
uncompressed size limit
single entry limit
CRC check
duplicate normalized path check
```

Import qua:

```text
staging directory
```

chỉ commit sau khi inspect hoàn thành.

---

# 129. TOTEM 3D ENGINE

Engine nên là domain-independent:

```text
ModelDocument
Geometry
Material
Texture
Camera
Viewport
```

Không để Vue component tự hiểu Minecraft JSON format.

Pipeline:

```text
Document
↓
Scene graph
↓
Renderer
↓
Minecraft exporter
```

---

# 130. TOTEM 3D RENDERER

Render quality levels:

```text
LOW
MEDIUM
HIGH
```

Developer/editor mode:

```text
wireframe
bounds
axis
face labels
UV debug
```

---

# 131. THREE.JS MEMORY MANAGEMENT

Mỗi scene phải có:

```text
disposeGeometry
 disposeMaterial
 disposeTexture
 disposeRenderTarget
 disposeScene
```

Khi rời tab:

```text
pause render loop
remove event listeners
dispose GPU resources
cancel animation
```

Đây là bắt buộc để tránh memory leak sau khi mở/đóng editor nhiều lần.

---

# 132. RESOURCE PREVIEW THROTTLING

Khi kéo slider:

```text
input events
 ↓
RAF debounce
 ↓
preview
```

Không rebuild ZIP mỗi keypress.

---

# 133. VISUAL STUDIO

Gộp logic editor:

```text
Visual Studio
├── Crosshair
├── HUD
├── Totem
├── FX
└── Presets
```

Một common document model:

```text
VisualDocument
├── schema
├── target
├── elements
├── layout
├── assets
└── metadata
```

---

# 134. CROSSHAIR EDITOR

State:

```text
shape
size
thickness
gap
outline
opacity
color
center
```

Preview engine dùng Canvas.

Không render qua DOM shape hàng nghìn node.

---

# 135. HUD EDITOR

Canvas/editor model:

```text
HUDDocument
├── widgets
├── anchors
├── positions
├── scale
├── opacity
└── visibility
```

Snap:

```text
screen center
edges
safe area
8px grid
```

---

# 136. PERFORMANCE CENTER — 3 LAYER MODEL

```text
System
Minecraft JVM
Minecraft Runtime
```

System:

```text
CPU
RAM
Disk
GPU
network
```

JVM:

```text
heap
GC
threads
process CPU
```

Minecraft:

```text
FPS
frametime
1% low
0.1% low
chunks
entities
network
```

---

# 137. PERFORMANCE SESSION

```text
session
├── hardware snapshot
├── instance snapshot
├── mod hash
├── profile
├── baseline
├── after
└── annotations
```

Annotate event:

```text
mod installed
resource pack changed
render distance changed
optimization applied
```

---

# 138. BENCHMARK ENGINE

Mỗi benchmark có:

```text
warmup
measurement
cooldown
```

Không compare frame rate khi game chưa warm-up.

Kết quả:

```text
mean
median
p95 frame time
p99 frame time
1% low
0.1% low
```

---

# 139. OPTIMIZATION POLICY ENGINE

Thay `if/else` rải rác bằng rule model:

```text
Rule
├── id
├── scope
├── evidence
├── condition
├── action
├── risk
├── reversible
└── recommendation
```

Ví dụ:

```text
Rule: insufficient-memory
Evidence: available RAM
Action: suggest Xmx
Risk: low
Reversible: yes
```

---

# 140. OPTIMIZATION PLAN

```text
Scan
↓
Evidence
↓
Rules
↓
Plan
↓
Diff
↓
Backup
↓
Apply
↓
Verify
↓
Benchmark
```

Không có:

```text
Optimize now
```

mà không preview nếu mutation là đáng kể.

---

# 141. SYSTEM OPTIMIZATION SAFETY TIERS

```text
SAFE
CAREFUL
EXPERIMENTAL
```

SAFE:

```text
cache cleanup
orphan cleanup
launcher-owned priority
```

CAREFUL:

```text
power profile suggestion
large cache purge
```

EXPERIMENTAL:

```text
OS-level changes
```

Experimental phải explicit opt-in.

---

# 142. WINDOWS INTEGRATION

Tạo layer:

```text
platform/windows/
├── process.rs
├── power.rs
├── filesystem.rs
├── security.rs
├── webview.rs
└── installer.rs
```

Không để WinAPI gọi trực tiếp từ feature service.

---

# 143. USER DATA / PRIVACY

Phân loại:

```text
PUBLIC
PRIVATE
SECRET
```

SECRET:

```text
refresh tokens
session tokens
IPC tokens
```

Không xuất vào diagnostics.

---

# 144. SECRET STORAGE

Credential path:

```text
OS secure storage
```

Filesystem chỉ lưu:

```text
credential reference
```

---

# 145. DIAGNOSTIC BUNDLE SANITIZER

Trước export:

```text
scan for:
Microsoft token patterns
Bearer tokens
API keys
IPC tokens
password-like keys
absolute private paths
```

User phải thấy preview files trước khi export.

---

# 146. PLUGIN SDK 2.0

Plugin có lifecycle:

```text
DISCOVERED
VALIDATED
LOADED
ENABLED
DISABLED
FAILED
UNLOADED
```

Permissions theo capability.

Không cấp process execute mặc định.

---

# 147. PLUGIN FAILURE ISOLATION

Một plugin lỗi:

```text
plugin crash
→ mark failed
→ unload
→ notification
```

Không được crash launcher.

---

# 148. Tauri CAPABILITY DESIGN

Không dùng capability wildcard cho production.

Tách:

```text
main-window.json
installer-window.json
plugin-window.json
resource-editor-window.json
```

Mỗi window chỉ có quyền cần thiết.

---

# 149. DEVTOOLS / DEBUG WORKFLOW

## Frontend

```text
Vue component
→ WebView2 DevTools
→ network
→ console
→ performance
```

## Rust

```text
requestId
→ tracing span
→ service log
→ task log
```

## Minecraft

```text
sessionId
→ companion events
→ runtime metrics
→ latest.log
```

Ba hệ thống phải nối được bằng correlation IDs.

---

# 150. DEVELOPER OVERLAY

Trong dev mode:

```text
FPS UI
render ms
Vue update rate
event rate
queue depth
pending tasks
IPC RTT
Rust command latency
WebView memory estimate
```

Không bật production.

---

# 151. STARTUP OPTIMIZATION

Startup phases:

```text
0. process start
1. core config
2. essential state
3. window paint
4. lightweight stores
5. background service init
6. optional registries
7. prefetch
```

Không làm:

```text
startup
↓
scan entire instances
↓
scan all mods
↓
load all thumbnails
↓
query every provider
```

---

# 152. LAZY LOADING

Heavy routes:

```text
Resource Studio
Visual Studio
Network Lab
Diagnostics
Servers
```

được lazy-load.

Khi user chưa mở:

```text
Three.js chưa load
chart engine chưa load
resource editor chưa load
```

---

# 153. CODE SPLITTING

Build chunks:

```text
core.js
ui-shell.js
resource-studio.js
visual-studio.js
network-lab.js
diagnostics.js
```

Không tạo giant JS bundle.

---

# 154. WEBVIEW PERFORMANCE

Không dùng:

```text
thousands of DOM shadows
huge blur backdrop
CSS filters trên toàn màn hình
continuous box-shadow animation
```

Glass chỉ ở surface cần thiết.

---

# 155. GLASSMORPHISM + NEUMORPHISM RULEBOOK

## Glass

Dùng cho:

```text
topbar
floating panels
modal
command palette
inspector
notification center
```

## Neumorphism

Dùng cho:

```text
toggle
slider
compact control
icon button
```

## Solid surfaces

Dùng cho:

```text
tables
log viewer
asset grid
mod list
diagnostics
```

---

# 156. VISUAL HIERARCHY

Mọi màn hình có 4 level:

```text
Level 1 — primary action
Level 2 — section
Level 3 — metadata
Level 4 — helper text
```

Accent chỉ dành cho:

```text
active
CTA
important status
selected
```

---

# 157. ANIMATION SYSTEM

Motion tokens:

```text
instant 80ms
fast 140ms
normal 220ms
slow 320ms
```

Animations:

```text
opacity
transform
clip-path where safe
```

Tránh animate:

```text
blur
large box-shadow
layout properties
```

Reduced motion:

```text
transition <= 80ms
or disabled
```

---

# 158. LOADING STATES

Mỗi feature có:

```text
initial
loading
empty
ready
error
stale
refreshing
```

Không dùng spinner cho toàn page nếu chỉ một card đang loading.

---

# 159. SKELETON SYSTEM

Skeleton chỉ nên dùng khi:

```text
layout known
response > 100ms
```

Nếu operation cực nhanh:

```text
không flash skeleton
```

---

# 160. ERROR UX

Error page không ghi:

```text
Something went wrong
```

Mà:

```text
Không thể tải Modrinth catalog
Reason: timeout
Retry
Work offline
Open diagnostics
```

---

# 161. OFFLINE-FIRST DESIGN

UI phải hoạt động khi provider chết.

Hiển thị:

```text
Offline
Using cached metadata
Last updated: ...
```

Không làm page blank.

---

# 162. MOD SYSTEM 2.0

Tách:

```text
ModRegistry
ModResolver
ModInstaller
ModScanner
ModCompatibility
ModUpdater
ModQuarantine
```

## Dependency graph

```text
Mod A
├── requires B
├── optional C
└── conflicts D
```

Resolver phải trả deterministic result.

---

# 163. MOD UPDATE PLAN

Không update bulk ngay.

```text
discover updates
→ compatibility graph
→ conflict simulation
→ backup
→ download staged
→ validate
→ apply
→ verify
```

---

# 164. MOD SECURITY LAYER

JAR scan gồm:

```text
manifest
entrypoints
classes/package names
URL indicators
process indicators
filesystem indicators
reflection indicators
embedded files
```

Đây là heuristic, không phải malware guarantee.

---

# 165. REPAIR ENGINE 2.0

Repair plan:

```text
Issue
↓
Evidence
↓
Safe action
↓
Preview
↓
Backup
↓
Apply
↓
Verify
```

Ví dụ:

```text
missing library
→ redownload exact hash
```

---

# 166. BACKUP ENGINE 2.0

Snapshot metadata:

```text
id
reason
scope
createdAt
files
size
hash
parentSnapshot
```

Deduplicate unchanged files nếu backend hỗ trợ content address.

---

# 167. ROLLBACK ENGINE

Rollback phải:

```text
check snapshot
lock scope
restore
verify hashes
release lock
emit event
```

Nếu restore thất bại:

```text
do not delete original backup
```

---

# 168. FILE LOCK MODEL

Lock scopes:

```text
instance
profile
resource-project
download-artifact
```

Không có global lock cho mọi thứ.

---

# 169. CONCURRENT USER ACTIONS

Ví dụ user click:

```text
Update Mod
```

hai lần.

Task dedupe:

```text
same resource + same desired version
→ attach to existing task
```

Không chạy duplicate downloads.

---

# 170. COMMAND IDEMPOTENCY

Một số command nên idempotent:

```text
refresh metadata
ensure artifact
apply profile desired state
install resource if exact hash exists
```

Nếu state đã đúng:

```text
NOOP
```

Không ghi file thừa.

---

# 171. NOTIFICATION ENGINE

Event rules:

```text
ERROR → notification
WARNING → coalesce
SUCCESS → toast + history
INFO → history only
```

Spam guard:

```text
same fingerprint within N seconds
→ increment count
```

---

# 172. DOWNLOAD CENTER UX

Global panel:

```text
Active
Queued
Completed
Failed
```

Task row:

```text
name
instance
progress
speed
ETA
status
cancel
retry
```

---

# 173. RESOURCE PACK INSTALL LAYERING

Layer priority phải deterministic:

```text
base
profile
user
preview
```

Apply mới phải cập nhật state atomically.

---

# 174. SERVER UI

Server subsystem không được block launcher.

Console virtualized.

Server process supervisor chung với Minecraft process supervisor nhưng:

```text
server
!=
client
```

scoped policy riêng.

---

# 175. PROFILE / SERVER / MOD / RESOURCE RELATION

```text
Profile
 ├─ Instance
 ├─ Account
 ├─ Java
 ├─ Mods
 ├─ Resource Packs
 ├─ HUD
 ├─ Crosshair
 ├─ Optimization
 └─ Launch target
```

Server là launch target, không phải core instance identity.

---

# 176. ACCOUNT ENGINE

Account types:

```text
Microsoft
Offline
```

Không để UI giữ access/refresh token lâu hơn cần thiết.

---

# 177. MICROSOFT AUTH SESSION MODEL

```text
UI starts auth
 ↓
Rust auth session
 ↓
credential store
 ↓
Minecraft profile
 ↓
credential reference returned to UI
```

UI chỉ nhận safe summary:

```text
displayName
uuid
account type
status
lastUsed
```

---

# 178. SETTINGS ARCHITECTURE

Settings chia:

```text
AppSettings
UiSettings
DownloadSettings
MinecraftSettings
OptimizationSettings
NetworkSettings
PrivacySettings
DeveloperSettings
```

UI section không biết JSON storage layout.

---

# 179. COMMAND PALETTE 2.0

Commands:

```text
Play instance
Open Resource Studio
Open Diagnostics
Optimize instance
Refresh mods
Create profile
Import resource pack
Open network lab
Open DevTools
Export diagnostics
```

Command result có:

```text
label
shortcut
availability
action
```

---

# 180. SEARCH INDEX

Indexer cho:

```text
instances
profiles
mods
resource projects
servers
commands
settings
```

Không scan filesystem mỗi search keystroke.

---

# 181. INTERNATIONALIZATION

Giữ:

```text
vi
 en
```

Nhưng chuyển sang typed translation keys:

```ts
$t('play.preflight.java.ok')
```

Không dùng string hardcode trong component mới.

---

# 182. TYPESCRIPT RULES

Bắt buộc:

```text
strict: true
noImplicitAny
noUncheckedIndexedAccess
exactOptionalPropertyTypes
verbatimModuleSyntax
```

Vue typecheck bằng `vue-tsc`.

Vue hiện có first-class TypeScript support và tài liệu chính thức khuyến nghị Vite + `vue-tsc` cho workflow Vue + TypeScript. citeturn707210search0

---

# 183. FRONTEND PACKAGE BASELINE

Source hiện chưa có frontend package manifest chuẩn.

Tạo:

```text
package.json
pnpm-lock.yaml
vite.config.ts
tsconfig.json
tsconfig.app.json
tsconfig.node.json
eslint.config.ts
```

Khuyến nghị package manager:

```text
pnpm
```

vì workspace/lockfile rõ và dependency management tốt cho project lớn.

---

# 184. FRONTEND DIRECTORY 2.0

```text
apps/desktop/src/
├── app/
│   ├── App.vue
│   ├── router/
│   ├── providers/
│   └── bootstrap/
├── features/
│   ├── dashboard/
│   ├── play/
│   ├── instances/
│   ├── mods/
│   ├── resources/
│   ├── visuals/
│   ├── optimization/
│   ├── network/
│   ├── runtime/
│   ├── diagnostics/
│   ├── profiles/
│   ├── servers/
│   └── settings/
├── shared/
│   ├── ui/
│   ├── composables/
│   ├── utils/
│   ├── types/
│   └── icons/
├── stores/
└── styles/
```

---

# 185. FEATURE OWNERSHIP

Mỗi feature có:

```text
components
pages
store
api
types
composables
validators
```

Ví dụ:

```text
features/resources/
├── pages/
├── components/
├── store/
├── api/
├── types/
├── validators/
└── composables/
```

Không cho resource code import sâu vào server internals.

---

# 186. RUST WORKSPACE

```text
src-tauri/
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── commands/
│   ├── state/
│   ├── events/
│   └── platform/
└── Cargo.toml

crates/
├── domain
├── core
├── storage
├── http
├── process
├── minecraft
├── downloads
├── mods
├── resources
├── visuals
├── optimization
├── network
├── diagnostics
├── accounts
├── plugins
└── telemetry
```

---

# 187. DOMAIN / INFRASTRUCTURE SEPARATION

Ví dụ resource:

```text
domain/resources
```

chỉ chứa:

```text
ResourceProject
Asset
PackTarget
BuildPlan
ValidationFinding
```

Không biết:

```text
Windows
Tauri
HTTP client implementation
SQLite driver
```

---

# 188. TRAITS / PORTS

Ví dụ:

```rust
trait ArtifactStore {}
trait MetadataProvider {}
trait ProcessSupervisor {}
trait Clock {}
trait SecureStore {}
```

Test dùng fake implementation.

---

# 189. TEST PYRAMID MỚI

```text
        E2E
      /     \
  Integration
 /           \
Unit / property
```

Mục tiêu:

```text
unit nhiều nhất
integration vừa đủ
E2E ít nhưng quan trọng
```

---

# 190. GIỮ NGUYÊN 254 TEST HIỆN TẠI

Giai đoạn migration:

```text
legacy tests
     ↓
compatibility layer
     ↓
new Rust implementation
```

Mỗi domain migrate phải đạt:

```text
old tests pass
new tests pass
benchmark no regression
```

Sau đó mới chuyển test ownership sang Rust/TS.

---

# 191. TEST CATEGORY BỔ SUNG

## Contract

```text
command schema
error schema
event schema
```

## Persistence

```text
migration
atomic write
corruption recovery
```

## Recovery

```text
partial download
interrupted build
crash during install
```

## Concurrency

```text
double click
parallel download
cancel during commit
```

---

# 192. PROPERTY-BASED TEST TARGETS

Các hàm deterministic:

```text
pack version parsing
path sanitation
profile diff
layer resolution
atlas packing
packet normalization
cache eviction
```

Test invariant:

```text
normalize(normalize(x)) == normalize(x)
```

---

# 193. GOLDEN FILE TESTS

Dùng cho:

```text
Minecraft JSON
resource pack metadata
model JSON
profile export
diagnostic export schema
```

Thay đổi output phải được review chủ động.

---

# 194. PERFORMANCE TEST MATRIX

### UI

```text
10k logs
10k mods
10k assets
1k notifications
high-frequency runtime events
```

### Core

```text
100MB hash
1GB download
10k asset catalog
100k packet ring
```

### Editor

```text
64-cube scene
4k texture
large asset tree
```

---

# 195. PERFORMANCE REGRESSION GATE

Mỗi PR quan trọng chạy:

```text
startup benchmark
command latency benchmark
event throughput
memory snapshot
resource build benchmark
```

Nếu regression vượt budget định nghĩa trước:

```text
PR blocked
```

---

# 196. MEMORY LEAK TEST

Đặc biệt:

```text
open Resource Studio 20x
open Totem 3D 20x
open Network Lab 20x
open Diagnostics 20x
```

Đo:

```text
RSS
WebView memory
GPU allocations
Rust heap
```

---

# 197. STRESS EVENT TEST

Kịch bản:

```text
1000 events/s
5 seconds
```

Yêu cầu:

```text
publisher does not block
UI stays responsive
coalescing works
critical events preserved
```

---

# 198. OFFLINE / FAILURE CHAOS TESTS

Mô phỏng:

```text
DNS unavailable
HTTP timeout
HTTP 500
partial file
corrupt JSON
locked file
permission denied
companion disconnect
Java missing
mod conflict
invalid ZIP
```

App không được “chết trắng”.

---

# 199. COMPANION IPC 2.0

## Session

```text
CONNECTING
AUTHENTICATING
READY
STALE
DISCONNECTED
```

## Heartbeat

Companion và launcher có heartbeat bounded.

Nếu stale:

```text
mark stale
stop processing runtime data
allow reconnect
```

---

# 200. IPC BACKPRESSURE

Nếu UI không consume nhanh:

```text
LATEST metrics → drop old
LOSSLESS diagnostic → queue bounded
PACKET payload → drop unless debug capture
```

Không để producer vô hạn queue.

---

# 201. IPC SECURITY

```text
loopback only
rotating token
schema version
message size limit
rate limit
session binding
```

Không bind `0.0.0.0` cho companion local mặc định.

---

# 202. GAME RUNTIME EVENT MODEL

Topics:

```text
runtime.session
runtime.performance
runtime.player
runtime.world
runtime.network
runtime.chat
runtime.error
runtime.packet
```

Tất cả có schema version.

---

# 203. FRAME TIME TELEMETRY

Không chỉ:

```text
FPS
```

Companion nên ưu tiên:

```text
frameMs
rolling fps
1% low estimate
0.1% low estimate
```

UI có thể tính histogram nhẹ.

---

# 204. GPU TELEMETRY

Launcher backend không nên giả định psutil có GPU metrics.

Thiết kế:

```text
GpuTelemetryProvider
├── Windows provider
├── companion provider
└── unavailable provider
```

Nếu metric không có:

```text
N/A
```

không giả số liệu.

---

# 205. RESOURCE STUDIO CACHE

Preview cache key:

```text
asset hash
renderer version
viewport settings
```

Nếu cùng asset + renderer config:

```text
reuse preview
```

---

# 206. THUMBNAIL PIPELINE

Không load 1000 ảnh full-size.

```text
source
↓
thumbnail worker
↓
webp/png thumbnail
↓
memory/disk cache
↓
virtualized grid
```

---

# 207. VIRTUALIZED UI

Bắt buộc cho:

```text
logs
mods
assets
servers
notifications
packets
```

---

# 208. FILESYSTEM SCANNING

Không scan synchronously khi mở tab.

```text
open tab
↓
show cached index
↓
background refresh
↓
patch changes
```

---

# 209. INDEXING STRATEGY

Watcher hoặc incremental scan khi phù hợp:

```text
changed file
→ rehash only file
```

Không recalculate SHA256 toàn thư mục sau mỗi thay đổi nhỏ.

---

# 210. HASHING STRATEGY

Hash large file:

```text
streaming
```

Chunk:

```text
1–8 MB
```

Không đọc toàn file vào RAM.

---

# 211. IMAGE PROCESSING

Import image:

```text
signature
metadata
size
decode
thumbnail
hash
```

GPU preview chỉ dùng data cần thiết.

---

# 212. RESOURCE PACK BUILD PARALLELISM

Parallel only independent tasks:

```text
texture validation ─┐
model validation   ─┼─> build graph
metadata generation─┘
```

Package final là single commit stage.

---

# 213. SAFE MODE

`--safe-mode` của app mới có ý nghĩa:

```text
disable plugins
disable experimental features
reduce animation
skip third-party integrations
```

Không disable core launcher.

---

# 214. RECOVERY MODE

Nếu app crash-loop:

```text
3 failed launches
→ offer safe mode
```

Không tự xóa user data.

---

# 215. SETTINGS CORRUPTION RECOVERY

Nếu `settings.json` hỏng:

```text
load last known backup
→ if invalid → default
→ preserve corrupted file as .broken
```

---

# 216. PROFILE CORRUPTION RECOVERY

Không xóa profile silently.

```text
invalid
→ mark damaged
→ isolate
→ offer repair/import
```

---

# 217. UPDATE SAFETY

Update flow:

```text
check
→ download
→ verify signature/hash
→ stage
→ install
→ restart
```

Nếu fail:

```text
remain on previous version
```

---

# 218. PORTABLE VS INSTALLED MODE

Installed:

```text
%APPDATA%\Antares
```

Portable:

```text
<data-dir cạnh app>
```

Runtime phải expose:

```text
StorageMode::Installed
StorageMode::Portable
```

Không để từng service tự đoán path.

---

# 219. WINDOWS PATH EDGE CASES

Test:

```text
Unicode username
spaces
long paths
non-ASCII drive path
UNC where relevant
```

Mọi service dùng canonical scoped path API.

---

# 220. RELEASE ARTIFACTS

```text
Antares-Setup-x64.exe
Antares-Portable-x64.zip
Antares-symbols.zip
Antares-checksums.txt
release-notes.md
```

Source package không chứa:

```text
__pycache__
dist nested zip
local caches
node_modules
```

---

# 221. CODESPACES ROLE

Codespaces dùng cho:

```text
frontend development
Rust development
Python migration
unit tests
lint
format
architecture work
```

Không giả định Codespaces là environment tốt nhất để test Windows-specific UX.

---

# 222. WINDOWS CI ROLE

`windows-latest` chịu trách nhiệm:

```text
Tauri build
WebView2 check
NSIS
portable packaging
installer smoke
file associations
process spawning
Windows API integration
```

Tauri trên Windows dựa trên Microsoft WebView2; tài liệu chính thức cũng yêu cầu Microsoft C++ Build Tools và WebView2 cho development. citeturn707210search6

---

# 223. TAURI SIDEcar MIGRATION

Trong migration có thể bundle Python worker thành sidecar.

```text
Tauri
  ↓
Rust Core
  ↓
Legacy Python sidecar
```

Tauri hỗ trợ external binaries/sidecars thông qua bundle `externalBin`, phù hợp để chuyển đổi từng subsystem mà không phải rewrite toàn bộ ngay lập tức. citeturn707210search2turn707210search3

---

# 224. TAURI CAPABILITY SECURITY

Frontend chỉ được gọi những command đã cấp quyền.

Capabilities được tách theo window/role thay vì một permission set khổng lồ. Tauri cung cấp hệ thống capability/permission để giới hạn exposure của frontend tới native commands. citeturn707210search4turn707210search1

---

# 225. TẠI SAO KHÔNG CẦN `app/ui_server.py` SAU MIGRATION

Hiện tại:

```text
Python
→ ThreadingHTTPServer
→ 127.0.0.1
→ pywebview
```

Kiến trúc mới:

```text
Tauri
→ bundled frontend assets
→ WebView2
```

Kết quả:

```text
không ephemeral localhost port
không SimpleHTTPRequestHandler
không frontend HTTP bootstrap riêng
```

Chỉ giữ dev server Vite trong development.

---

# 226. TẠI SAO KHÔNG GIỮ PYWEBVIEW LÀ UI CHÍNH

Không phải vì pywebview “không chạy được”. Source hiện tại đã chứng minh nó chạy được.

Nhưng mục tiêu 4.0 cần:

```text
typed frontend
native Rust core
capability security
strong DevTools workflow
code splitting
plugin/editor isolation
```

Vì vậy pywebview nên dừng ở migration compatibility, không phải kiến trúc đích.

---

# 227. MIGRATION ORDER — KHÔNG ĐƯỢC ĐẢO

```text
1. Freeze baseline
2. Clean source tree
3. Create Tauri/Vue shell
4. Create typed contracts
5. Create Rust core skeleton
6. Python bridge adapter
7. Migrate storage/config
8. Migrate events/tasks
9. Migrate downloads
10. Migrate Java
11. Migrate Minecraft launch
12. Migrate profiles
13. Migrate instances
14. Migrate mods
15. Migrate resources
16. Migrate visuals
17. Migrate optimization
18. Migrate diagnostics/network
19. Companion V2
20. Remove pywebview
21. Remove Python legacy
```

---

# 228. BATCH SIZE RULE

Mỗi implementation batch:

```text
5–10 production files
```

Ngoại lệ:

```text
mechanical migration
```

nhưng vẫn phải compile/test sau mỗi batch.

---

# 229. MILESTONE 0 — BASELINE FREEZE

- [ ] Capture current test result: 254/254.
- [ ] Export current UI screenshots.
- [ ] Save test fixtures.
- [ ] Save known working Minecraft instance fixture.
- [ ] Save Resource Studio fixture.
- [ ] Save Totem fixture.
- [ ] Save profile fixture.
- [ ] Save diagnostics fixture.
- [ ] Define performance baselines.

---

# 230. MILESTONE 1 — SOURCE HYGIENE

- [ ] Remove `__pycache__` from repository/archive.
- [ ] Remove nested source ZIP from source tree.
- [ ] Correct `.gitignore`.
- [ ] Separate build artifacts.
- [ ] Remove generated test artifacts.
- [ ] Add reproducible source packaging.

---

# 231. MILESTONE 2 — FRONTEND FOUNDATION

- [ ] Vue 3
- [ ] TypeScript strict
- [ ] Vite
- [ ] Vue Router
- [ ] Pinia
- [ ] eslint
- [ ] `vue-tsc`
- [ ] Vitest
- [ ] Playwright/E2E equivalent for desktop smoke where appropriate
- [ ] component library
- [ ] design tokens

---

# 232. MILESTONE 3 — TAURI FOUNDATION

- [ ] Tauri 2 shell
- [ ] window lifecycle
- [ ] tray
- [ ] commands
- [ ] capabilities
- [ ] updater foundation
- [ ] logging
- [ ] state
- [ ] devtools
- [ ] sidecar compatibility

---

# 233. MILESTONE 4 — RUST CORE

- [ ] typed AppState
- [ ] storage
- [ ] config
- [ ] events
- [ ] task registry
- [ ] error taxonomy
- [ ] logging
- [ ] process abstraction

---

# 234. MILESTONE 5 — MIGRATION BRIDGE

- [ ] package Python legacy as sidecar
- [ ] request/response protocol
- [ ] health endpoint
- [ ] version handshake
- [ ] timeout
- [ ] restart
- [ ] graceful shutdown
- [ ] crash detection

---

# 235. MILESTONE 6 — CORE USER FLOWS

- [ ] Dashboard
- [ ] Accounts
- [ ] Instances
- [ ] Profiles
- [ ] Play
- [ ] Downloads

Acceptance:

```text
existing 254 tests still pass
new UI smoke passes
```

---

# 236. MILESTONE 7 — MODS

- [ ] Modrinth provider
- [ ] cache
- [ ] install
- [ ] update
- [ ] dependency resolver
- [ ] quarantine
- [ ] compatibility graph

---

# 237. MILESTONE 8 — RESOURCE / VISUAL

- [ ] Asset Library
- [ ] Resource Studio
- [ ] Crosshair
- [ ] HUD
- [ ] Totem 3D
- [ ] build graph
- [ ] validator
- [ ] install

---

# 238. MILESTONE 9 — OPTIMIZATION

- [ ] system scan
- [ ] game scan
- [ ] rule engine
- [ ] plan
- [ ] backup
- [ ] apply
- [ ] rollback
- [ ] benchmark

---

# 239. MILESTONE 10 — NETWORK / RUNTIME

- [ ] DNS
- [ ] TCP
- [ ] MC status
- [ ] runtime IPC
- [ ] packet metadata
- [ ] runtime monitor
- [ ] network timeline

---

# 240. MILESTONE 11 — DIAGNOSTICS

- [ ] log center
- [ ] crash fingerprint
- [ ] evidence graph
- [ ] repair plan
- [ ] diagnostic bundle
- [ ] privacy sanitization

---

# 241. MILESTONE 12 — LEGACY CUTOVER

Chỉ khi:

```text
all critical flows green
all baseline tests green
startup benchmark acceptable
memory benchmark acceptable
```

mới:

- [ ] pywebview removed
- [ ] Python UI bridge removed
- [ ] Python services deprecated
- [ ] Python sidecar removed

---

# 242. CUTOVER RULE

Không merge branch migration nếu:

```text
old flow broken
new flow incomplete
fallback unavailable
```

Mỗi migration domain phải có:

```text
old
new
flag
rollback
```

---

# 243. FEATURE FLAG MATRIX

```text
new_ui
rust_core
rust_downloads
rust_profiles
rust_minecraft
resource_studio_v2
network_lab
packet_debug
companion_v2
```

Production mặc định:

```text
stable only
```

---

# 244. RELEASE CHANNELS

```text
nightly
canary
stable
```

Experimental features chỉ có trong nightly/canary.

---

# 245. TELEMETRY PRIVACY DEFAULT

Local-only metrics.

Không upload:

```text
logs
packet data
account data
game saves
```

Remote telemetry nếu có về sau phải explicit opt-in và documented.

---

# 246. USER-CONTROLLED RESOURCE LIMITS

Settings:

```text
download concurrency
cache size
packet buffer size
log retention
telemetry retention
thumbnail cache
```

Có preset:

```text
Low RAM
Balanced
High Performance
Developer
```

---

# 247. LOW-RAM PROFILE

```text
blur off/reduced
animation reduced
chart sampling reduced
preview cache small
packet buffer 10k
background prefetch off
```

---

# 248. PERFORMANCE PROFILE

```text
preload commonly used metadata
larger cache
higher thumbnail cache
faster background refresh
```

Nhưng vẫn không scan toàn bộ filesystem vô hạn.

---

# 249. DEVELOPER PROFILE

```text
DevTools
verbose logs
event inspector
packet capture controls
diagnostic overlay
```

Không dùng làm default.

---

# 250. ACCEPTANCE TEST — FIRST LAUNCH

```text
Install
→ launch
→ splash
→ dashboard
```

Yêu cầu:

```text
no blank screen
no dev console error
logo appears
navigation works
notification center works
```

---

# 251. ACCEPTANCE TEST — CREATE INSTANCE

```text
New instance
→ select MC
→ loader
→ Java
→ memory
→ save
```

Kiểm tra:

```text
atomic save
profile relation
UI state
```

---

# 252. ACCEPTANCE TEST — PLAY

```text
Play
→ preflight
→ download if needed
→ companion pairing
→ Minecraft
→ runtime metrics
→ exit
→ diagnostics
```

---

# 253. ACCEPTANCE TEST — RESOURCE

```text
create project
→ import PNG
→ assign
→ preview
→ validate
→ build
→ install
→ rollback
```

---

# 254. ACCEPTANCE TEST — TOTEM

```text
open
→ create cube
→ move
→ texture
→ preview
→ save
→ reopen
→ export
```

Kiểm tra memory sau 20 lần.

---

# 255. ACCEPTANCE TEST — MOD UPDATE

```text
discover
→ compatibility
→ plan
→ backup
→ download
→ verify
→ apply
```

---

# 256. ACCEPTANCE TEST — NETWORK

```text
DNS check
→ TCP
→ MC status
→ runtime attach
→ network graph
→ packet metadata
```

---

# 257. ACCEPTANCE TEST — CRASH

Simulate:

```text
bad mod
```

Expected:

```text
Minecraft exits
→ launcher remains alive
→ crash analyzer
→ issue fingerprint
→ evidence
→ repair suggestion
```

---

# 258. ACCEPTANCE TEST — COMPANION DISCONNECT

```text
Minecraft running
→ companion disconnect
```

Expected:

```text
runtime state = stale
launcher remains usable
reconnect available
```

---

# 259. ACCEPTANCE TEST — APP CLOSE

```text
close launcher
```

Expected:

```text
cancel safe tasks
stop IPC
flush state
close child processes only according to policy
release locks
```

No orphan Antares-owned helper process.

---

# 260. RELEASE QUALITY BAR

## Stability

- [ ] No crash on normal navigation.
- [ ] No crash when provider unavailable.
- [ ] No UI freeze on large asset scan.
- [ ] No UI freeze on 10k logs.
- [ ] No leaked 3D resources.
- [ ] No orphan owned process.

## Performance

- [ ] startup budget measured.
- [ ] route transition budget measured.
- [ ] memory budget measured.
- [ ] event storm test green.
- [ ] editor performance green.

## Security

- [ ] scoped file permissions.
- [ ] sanitized diagnostics.
- [ ] ZIP hardening.
- [ ] secure credential storage.
- [ ] plugin permissions.

## UX

- [ ] loading states.
- [ ] empty states.
- [ ] error states.
- [ ] keyboard paths.
- [ ] reduced motion.
- [ ] English/Vietnamese.

---

# 261. DEFINITION OF “MƯỢT” CHO ANTARES

“Mượt” không đồng nghĩa với animation nhiều.

Antares mượt khi:

```text
click
↓
feedback < ~100ms
↓
long task có progress
↓
UI không block
↓
background work có throttling
```

“Mượt” cũng có nghĩa:

```text
no stutter
no event storm
no spinner flash
no duplicate task
no random reload
no state desync
```

---

# 262. DEFINITION OF “ỔN ĐỊNH”

Ổn định là:

```text
restart-safe
crash-safe
power-loss safer
network-failure tolerant
migration-safe
rollback-capable
version-aware
```

Không phải chỉ “không crash”.

---

# 263. DEFINITION OF “NHANH”

Nhanh gồm:

```text
fast startup
fast navigation
fast metadata cache
fast local search
fast task scheduling
fast download resume
fast editor preview
```

Không đánh đổi correctness để lấy benchmark giả.

---

# 264. KIẾN TRÚC CUỐI

```text
                       ANTARES DESKTOP
                              │
                       Tauri 2 + WebView2
                              │
                     Vue 3 + TypeScript
                              │
                    Pinia + Feature Modules
                              │
                    Typed Command/Event API
                              │
                         Rust Core
          ┌───────────────┬───┴─────────────┬───────────────┐
          │               │                 │               │
       Storage         Runtime           Network         Tasks
          │               │                 │               │
          │          Minecraft        Diagnostics       Scheduler
          │           Companion         Packet             │
          │               │             telemetry          │
          └───────────────┴─────────────────┴───────────────┘
                              │
                     Python sidecar (temporary)
                              │
                       Legacy services
```

Đây là architecture đích; Python legacy chỉ là cầu chuyển đổi.

---

# 265. PHÂN BỔ TRÁCH NHIỆM CUỐI

| Layer | Được làm | Không được làm |
|---|---|---|
| Vue | presentation/state | filesystem/process |
| Tauri commands | bridge | business logic lớn |
| Rust domain | rules/models | UI |
| Rust services | orchestration | direct rendering |
| Storage | persistence | UI decisions |
| Companion | Minecraft runtime data | launcher UI |
| Python legacy | compatibility | new architecture |
| Plugin | scoped extension | unrestricted system control |

---

# 266. FILE MIGRATION MAP

```text
api/bridge/api.py
    → src-tauri/commands/*.rs

api/events/bridge.py
    → src-tauri/events/*.rs

core/events/
    → crates/core/events

core/tasks/
    → crates/core/tasks

infrastructure/fs/
    → crates/storage

infrastructure/http/
    → crates/http

infrastructure/process/
    → crates/process

services/downloads/
    → crates/downloads

services/java/
    → crates/minecraft/java

services/minecraft/
    → crates/minecraft

services/mods/
    → crates/mods

services/resources/
    → crates/resources

services/visuals/
    → crates/visuals

services/optimization/
    → crates/optimization

services/performance/
    → crates/telemetry

services/diagnostics/
    → crates/diagnostics
```

Frontend:

```text
frontend/tabs/*
    → apps/desktop/src/features/*
```

---

# 267. FILE SIZE RULES — 4.0

Guideline:

```text
Vue component       < 350 lines
TS module           < 500 lines
Rust module         < 500 lines
service             < 500 lines
command group       < 300 lines
```

Không phải hard limit compiler; là maintainability gate.

---

# 268. CODE REVIEW CHECKLIST

Mỗi PR lớn:

```text
[ ] domain logic isolated
[ ] no hidden global
[ ] typed error
[ ] task cancellable
[ ] timeout exists
[ ] retry policy justified
[ ] file paths scoped
[ ] event QoS defined
[ ] tests added
[ ] benchmark considered
[ ] rollback considered
[ ] i18n complete
[ ] accessibility complete
```

---

# 269. KHÔNG ĐƯỢC THÊM TÍNH NĂNG KIỂU “HỆ THỐNG NẶNG” MỘT CÁCH TÙY TIỆN

Không thêm chỉ vì “ngầu”:

```text
massive animated background
live 3D background toàn app
permanent packet capture
filesystem full scan every minute
system tray polling every second
hundreds of charts
```

Antares là launcher/control center, không phải benchmark GPU.

---

# 270. PRIORITY ROADMAP CUỐI

## P0 — Foundation

```text
Tauri
Vue TS
Rust core
typed contracts
storage
tasks
events
security
```

## P1 — Playability

```text
accounts
instances
Java
Minecraft
downloads
profiles
mods
```

## P2 — Creative

```text
Resource Studio
Crosshair
HUD
Totem 3D
Visual Studio
```

## P3 — Deep diagnostics

```text
runtime
network
packet
crash
repair
```

## P4 — Optimization

```text
benchmark
profiles
safe system tuning
cache
```

## P5 — Ecosystem

```text
plugins
updater
release channels
advanced integrations
```

---

# 271. CHỐT TRIỂN KHAI

Không nên bắt đầu bằng việc “làm UI thật đẹp” rồi mới nghĩ core.

Thứ tự chuẩn:

```text
BASELINE
  ↓
BOUNDARIES
  ↓
CONTRACTS
  ↓
CORE
  ↓
STATE
  ↓
UI SHELL
  ↓
FEATURES
  ↓
TELEMETRY
  ↓
OPTIMIZATION
  ↓
HARDENING
  ↓
PACKAGING
```

UI đẹp sẽ được dựng trên architecture ổn định, không phải dùng UI để che lỗi backend.

---

# 272. FINAL TARGET

Antares 4.0 cuối cùng phải cho cảm giác:

```text
Launch nhanh
Navigate nhanh
Search nhanh
Play ổn định
Download ổn định
Mods dễ quản lý
Profiles rõ ràng
Resource Studio mạnh
Totem Studio thật
HUD/Crosshair đẹp
Runtime realtime
Network có số liệu
Diagnostics có bằng chứng
Optimization có rollback
Plugin có permission
Data có cấu trúc
Update an toàn
```

và ở tầng engineering:

```text
typed
observable
testable
recoverable
version-aware
bounded
modular
```

**Đây mới là baseline “mượt + ổn định + tối ưu sâu Minecraft” cần dùng cho implementation thực tế.**
