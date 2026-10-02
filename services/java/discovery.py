"""Java discovery — học từ minecraft_loader/java_utils.py.

Học được:
 - Scan known directories để tìm mọi Java install (không chỉ PATH)
 - Check bin/java hoặc bin/java.exe tồn tại trong subdir
 - javaw.exe riêng cho Windows GUI (không console)
"""
from __future__ import annotations

import os
import platform
import re
import shutil
import subprocess
from pathlib import Path

from core.logging.setup import get_logger

logger = get_logger("java.discovery")

_VERSION_RE = re.compile(r'version "(\d+)')

_WIN_JAVA_DIRS = [
    r"C:\Program Files\Java",
    r"C:\Program Files (x86)\Java",
    r"C:\Program Files\Eclipse Adoptium",
    r"C:\Program Files\Microsoft\jdk",
]
_LINUX_JAVA_DIRS = [
    "/usr/lib/jvm",
    "/opt/java",
]
_MAC_JAVA_DIRS = [
    "/Library/Java/JavaVirtualMachines",
]


def java_executable(java_home: Path) -> Path | None:
    """bin/java(.exe) trong 1 Java home (port logic từ get_java_information)."""
    exe = "java.exe" if os.name == "nt" else "java"
    p = java_home / "bin" / exe
    return p if p.is_file() else None


def javaw_executable(java_home: Path) -> Path | None:
    """javaw cho Windows GUI launch — không console window (học java_utils)."""
    if os.name != "nt":
        return None
    p = java_home / "bin" / "javaw.exe"
    return p if p.is_file() else None


def _scan_directory(base: str) -> list[Path]:
    """Tìm mọi subdir có bin/java (port _search_java_directory)."""
    root = Path(base)
    if not root.is_dir():
        return []
    found = []
    for entry in root.iterdir():
        if not entry.is_dir() or entry.is_symlink():
            continue
        if java_executable(entry):
            found.append(entry)
    return found


def scan_system_java() -> list[Path]:
    """Tìm mọi Java install trên máy (port find_system_java_versions)."""
    homes: list[Path] = []
    dirs = (list(_WIN_JAVA_DIRS) if os.name == "nt"
            else _LINUX_JAVA_DIRS if platform.system() == "Linux"
            else _MAC_JAVA_DIRS)
    for d in dirs:
        homes += _scan_directory(d)

    # JAVA_HOME
    java_home = os.environ.get("JAVA_HOME")
    if java_home and java_executable(Path(java_home)):
        homes.append(Path(java_home))

    # PATH java — resolve symlink
    which = shutil.which("java")
    if which:
        real = Path(which).resolve().parent.parent
        if java_executable(real):
            homes.append(real)

    # dedupe giữ thứ tự
    seen, out = set(), []
    for h in homes:
        if h not in seen:
            seen.add(h)
            out.append(h)
    return out


def detect_major(exe: Path) -> int | None:
    """Parse major version từ `java -showversion` (học get_java_information)."""
    try:
        r = subprocess.run([str(exe), "-showversion"], capture_output=True,
                           text=True, timeout=15)
        out = (r.stderr or "") + (r.stdout or "")
        m = _VERSION_RE.search(out)
        return int(m.group(1)) if m else None
    except (FileNotFoundError, subprocess.TimeoutExpired, OSError):
        return None


def java_info(java_home: Path) -> dict | None:
    """Full info cho 1 java home (port JavaInformation)."""
    exe = java_executable(java_home)
    if not exe:
        return None
    major = detect_major(exe)
    if major is None:
        return None
    return {
        "path": str(java_home),
        "exe": str(exe),
        "javaw": str(javaw_executable(java_home)) if javaw_executable(java_home) else None,
        "major": major,
        "name": java_home.name,
    }
