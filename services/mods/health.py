"""Mod health checker — metadata + dependency + outdated detection (mục 305, 451, 497).

Học từ Fabric Loader docs (remake.md mục 307): fabric.mod.json chứa
depends/recommends/breaks — đọc để preview conflict thay vì copy mù jar.
"""
from __future__ import annotations

import json
import re
import zipfile
from dataclasses import dataclass, field
from pathlib import Path

from core.logging.setup import get_logger

logger = get_logger("mods.health")


@dataclass
class ModInfo:
    filename: str
    mod_id: str | None = None
    name: str | None = None
    version: str | None = None
    loader: str | None = None           # fabric | forge | quilt | neoforge | None
    mc_versions: list[str] = field(default_factory=list)
    depends: dict = field(default_factory=dict)      # modId -> version spec
    recommends: dict = field(default_factory=dict)
    breaks: dict = field(default_factory=dict)
    fabric_mod_json: bool = False
    forge_toml: bool = False
    readable: bool = True


def read_mod_info(jar_path: Path) -> ModInfo:
    """Đọc metadata mod từ jar (fabric.mod.json / mods.toml / mcmod.info)."""
    info = ModInfo(filename=jar_path.name)
    try:
        with zipfile.ZipFile(jar_path, "r") as zf:
            names = set(zf.namelist())

            # --- Fabric / Quilt ---
            for marker, loader in (("fabric.mod.json", "fabric"),
                                   ("quilt.mod.json", "quilt")):
                if marker in names:
                    try:
                        data = json.loads(zf.read(marker).decode("utf-8", errors="replace"))
                    except Exception:
                        info.readable = False
                        continue
                    info.loader = loader
                    info.fabric_mod_json = True
                    if loader == "fabric":
                        schema = data
                    else:
                        schema = (data.get("quilt_loader") or {})
                    info.mod_id = schema.get("id")
                    info.name = schema.get("name") or info.mod_id
                    info.version = schema.get("version")
                    info.depends = _clean_deps(schema.get("depends") or
                                               schema.get("dependencies") or {})
                    info.recommends = _clean_deps(schema.get("recommends") or {})
                    info.breaks = _clean_deps(schema.get("breaks") or {})
                    return info

            # --- Forge / NeoForge (mods.toml) ---
            for marker, loader in (("META-INF/mods.toml", "forge"),
                                   ("META-INF/neoforge.mods.toml", "neoforge")):
                if marker in names:
                    info.loader = loader
                    info.forge_toml = True
                    content = zf.read(marker).decode("utf-8", errors="replace")
                    m = re.search(r'modId\s*=\s*"([^"]+)"', content)
                    if m:
                        info.mod_id = m.group(1)
                    mv = re.search(r'version\s*=\s*"([^"]+)"', content)
                    if mv:
                        info.version = mv.group(1)
                    dn = re.search(r'displayName\s*=\s*"([^"]+)"', content)
                    if dn:
                        info.name = dn.group(1)
                    deps = re.findall(r'modId\s*=\s*"([^"]+)"', content)
                    info.depends = {d: "*" for d in deps if d != info.mod_id}
                    return info

            # --- Legacy (1.7/1.12 mcmod.info) ---
            if "mcmod.info" in names:
                info.loader = "forge"
                try:
                    data = json.loads(zf.read("mcmod.info").decode("utf-8", errors="replace"))
                    if isinstance(data, list) and data:
                        info.mod_id = data[0].get("modid")
                        info.name = data[0].get("name")
                        info.version = data[0].get("version")
                except Exception:
                    info.readable = False
    except (zipfile.BadZipFile, OSError):
        info.readable = False
    return info


def _clean_deps(raw) -> dict:
    """Fabric depends dạng {'fabric-api': '*', 'minecraft': '>=1.21'}."""
    if isinstance(raw, dict):
        return {str(k): str(v) for k, v in raw.items()}
    if isinstance(raw, list):
        return {str(k): "*" for k in raw}
    return {}


@dataclass
class HealthIssue:
    mod: str
    kind: str        # missing_dependency | outdated | broken_dependency | unreadable | wrong_loader
    detail: str
    fixable: bool = False
    suggestion: str | None = None   # modId nên cài thêm / nên update


def check_health(mods: list[ModInfo], *, loader: str, mc_version: str) -> list[HealthIssue]:
    """Kiểm tra tập mod của instance: thiếu dep, lỗi thời, xung đột loader."""
    issues: list[HealthIssue] = []
    installed_ids = {m.mod_id.lower() for m in mods if m.mod_id}
    installed_ids.add("minecraft")
    installed_ids.add("java")

    for m in mods:
        # 1. Unreadable jar
        if not m.readable:
            issues.append(HealthIssue(
                mod=m.filename, kind="unreadable",
                detail="Jar hỏng hoặc không đọc được metadata"))
            continue

        # 2. Loader mismatch
        if m.loader and m.loader != loader:
            issues.append(HealthIssue(
                mod=m.filename, kind="wrong_loader",
                detail=f"Mod built cho {m.loader} nhưng instance dùng {loader}",
                suggestion=m.loader))
            continue

        # 3. Missing dependencies (fabric depends)
        for dep_id, dep_spec in m.depends.items():
            if dep_id.lower() not in installed_ids:
                issues.append(HealthIssue(
                    mod=m.filename, kind="missing_dependency",
                    detail=f"Thiếu dependency '{dep_id}' ({dep_spec})",
                    fixable=True, suggestion=dep_id))

        # 4. Breaks declaration
        for broken_id in m.breaks:
            if broken_id.lower() in installed_ids:
                issues.append(HealthIssue(
                    mod=m.filename, kind="broken_dependency",
                    detail=f"Xung đột với mod '{broken_id}' đang cài"))

    # 5. Outdated — check qua Modrinth (async ở tầng caller nếu muốn non-block)
    return issues


def check_outdated(mods: list[ModInfo], *, loader: str, mc_version: str,
                   http) -> list[HealthIssue]:
    """Check phiên bản mới trên Modrinth — network call, gọi từ worker."""
    issues: list[HealthIssue] = []
    for m in mods:
        if not m.mod_id or not m.loader:
            continue
        try:
            versions = http.get_json(
                f"https://api.modrinth.com/v2/project/{m.mod_id}/version",
                params={"loaders": json.dumps([m.loader]),
                        "game_versions": json.dumps([mc_version])},
                timeout=10)
            if not versions:
                continue
            latest = versions[0].get("version_number", "")
            if latest and m.version and _version_newer(latest, m.version):
                issues.append(HealthIssue(
                    mod=m.filename, kind="outdated",
                    detail=f"Đang dùng {m.version}, có bản {latest}",
                    fixable=True, suggestion=latest))
        except Exception as e:
            logger.debug("Outdated check failed for %s: %s", m.mod_id, e)
    return issues


_VERSION_PARTS = re.compile(r"\d+")


def _version_newer(candidate: str, current: str) -> bool:
    """So sánh semantic-ish: 1.2.10 > 1.2.9."""
    def parts(v: str) -> list[int]:
        return [int(x) for x in _VERSION_PARTS.findall(v)[:4]] or [0]
    return parts(candidate) > parts(current)
