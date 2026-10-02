"""LoaderProvider contract (mục 132).

Mỗi provider: list_versions / resolve / install / validate.
Provider KHÔNG biết UI — emit progress qua callback.
"""
from __future__ import annotations

from typing import Protocol

from core.tasks.manager import Task


class LoaderProvider(Protocol):
    id: str

    def list_versions(self) -> list[str]:
        """Danh sách version minecraft được hỗ trợ."""
        ...

    def resolve_launch_version(self, mc_version: str, loader_version: str | None = None) -> str:
        """Tên version folder trong versions/ (vd fabric-loader-x-1.21)."""
        ...

    def install(self, mc_version: str, game_dir: str, task: Task | None = None) -> str:
        """Cài loader vào game_dir; trả về resolved version."""
        ...
