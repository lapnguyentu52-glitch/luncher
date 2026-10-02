"""Application context — dependency container (no framework needed).

Mỗi service nhận context thay vì dùng global mutable state.
"""
from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path

from core.config.manager import ConfigManager
from core.events.bus import EventBus
from core.tasks.manager import TaskManager


@dataclass
class AppPaths:
    """PathResolver — nguồn chân lý duy nhất cho mọi đường dẫn.

    Portable mode: file `portable.flag` cạnh exe -> data nằm trong ./data.
    data_dir: override cho test/dev (mục 585: temporary home directory).
    """

    root: Path
    data_dir: Path | None = None

    @property
    def portable(self) -> bool:
        return (self.root / "portable.flag").exists()

    @property
    def data(self) -> Path:
        if self.data_dir is not None:
            return self.data_dir
        if self.portable:
            return self.root / "data"
        import os

        base = os.environ.get("APPDATA") or str(Path.home())
        return Path(base) / "AntaresLauncher"

    @property
    def config(self) -> Path:
        return self.data / "config"

    @property
    def logs(self) -> Path:
        return self.data / "logs"

    @property
    def cache(self) -> Path:
        return self.data / "cache"

    @property
    def instances(self) -> Path:
        return self.data / "instances"

    @property
    def servers(self) -> Path:
        return self.data / "servers"

    @property
    def accounts(self) -> Path:
        return self.data / "accounts"

    @property
    def sessions(self) -> Path:
        return self.data / "sessions"

    @property
    def resource_studio(self) -> Path:
        """Workspace Resource Pack Studio (mục 10.3)."""
        return self.data / "resource-studio"

    @property
    def plugins(self) -> Path:
        """Thư mục plugin người dùng (mục 84): mỗi plugin 1 thư mục con."""
        return self.data / "plugins"

    def ensure_all(self) -> None:
        for p in (self.data, self.config, self.logs, self.cache,
                  self.instances, self.servers, self.accounts, self.sessions,
                  self.plugins):
            p.mkdir(parents=True, exist_ok=True)


@dataclass
class AppContext:
    """Container được inject vào mọi service."""

    paths: AppPaths
    config: ConfigManager
    events: EventBus
    tasks: TaskManager
    dev_mode: bool = False
    _extra: dict = field(default_factory=dict)

    def get(self, key: str):
        return self._extra.get(key)

    def set(self, key: str, value) -> None:
        self._extra[key] = value
