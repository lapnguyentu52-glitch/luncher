"""JavaManager — discovery, version cache, compatibility (mục 13, 282-283).

Không gọi `java -version` mỗi click — cache path -> major.
"""
from __future__ import annotations

import re
import shutil
import subprocess
import threading
from pathlib import Path

from core.errors import codes
from core.errors.base import JavaError
from core.logging.setup import get_logger

logger = get_logger("java")

_VERSION_RE = re.compile(r'version "(\d+)')

# Minecraft version -> required Java major (mục 13: không hardcode 1 giá trị cho mọi MC)
def required_major_for(mc_version: str) -> int:
    parts = [int(x) for x in re.findall(r"\d+", str(mc_version))[:3]]
    while len(parts) < 3:
        parts.append(0)
    if parts[0] > 1 or (parts[0] == 1 and parts[1] >= 21):
        return 21
    if parts[0] == 1 and parts[1] == 20 and parts[2] >= 5:
        return 21
    if parts[0] == 1 and parts[1] >= 18:
        return 17
    return 8


class JavaManager:
    def __init__(self) -> None:
        self._cache: dict[str, int | None] = {}
        self._lock = threading.Lock()
        self._homes: list[Path] = []

    def discover_all(self) -> list[dict]:
        """Full discovery — scan system dirs + JAVA_HOME + PATH (học java_utils)."""
        from services.java.discovery import java_info, scan_system_java
        self._homes = scan_system_java()
        infos = [i for h in self._homes if (i := java_info(h))]
        for info in infos:
            with self._lock:
                self._cache[info["exe"]] = info["major"]
        return infos

    def detect_major(self, exe: str = "java") -> int | None:
        """Trả major version của java executable; cache theo path."""
        with self._lock:
            if exe in self._cache:
                return self._cache[exe]
        try:
            r = subprocess.run([exe, "-version"], capture_output=True,
                               text=True, timeout=15)
            out = (r.stderr or "") + (r.stdout or "")
            m = _VERSION_RE.search(out)
            major = int(m.group(1)) if m else None
        except (FileNotFoundError, subprocess.TimeoutExpired):
            major = None
        with self._lock:
            self._cache[exe] = major
        return major

    def find_compatible(self, mc_version: str) -> str | None:
        """Tìm java thoả yêu cầu — scan discovery trước, PATH fallback (mục 282)."""
        need = required_major_for(mc_version)
        # 1. PATH java (nhanh nhất)
        exe = shutil.which("java")
        if exe:
            major = self.detect_major(exe)
            if major is not None and major >= need:
                return exe
        # 2. Full system discovery (học find_system_java_versions)
        if not self._homes:
            self.discover_all()
        from services.java.discovery import java_executable, detect_major as dm_fn
        for home in self._homes:
            exe_path = java_executable(home)
            if not exe_path:
                continue
            major = dm_fn(exe_path)
            if major is not None and major >= need:
                return str(exe_path)
        return None

    def validate(self, mc_version: str, exe: str = "java") -> tuple[bool, int | None]:
        """(ok, actual_major) — raise JavaError nếu không tìm thấy java nào."""
        major = self.detect_major(exe)
        if major is None:
            need = required_major_for(mc_version)
            raise JavaError(codes.JAVA_NOT_FOUND,
                            f"Java {need}+ not found. Install e.g. Adoptium Temurin {need}.",
                            action="OPEN_JAVA_SETTINGS")
        need = required_major_for(mc_version)
        if major < need:
            raise JavaError(
                codes.JAVA_VERSION_TOO_OLD,
                f"This version needs Java {need}+, but Java {major} was found.",
                action="OPEN_JAVA_SETTINGS")
        return True, major

    def mojang_runtime_executable(self, mc_version: str, game_dir: str) -> str | None:
        """Lấy executable từ Mojang runtime đã cài (nếu có) (mục 282)."""
        try:
            from minecraft_launcher_lib.runtime import get_executable_path
            from minecraft_launcher_lib.runtime import get_client_json
            data = get_client_json(mc_version, game_dir)
            component = (data.get("javaVersion") or {}).get("component")
            if not component:
                return None
            return get_executable_path(component, game_dir)
        except Exception:
            return None

    def install_mojang_runtime(self, mc_version: str, game_dir: str,
                               status_cb=None) -> str | None:
        """Cài Mojang runtime cho version — dùng khi system java quá cũ."""
        try:
            from minecraft_launcher_lib.runtime import (
                get_client_json, get_executable_path, install_jvm_runtime,
            )
            data = get_client_json(mc_version, game_dir)
            jv = data.get("javaVersion") or {}
            component, major = jv.get("component"), jv.get("majorVersion")
            if not component:
                return None
            if status_cb:
                status_cb(f"Installing Java runtime {component} (Java {major})...")
            install_jvm_runtime(component, game_dir, callback={
                "setStatus": lambda s: status_cb(s) if status_cb else None})
            exe = get_executable_path(component, game_dir)
            if exe and Path(exe).is_file():
                return exe
        except Exception as e:
            logger.warning("Mojang runtime install failed: %s", e)
        return None

    def invalidate(self, exe: str | None = None) -> None:
        with self._lock:
            if exe:
                self._cache.pop(exe, None)
            else:
                self._cache.clear()
