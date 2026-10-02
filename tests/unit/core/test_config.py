"""ConfigManager — cache in-memory + reload từ đĩa (dùng sau restore)."""
from __future__ import annotations

import json

from core.config.manager import ConfigManager


def test_defaults_and_set_get(paths):
    cfg = ConfigManager(paths.config / "settings.json")
    assert cfg.get("performance.defaultMaxMb") == 2048
    cfg.set("selectedInstance", "abc123", flush_now=True)
    assert cfg.get("selectedInstance") == "abc123"


def test_flush_writes_disk_and_reload_picks_up(paths):
    cfg = ConfigManager(paths.config / "settings.json")
    cfg.set("ui.language", "en", flush_now=True)

    # Instance thứ hai "từ đĩa" — mô phỏng process/tầng khác đọc lại
    cfg2 = ConfigManager(paths.config / "settings.json")
    assert cfg2.get("ui.language") == "en"

    # Ghi đè đĩa trực tiếp -> manager cũ phải reload mới thấy
    data = json.loads((paths.config / "settings.json").read_text(encoding="utf-8"))
    data["ui"]["language"] = "vi"
    (paths.config / "settings.json").write_text(json.dumps(data), encoding="utf-8")
    assert cfg.get("ui.language") == "en"      # vẫn cache cũ
    cfg.reload()
    assert cfg.get("ui.language") == "vi"      # sau reload thấy mới


def test_update_section_merges_not_replaces(paths):
    cfg = ConfigManager(paths.config / "settings.json")
    cfg.update_section("performance", {"defaultMaxMb": 4096}, flush_now=True)
    assert cfg.get("performance.defaultMaxMb") == 4096
    assert cfg.get("performance.defaultMinMb") == 512  # key khác giữ nguyên
