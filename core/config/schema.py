"""Config schema — versioned, typed defaults.

schemaVersion: 3 (mục 1.3.3)
"""
from __future__ import annotations

from typing import Any

SCHEMA_VERSION = 3

DEFAULTS: dict[str, Any] = {
    "schemaVersion": SCHEMA_VERSION,
    "app": {
        "singleInstance": True,
        "checkUpdates": True,
    },
    "ui": {
        "theme": "dark",
        "accent": "red",
        "scale": 100,
        "sidebarCollapsed": False,
        "animations": "full",
        # Mặc định tiếng Việt; frontend auto-detect theo navigator.language
        # nếu user chưa từng lưu lựa chọn (null = auto).
        "language": "vi",
    },
    "accounts": [],
    "instances": {},
    "java": {
        "mode": "auto",
        "executable": None,
    },
    "network": {
        "maxConcurrentDownloads": 4,
        "retryAttempts": 4,
        "timeoutConnect": 15,
        "timeoutRead": 45,
        "useResume": True,
        "verifyChecksums": True,
    },
    "performance": {
        "defaultMinMb": 512,
        "defaultMaxMb": 2048,
        "jvmPreset": "balanced",
        # Mục 36 + 74: mặc định Normal — không tự nâng priority nếu user không bật
        "gamePriority": "normal",
    },
    "servers": {
        "defaultRamMb": 2048,
        "autoAcceptEula": False,
    },
    "selectedInstance": None,
    "selectedAccount": None,
}
