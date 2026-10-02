"""Checksum verification — SHA1/SHA256 stream, không load cả file (mục 9.3)."""
from __future__ import annotations

import hashlib
from pathlib import Path


def file_hash(path: Path, algorithm: str = "sha1") -> str:
    h = hashlib.new(algorithm)
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def verify(path: Path, expected: str | None, algorithm: str = "sha1") -> bool:
    if not expected:
        return True
    return file_hash(path, algorithm) == expected.lower()


def file_sha256(path: Path) -> str:
    return file_hash(path, "sha256")
