"""Event names — taxonomy tập trung (spec mục 181/336)."""
from __future__ import annotations

APP_READY = "app.ready"
APP_SHUTDOWN = "app.shutdown"
APP_ERROR = "app.error"

DOWNLOAD_STARTED = "download.started"
DOWNLOAD_PROGRESS = "download.progress"
DOWNLOAD_COMPLETED = "download.completed"
DOWNLOAD_FAILED = "download.failed"
DOWNLOAD_CANCELLED = "download.cancelled"

MINECRAFT_VALIDATING = "minecraft.validating"
MINECRAFT_INSTALL_PROGRESS = "minecraft.install_progress"
MINECRAFT_LAUNCHING = "minecraft.launching"
MINECRAFT_STARTED = "minecraft.started"
MINECRAFT_OUTPUT = "minecraft.output"
MINECRAFT_EXITED = "minecraft.exited"
MINECRAFT_ERROR = "minecraft.error"

SERVER_STARTING = "server.starting"
SERVER_STARTED = "server.started"
SERVER_OUTPUT = "server.output"
SERVER_STOPPED = "server.stopped"
SERVER_FAILED = "server.failed"
SERVER_METRICS = "server.metrics"

AUTH_PENDING = "auth.pending"
AUTH_SUCCESS = "auth.success"
AUTH_FAILED = "auth.failed"

INSTANCE_UPDATED = "instance.updated"
INSTANCE_CREATED = "instance.created"
INSTANCE_DELETED = "instance.deleted"

# ---- Kênh tới UI (spec mục 8.2, 15.2) ----
# Thay cho việc UI poll định kỳ: backend đẩy event khi state đổi.
TASK_UPDATED = "task.updated"        # mọi thay đổi của TaskManager (progress/state)
LOG_LINES = "log.lines"              # batch dòng log launcher (từ ring buffer)

SETTINGS_CHANGED = "settings.changed"
ACCOUNTS_CHANGED = "accounts.changed"
INSTANCES_CHANGED = "instances.changed"
SERVERS_CHANGED = "servers.changed"
MODS_CHANGED = "mods.changed"
SELECTION_CHANGED = "selection.changed"

# ---- Notification Center (spec 3.0 mục 6) ----
NOTIFICATION_CREATED = "notification.created"      # NotificationService -> UI realtime
NOTIFICATIONS_CHANGED = "notifications.changed"     # read/clear/clear-all -> UI sync badge

# ---- Performance Center (spec 3.0 mục 5, 89) ----
PERFORMANCE_TELEMETRY = "performance.telemetry"     # sample CPU/RAM/disk/launcher (coalesce latest)

# ---- Game Optimization (spec 3.0 mục 3, 70) ----
OPTIMIZATION_APPLIED = "optimization.applied"       # profile apply xong
OPTIMIZATION_ROLLED_BACK = "optimization.rolled_back"  # rollback xong

# ---- Player Profiles (spec 3.0 mục 40) ----
PROFILE_APPLIED = "profile.applied"                 # apply xong (switch + jvm + game)
PROFILE_REVERTED = "profile.reverted"               # revert raw options xong
PROFILES_CHANGED = "profiles.changed"               # CRUD create/update/delete

# ---- Skin & Cape Studio (spec 3.0 mục 43) ----
SKINS_CHANGED = "skins.changed"                     # import/generate/delete xong
SKIN_APPLIED = "skin.applied"                       # apply/unapply per-instance

# ---- System Optimization (spec 3.0 mục 4, 35-37) ----
SYSTEM_CLEANED = "system.cleaned"                   # disk cleanup xong (payload: bytes freed)

# ---- Resource Pack Studio (spec 3.0 mục 10, 33) ----
RESOURCE_BUILT = "resource.built"                   # build ZIP xong (payload: projectId, file)
RESOURCE_INSTALLED = "resource.installed"           # install vào instance xong

# ---- Visual Studio (spec 3.0 mục 8, 9, 11) ----
VISUAL_EXPORTED = "visual.exported"                 # crosshair/totem/hud export xong

# ---- Backup & Restore Center (spec 3.0 mục 22) ----
BACKUP_CREATED = "backup.created"                   # snapshot xong
BACKUP_RESTORED = "backup.restored"                 # restore xong

# ---- Repair Center (spec 3.0 mục 39) ----
REPAIR_RUN = "repair.run"                           # repair action xong (payload: action, performed)

# ---- Runtime Companion (spec 3.0 mục 12, 13) ----
RUNTIME_CONNECTED = "runtime.connected"             # companion hello
RUNTIME_DISCONNECTED = "runtime.disconnected"       # companion bye
RUNTIME_PERFORMANCE = "runtime.performance"         # FPS/frametime từ game (coalesce latest)
RUNTIME_GAME_STATE = "runtime.game_state"           # screen/world/player state
RUNTIME_CHAT = "runtime.chat"                       # chat event (nếu companion cấp)
RUNTIME_ERROR = "runtime.error"                     # lỗi bên companion

# ---- Plugin SDK (spec 3.0 mục 84, 85) ----
PLUGIN_LOADED = "plugin.loaded"                     # 1 plugin load xong
PLUGIN_UNLOADED = "plugin.unloaded"                 # plugin bị disable/unload
PLUGIN_ERROR = "plugin.error"                       # lỗi trong plugin (không giết launcher)
