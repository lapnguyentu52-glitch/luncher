"""Secure token storage — Windows Credential Manager / DPAPI (mục 435).

Fallback: file với base64-xor (không hoàn toàn secure nhưng tách khỏi settings.json).
Metadata account ≠ credential secret — tách 2 nơi.
"""
from __future__ import annotations

import base64
import json
import os
from pathlib import Path

_KEY = b"antares-launcher-v1"


class SecureStore:
    """Lưu secret theo key. Windows dùng DPAPI qua win32crypt nếu có."""

    def __init__(self, store_dir: Path) -> None:
        self._dir = store_dir
        self._dir.mkdir(parents=True, exist_ok=True)

    def _path(self, key: str) -> Path:
        return self._dir / f"{key}.secret"

    def save(self, key: str, secret: str) -> None:
        data = secret.encode("utf-8")
        if os.name == "nt":
            try:
                import win32crypt  # type: ignore
                blob = win32crypt.CryptProtectData(data, key, None, None, None, 0)
                self._path(key).write_bytes(blob)
                return
            except ImportError:
                pass
        encoded = base64.b64encode(bytes(b ^ _KEY[i % len(_KEY)]
                                         for i, b in enumerate(data)))
        self._path(key).write_bytes(encoded)

    def load(self, key: str) -> str | None:
        path = self._path(key)
        if not path.exists():
            return None
        raw = path.read_bytes()
        if os.name == "nt":
            try:
                import win32crypt  # type: ignore
                _, data = win32crypt.CryptUnprotectData(raw, None, None, None, 0)
                return data.decode("utf-8")
            except ImportError:
                pass
        try:
            decoded = bytes(b ^ _KEY[i % len(_KEY)]
                            for i, b in enumerate(base64.b64decode(raw)))
            return decoded.decode("utf-8")
        except Exception:
            return None

    def delete(self, key: str) -> None:
        self._path(key).unlink(missing_ok=True)
