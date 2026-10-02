"""Server jar installer — từng software 1 nguồn API (mục 516-518).

Port từ Spark _srv_download_jar: Vanilla (Mojang), Paper/Velocity (fill.papermc.io),
Purpur (api.purpurmc.org), Fabric (meta.fabricmc.net).
"""
from __future__ import annotations

from app.context import AppContext
from core.errors import codes
from core.errors.base import AntaresError
from core.tasks.manager import Task
from core.utils.paths import human_size
from infrastructure.http.client import HttpClient


def install_server_jar(ctx: AppContext, meta: dict, task: Task) -> None:
    software = meta.get("software", "vanilla")
    version = meta.get("version", "")
    d = ctx.paths.servers / meta["id"]
    jar = d / "server.jar"
    if jar.exists() and software != "custom":
        return  # đã cài — idempotent (mục 338)

    http: HttpClient = ctx.get("http_client")
    dm = ctx.get("download_manager")

    def status(s: str) -> None:
        task.message = s

    try:
        if software == "vanilla":
            _install_vanilla(ctx, http, dm, version, jar, status)
        elif software in ("paper", "velocity"):
            _install_papermc(ctx, http, dm, software, version, jar, status)
        elif software == "purpur":
            _install_purpur(http, dm, version, jar, status)
        elif software == "fabric":
            _install_fabric(http, dm, version, jar, status)
        elif software == "custom":
            src = meta.get("jarSource")
            if not src:
                raise AntaresError(codes.VALIDATION_FAILED,
                                   "Custom server needs a jar file")
            shutil_copy(src, jar)
        else:
            raise AntaresError(codes.VALIDATION_FAILED, f"Unknown software: {software}")
    except AntaresError:
        raise
    except Exception as e:
        raise AntaresError(codes.SERVER_START_FAILED, f"Server install failed: {e}") from e


def _install_vanilla(ctx, http, dm, version, jar, status) -> None:
    from services.minecraft.versions.manifest import ManifestService
    from infrastructure.cache.manifests import DiskCache
    manifest = ManifestService(http, DiskCache(ctx.paths.cache))
    entry = manifest.find(version)
    if not entry:
        raise AntaresError(codes.MINECRAFT_VERSION_NOT_FOUND, f"Version {version} not found")
    info = http.get_json(entry["url"])
    dl = (info.get("downloads") or {}).get("server")
    if not dl:
        raise AntaresError(codes.MINECRAFT_VERSION_NOT_FOUND,
                           f"No server jar for {version}")
    status(f"Downloading Vanilla server {version} ({human_size(dl.get('size'))})...")
    dm.download(dl["url"], jar, sha1=dl.get("sha1"), task=_as_task_arg(status))


def _install_papermc(ctx, http, dm, software, version, jar, status) -> None:
    proj = "paper" if software == "paper" else "velocity"
    builds = http.get_json(
        f"https://fill.papermc.io/v3/projects/{proj}/versions/{version}/builds")
    if not builds:
        raise AntaresError(codes.SERVER_START_FAILED, f"No {proj} builds for {version}")
    b = max(builds, key=lambda x: x.get("id", 0))
    app = b["downloads"].get("server:default") or next(iter(b["downloads"].values()))
    status(f"Downloading {proj.capitalize()} {version} build {b['id']}...")
    dm.download(app["url"], jar, sha256=(app.get("checksums") or {}).get("sha256"),
                task=_as_task_arg(status))


def _install_purpur(http, dm, version, jar, status) -> None:
    info = http.get_json(f"https://api.purpurmc.org/v2/purpur/{version}")
    b = info["builds"]["latest"]
    status(f"Downloading Purpur {version} build {b}...")
    dm.download(f"https://api.purpurmc.org/v2/purpur/{version}/{b}/download", jar,
                task=_as_task_arg(status))


def _install_fabric(http, dm, version, jar, status) -> None:
    loaders = http.get_json(f"https://meta.fabricmc.net/v2/versions/loader/{version}")
    installer = http.get_json("https://meta.fabricmc.net/v2/versions/installer")[0]["version"]
    loader = loaders[0]["loader"]["version"]
    url = f"https://meta.fabricmc.net/v2/versions/loader/{version}/{loader}/{installer}/server/jar"
    status(f"Downloading Fabric server {version} (loader {loader})...")
    dm.download(url, jar, task=_as_task_arg(status))


def _as_task_arg(status_cb) -> dict:
    """download() của DownloadManager nhận task object; ở đây không có task thật
    nên truyền shim để progress cập nhật message."""
    class _Shim:
        cancelled = False
    shim = _Shim()
    shim.message = ""
    return shim


def shutil_copy(src: str, dst) -> None:
    import shutil
    shutil.copy2(src, dst)
