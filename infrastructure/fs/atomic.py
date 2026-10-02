"""Filesystem — atomic write, file lock, safe extract (mục 91, 24)."""
from core.config.writer import write_json_atomic, write_text_atomic

__all__ = ["write_json_atomic", "write_text_atomic"]
