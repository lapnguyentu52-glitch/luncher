"""Shared fixtures — AppContext với data dir tạm (mục 585: temporary home)."""
from __future__ import annotations

import sys
from pathlib import Path

import pytest

# Đảm bảo import từ project root (pytest chạy từ antares-src/)
ROOT = Path(__file__).resolve().parents[1]
if str(ROOT) not in sys.path:
    sys.path.insert(0, str(ROOT))

from core.config.manager import ConfigManager          # noqa: E402
from core.events.bus import EventBus                   # noqa: E402
from core.tasks.manager import TaskManager             # noqa: E402
from app.context import AppContext, AppPaths           # noqa: E402
from services.instances.service import InstanceService  # noqa: E402
from infrastructure.process.manager import ProcessManager  # noqa: E402


@pytest.fixture()
def paths(tmp_path: Path) -> AppPaths:
    p = AppPaths(root=tmp_path, data_dir=tmp_path / "data")
    p.ensure_all()
    return p


@pytest.fixture()
def ctx(paths: AppPaths) -> AppContext:
    ctx = AppContext(
        paths=paths,
        config=ConfigManager(paths.config / "settings.json"),
        events=EventBus(),
        tasks=TaskManager(),
    )
    ctx.set("instances", InstanceService(ctx))
    ctx.set("process_manager", ProcessManager())
    yield ctx


@pytest.fixture()
def instance_id(ctx: AppContext) -> str:
    """Instance thật để test gắn vào."""
    return ctx.get("instances").create("TestInst", "1.21")["id"]
