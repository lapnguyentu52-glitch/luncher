"""Bootstrap — khởi tạo logger, config, services, API bridge (spec 126)."""
from __future__ import annotations

from pathlib import Path

from app.context import AppContext, AppPaths
from app.version import APP_NAME
from core.config.manager import ConfigManager
from core.events.bus import EventBus
from core.logging.setup import get_logger, setup_logging
from core.tasks.manager import TaskManager

logger = get_logger("bootstrap")


def bootstrap(*, dev_mode: bool = False, root: Path | None = None,
              data_dir: Path | None = None) -> AppContext:
    """Cold start pipeline (spec 6): config -> logger -> context -> services."""
    paths = AppPaths(root=root or Path.cwd(), data_dir=data_dir)
    paths.ensure_all()

    setup_logging(paths.logs, dev_mode=dev_mode)
    logger.info("Starting %s (dev=%s)", APP_NAME, dev_mode)

    config = ConfigManager(paths.config / "settings.json")
    events = EventBus()
    tasks = TaskManager()

    ctx = AppContext(paths=paths, config=config, events=events, tasks=tasks,
                     dev_mode=dev_mode)
    logger.info("Bootstrap complete (portable=%s)", paths.portable)
    return ctx
