"""DownloadManager — retry, resume, checksum, atomic finalize (mục 9).

Không phụ thuộc UI. Progress qua callback + TaskManager.
"""
from __future__ import annotations

from pathlib import Path
from typing import Callable

from core.config.reader import read_json
from core.errors import codes
from core.errors.base import AntaresError, DownloadError
from core.logging.setup import get_logger
from core.tasks.manager import Task, TaskManager
from infrastructure.http.client import HttpClient, DEFAULT_TIMEOUT
from services.downloads.checksum import verify
from services.downloads.resume import PartInfo

logger = get_logger("downloads")

ProgressCb = Callable[[float, str], None]


class DownloadManager:
    def __init__(self, http: HttpClient, tasks: TaskManager, cache_dir: Path) -> None:
        self._http = http
        self._tasks = tasks
        self._cache_dir = cache_dir
        self._cache_dir.mkdir(parents=True, exist_ok=True)

    def download(self, url: str, target: Path, *,
                 sha1: str | None = None,
                 sha256: str | None = None,
                 overwrite: bool = False,
                 task: Task | None = None,
                 progress: ProgressCb | None = None) -> bool:
        """Tải file về target. Trả False nếu đã tồn tại & hợp lệ.

        Pipeline (mục 174): cache? -> HTTP stream -> .part -> checksum -> atomic.
        """
        if target.exists() and not overwrite:
            if verify(target, sha1, "sha1") or verify(target, sha256, "sha256"):
                return False
            logger.info("Existing file corrupt, redownloading: %s", target.name)

        part = PartInfo.for_target(target)
        part.url = url
        resumable = (part.bytes_have() > 0 and part.load_meta())

        max_attempts = 8
        last_err: Exception | None = None
        for attempt in range(1, max_attempts + 1):
            if task and task.cancelled:
                raise DownloadError(codes.DOWNLOAD_CANCELLED, "Download cancelled")
            try:
                self._attempt(url, target, part, resumable and attempt > 1,
                              task, progress)
                # verify trước khi finalize (mục 9.3)
                if sha1 and not verify(part.part_path, sha1, "sha1"):
                    part.cleanup()
                    last_err = DownloadError(codes.DOWNLOAD_CHECKSUM_MISMATCH,
                                             f"SHA1 mismatch: {url}")
                    continue
                if sha256 and not verify(part.part_path, sha256, "sha256"):
                    part.cleanup()
                    last_err = DownloadError(codes.DOWNLOAD_CHECKSUM_MISMATCH,
                                             f"SHA256 mismatch: {url}")
                    continue
                part.finalize(target)
                return True
            except AntaresError:
                raise
            except Exception as e:
                last_err = e
                import time
                time.sleep(min(2 * attempt, 8))
        if last_err:
            raise last_err
        raise DownloadError(codes.INTERNAL_ERROR, f"Download failed: {url}")

    def _attempt(self, url: str, target: Path, part: PartInfo,
                 resume: bool, task: Task | None, progress: ProgressCb | None) -> None:
        headers = {}
        mode = "wb"
        have = part.bytes_have()
        if resume and have > 0:
            headers["Range"] = f"bytes={have}-"
            mode = "ab"
        else:
            have = 0
        resp = self._http.stream(url, headers=headers)

        if resume and resp.status_code != 206 and mode == "ab":
            mode = "wb"  # server không hỗ trợ range -> restart
            have = 0

        total = int(resp.headers.get("content-length") or 0) + have
        if total:
            part.size = total
            part.save_meta()

        done = have
        with open(part.part_path, mode) as f:
            for chunk in resp.iter_content(1 << 16):
                if task and task.cancelled:
                    resp.close()
                    raise DownloadError(codes.DOWNLOAD_CANCELLED, "Cancelled")
                f.write(chunk)
                done += len(chunk)
                if progress and total:
                    progress(100.0 * done / total, target.name)
        resp.close()
