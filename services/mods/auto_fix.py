"""Mod auto-fix — tự động tải dependency thiếu từ Modrinth (mục 176, 308).

Flow (spec mục 176): resolve -> plan -> download -> verify -> commit.
"""
from __future__ import annotations

from pathlib import Path

from core.errors import codes
from core.errors.base import AntaresError
from core.logging.setup import get_logger
from core.tasks.manager import Task
from services.mods.health import HealthIssue
from services.mods.modrinth import ModrinthClient

logger = get_logger("mods.autofix")

# Map dependency id phổ biến -> Modrinth project slug/id
KNOWN_DEPS = {
    "fabric-api": "fabric-api",          # Fabric API
    "fabricloader": None,                 # loader runtime — không phải mod
    "java": None,
    "minecraft": None,
}


def build_fix_plan(issues: list[HealthIssue], *, loader: str,
                   mc_version: str, client: ModrinthClient) -> list[dict]:
    """Tạo plan cài các dependency thiếu (chỉ fixable=True)."""
    plan: list[dict] = []
    seen: set[str] = set()
    for issue in issues:
        if issue.kind != "missing_dependency" or not issue.suggestion:
            continue
        dep_id = issue.suggestion.lower()
        if dep_id in seen or dep_id not in KNOWN_DEPS or KNOWN_DEPS[dep_id] is None:
            continue
        slug = KNOWN_DEPS[dep_id]
        try:
            versions = client.get_versions(slug, loader=loader, mc_version=mc_version)
        except Exception as e:
            logger.warning("Cannot resolve %s: %s", slug, e)
            continue
        if not versions:
            continue
        file = client.pick_file(versions[0])
        if not file:
            continue
        plan.append({
            "forMod": issue.mod,
            "depId": dep_id,
            "projectId": versions[0].get("project_id", slug),
            "filename": file["filename"],
            "url": file["url"],
            "sha1": file.get("hashes", {}).get("sha1"),
            "version": versions[0].get("version_number"),
        })
        seen.add(dep_id)
    return plan


def apply_fix_plan(ctx, instance_id: str, plan: list[dict],
                   task: Task) -> list[str]:
    """Tải + cài các dependency trong plan vào mods dir. Trả filenames."""
    dm = ctx.get("download_manager")
    mods_dir = ctx.paths.instances / instance_id / "game" / "mods"
    mods_dir.mkdir(parents=True, exist_ok=True)

    installed: list[str] = []
    total = len(plan)
    for i, item in enumerate(plan):
        if task.cancelled:
            raise AntaresError(codes.DOWNLOAD_CANCELLED, "Auto-fix cancelled")
        task.message = f"Downloading {item['filename']} ({i + 1}/{total})"
        task.progress = 100.0 * i / max(1, total)
        target = mods_dir / item["filename"]
        dm.download(item["url"], target, sha1=item.get("sha1"), task=task)
        installed.append(item["filename"])
    return installed


def auto_fix_instance(ctx, instance_id: str, *, loader: str, mc_version: str,
                      task: Task) -> dict:
    """Full auto-fix: scan mods dir -> plan -> download deps."""
    mods_dir = ctx.paths.instances / instance_id / "game" / "mods"
    from services.mods.health import ModInfo, read_mod_info, check_health
    mods: list[ModInfo] = []
    if mods_dir.exists():
        for f in sorted(mods_dir.glob("*.jar")):
            mods.append(read_mod_info(f))

    issues = check_health(mods, loader=loader, mc_version=mc_version)
    http = ctx.get("http_client")
    client = ModrinthClient(http)
    plan = build_fix_plan(issues, loader=loader, mc_version=mc_version, client=client)

    task.message = f"Resolving {len(plan)} missing dependencies"
    installed = apply_fix_plan(ctx, instance_id, plan, task)
    return {
        "issuesFound": len(issues),
        "dependenciesInstalled": installed,
        "plan": [{k: v for k, v in p.items() if k != "url"} for p in plan],
    }
