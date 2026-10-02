"""Resume strategy — .part + .part.meta (mục 9.3).

Quy tắc: KHÔNG replace file đích trước khi verify checksum.
"""
from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path


@dataclass
class PartInfo:
    part_path: Path
    meta_path: Path
    url: str = ""
    etag: str | None = None
    size: int = 0

    @staticmethod
    def for_target(target: Path) -> "PartInfo":
        return PartInfo(
            part_path=target.with_suffix(target.suffix + ".antares-part"),
            meta_path=target.with_suffix(target.suffix + ".antares-part.meta"),
        )

    def save_meta(self) -> None:
        self.meta_path.write_text(json.dumps({
            "url": self.url, "etag": self.etag, "size": self.size,
        }), encoding="utf-8")

    def load_meta(self) -> bool:
        try:
            data = json.loads(self.meta_path.read_text(encoding="utf-8"))
        except Exception:
            return False
        saved_url = data.get("url")
        if not saved_url or (self.url and saved_url != self.url):
            return False
        self.etag = data.get("etag")
        self.size = int(data.get("size", 0))
        self.url = saved_url
        return True

    def bytes_have(self) -> int:
        return self.part_path.stat().st_size if self.part_path.exists() else 0

    def cleanup(self) -> None:
        self.part_path.unlink(missing_ok=True)
        self.meta_path.unlink(missing_ok=True)

    def finalize(self, target: Path) -> None:
        """Rename .part -> target sau khi verify; dọn meta."""
        target.parent.mkdir(parents=True, exist_ok=True)
        self.part_path.replace(target)
        self.meta_path.unlink(missing_ok=True)
