"""Logging — rotating file + in-memory ring buffer + redaction (mục 16, 79, 294)."""
from __future__ import annotations

import logging
import logging.handlers
import re
import threading
from collections import deque
from pathlib import Path

_REDACT_PATTERNS = [
    re.compile(r"(accessToken|access_token|clientToken|client_token|refreshToken|refreshToken|authorization|password|clientSecret)(\s*[=:]\s*)(\S+)", re.IGNORECASE),
    re.compile(r"(Bearer\s+)([A-Za-z0-9._\-]+)"),
]


def redact(text: str) -> str:
    for pat in _REDACT_PATTERNS:
        text = pat.sub(lambda m: f"{m.group(1)}{m.group(2)}***REDACTED***" if m.lastindex == 3 else f"{m.group(1)}***REDACTED***", text)
    return text


class RingBufferHandler(logging.Handler):
    """Giữ N dòng cuối trong memory cho UI console (mục 16)."""

    def __init__(self, capacity: int = 5000) -> None:
        super().__init__()
        self._buf: deque[str] = deque(maxlen=capacity)
        # Metadata per dòng (level + logger name) — song song với _buf,
        # phục vụ Console lọc theo mức/nguồn mà không phải re-parse chuỗi (mục 41).
        self._meta: deque[dict] = deque(maxlen=capacity)
        self._lock = threading.Lock()
        # Số thứ tự tích luỹ (không bị cắt theo capacity) -> UI lấy phần mới
        # bằng snapshot_since() thay vì đọc lại cả buffer mỗi lần (mục 15.2).
        self._written = 0

    def emit(self, record: logging.LogRecord) -> None:
        try:
            msg = self.format(record)
        except Exception:
            return
        with self._lock:
            self._buf.append(msg)
            self._meta.append({
                "level": record.levelname,
                "logger": record.name,
                "ts": record.created,
            })
            self._written += 1

    def snapshot(self, limit: int | None = None) -> list[str]:
        with self._lock:
            items = list(self._buf)
        return items[-limit:] if limit else items

    def snapshot_meta(self, limit: int | None = None) -> list[dict]:
        """Dòng + metadata (level/logger) dạng [{text, level, logger, ts}] —
        Console dùng để lọc theo mức không phải re-parse chuỗi (mục 41)."""
        with self._lock:
            lines = list(self._buf)
            meta = list(self._meta)
        # meta có thể ngắn hơn khi nâng cấp chạy nóng — pad đầu bằng level suy đoán.
        if len(meta) < len(lines):
            pad = [{"level": "INFO", "logger": "antares", "ts": None}] \
                * (len(lines) - len(meta))
            meta = pad + meta
        out = [{"text": line, **meta[i]} for i, line in enumerate(lines)]
        return out[-limit:] if limit else out

    @property
    def written(self) -> int:
        """Tổng số dòng đã ghi (con trỏ cho lần đọc kế tiếp)."""
        with self._lock:
            return self._written

    def snapshot_since(self, seq: int) -> tuple[list[str], int]:
        """Trả (dòng mới kể từ seq, con trỏ mới).

        Nếu seq cũ hơn dòng lâu nhất còn giữ (bị cắt), trả toàn bộ buffer và
        con trỏ hiện tại — UI tự coi đây là lần nạp lại.
        """
        with self._lock:
            items = list(self._buf)
            written = self._written
        first_seq = written - len(items)
        if seq < first_seq:
            return items, written
        return items[seq - first_seq:], written

    def snapshot_since_meta(self, seq: int) -> tuple[list[dict], int]:
        """Giống snapshot_since nhưng trả [{text, level, logger, ts}, ...].

        Dùng cho kênh đẩy log.lines mang luôn level — UI lọc realtime không re-parse.
        """
        with self._lock:
            lines = list(self._buf)
            meta = list(self._meta)
            written = self._written
        first_seq = written - len(lines)
        if seq < first_seq:
            start = 0
            reset = True
        else:
            start = seq - first_seq
            reset = False
        if len(meta) < len(lines):
            pad = [{"level": "INFO", "logger": "antares", "ts": None}] \
                * (len(lines) - len(meta))
            meta = pad + meta
        out = [{"text": lines[i], **meta[i]} for i in range(start, len(lines))]
        return out, written


_ring: RingBufferHandler | None = None


def setup_logging(logs_dir: Path, *, level: int = logging.INFO, dev_mode: bool = False) -> logging.Logger:
    global _ring
    logs_dir.mkdir(parents=True, exist_ok=True)
    root = logging.getLogger("antares")
    root.setLevel(logging.DEBUG if dev_mode else level)
    root.handlers.clear()

    fmt = logging.Formatter(
        "%(asctime)s %(levelname)s %(name)s %(message)s", "%Y-%m-%d %H:%M:%S"
    )

    file_h = logging.handlers.RotatingFileHandler(
        logs_dir / "launcher.log", maxBytes=2_000_000, backupCount=3, encoding="utf-8"
    )
    file_h.setFormatter(fmt)
    root.addHandler(file_h)

    _ring = RingBufferHandler()
    _ring.setFormatter(fmt)
    root.addHandler(_ring)

    if dev_mode:
        import sys
        console = logging.StreamHandler(sys.stderr)
        console.setFormatter(fmt)
        root.addHandler(console)

    return root


def get_logger(name: str) -> logging.Logger:
    return logging.getLogger(f"antares.{name}")


def get_ring_snapshot(limit: int | None = None) -> list[str]:
    if _ring is None:
        return []
    return _ring.snapshot(limit)


def get_ring() -> RingBufferHandler | None:
    """Ring buffer hiện tại (None nếu logging chưa setup)."""
    return _ring


def get_ring_cursor_only() -> int:
    """Con trỏ tính bằng tổng số dòng đã ghi — dùng cho logs_recent."""
    return _ring.written if _ring is not None else 0


def get_ring_cursor() -> int:
    """Con trỏ hiện tại của ring buffer (0 nếu logging chưa setup)."""
    return _ring.written if _ring is not None else 0


def get_ring_since(seq: int) -> tuple[list[str], int]:
    """Lấy các dòng log mới kể từ con trỏ seq -> dùng cho batch event."""
    if _ring is None:
        return [], seq
    return _ring.snapshot_since(seq)


def get_ring_since_meta(seq: int) -> tuple[list[dict], int]:
    """Batch log mới kèm level/logger — Console lọc realtime (mục 41)."""
    if _ring is None:
        return [], seq
    return _ring.snapshot_since_meta(seq)
