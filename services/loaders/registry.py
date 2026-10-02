"""Loader registry — adapter lên minecraft-launcher-lib (spec 0: 'minecraft-launcher-lib ở adapter').

Port từ Spark: _load_*_versions + download() cho từng loader.
"""
from __future__ import annotations

from core.errors import codes
from core.errors.base import AntaresError
from core.logging.setup import get_logger
from core.tasks.manager import Task
from services.loaders.base import LoaderProvider

logger = get_logger("loaders")


class VanillaProvider:
    id = "vanilla"

    def list_versions(self) -> list[dict]:
        import minecraft_launcher_lib.utils as mlu
        return mlu.get_available_versions("")

    def resolve_launch_version(self, mc_version: str, loader_version: str | None = None) -> str:
        return mc_version

    def install(self, mc_version: str, game_dir: str, task: Task | None = None) -> str:
        import minecraft_launcher_lib.install as mli
        cb = _callback(task)
        mli.install_minecraft_version(mc_version, game_dir, callback=cb)
        return mc_version


class FabricProvider:
    id = "fabric"

    def list_versions(self) -> list[str]:
        from minecraft_launcher_lib.fabric import get_stable_minecraft_versions
        return sorted(get_stable_minecraft_versions(), reverse=True)

    def resolve_launch_version(self, mc_version: str, loader_version: str | None = None) -> str:
        from minecraft_launcher_lib.fabric import get_latest_loader_version
        lv = loader_version or get_latest_loader_version()
        return f"fabric-loader-{lv}-{mc_version}"

    def install(self, mc_version: str, game_dir: str, task: Task | None = None) -> str:
        from minecraft_launcher_lib.fabric import install_fabric
        cb = _callback(task)
        install_fabric(mc_version, game_dir, callback=cb)
        return self.resolve_launch_version(mc_version)


class QuiltProvider:
    id = "quilt"

    def list_versions(self) -> list[str]:
        from minecraft_launcher_lib import quilt
        return sorted(quilt.get_stable_minecraft_versions(), reverse=True)

    def resolve_launch_version(self, mc_version: str, loader_version: str | None = None) -> str:
        from minecraft_launcher_lib import quilt
        lv = loader_version or quilt.get_latest_loader_version()
        return f"quilt-loader-{lv}-{mc_version}"

    def install(self, mc_version: str, game_dir: str, task: Task | None = None) -> str:
        from minecraft_launcher_lib import quilt
        cb = _callback(task)
        quilt.install_quilt(mc_version, game_dir, callback=cb, java="java")
        return self.resolve_launch_version(mc_version)


class ForgeProvider:
    id = "forge"

    def list_versions(self) -> list[str]:
        from minecraft_launcher_lib.forge import list_forge_versions
        return sorted(list_forge_versions(), reverse=True)

    def resolve_launch_version(self, mc_version: str, loader_version: str | None = None) -> str:
        # Spark convention: "1.20.1-47.2.0" folder tên "{mc}-forge-{build}"
        if "-" in mc_version:
            mc, build = mc_version.split("-", 1)
            return f"{mc}-forge-{build}"
        return mc_version

    def install(self, mc_version: str, game_dir: str, task: Task | None = None) -> str:
        from minecraft_launcher_lib.forge import (
            install_forge_version, run_forge_installer, supports_automatic_install,
        )
        cb = _callback(task)
        for attempt in range(3):
            try:
                if supports_automatic_install(mc_version):
                    install_forge_version(mc_version, game_dir, callback=cb)
                else:
                    run_forge_installer(mc_version)
                return self.resolve_launch_version(mc_version)
            except Exception as e:
                if attempt == 2:
                    raise AntaresError(
                        codes.LOADER_INSTALL_FAILED,
                        f"Forge install failed: {e}") from e
                logger.warning("Forge install retry %d: %s", attempt + 1, e)
        raise AntaresError(codes.LOADER_INSTALL_FAILED, "unreachable")


class NeoForgeProvider:
    id = "neoforge"

    def list_versions(self) -> list[str]:
        from minecraft_launcher_lib import mod_loader
        nf = mod_loader.Neoforge()
        return sorted(nf.get_minecraft_versions(True), reverse=True)

    def resolve_launch_version(self, mc_version: str, loader_version: str | None = None) -> str:
        from minecraft_launcher_lib import mod_loader
        nf = mod_loader.Neoforge()
        loaders = nf.get_loader_versions(mc_version, True)
        if not loaders:
            raise AntaresError(codes.LOADER_INSTALL_FAILED,
                               f"No NeoForge loader for {mc_version}")
        return nf.get_installed_version(mc_version, loaders[0])

    def install(self, mc_version: str, game_dir: str, task: Task | None = None) -> str:
        from minecraft_launcher_lib import mod_loader
        nf = mod_loader.Neoforge()
        loaders = nf.get_loader_versions(mc_version, True)
        if not loaders:
            raise AntaresError(codes.LOADER_INSTALL_FAILED,
                               f"No NeoForge loader for {mc_version}")
        cb = _callback(task)
        nf.install(mc_version, game_dir, cb, "java", loaders[0])
        return self.resolve_launch_version(mc_version)


_REGISTRY: dict[str, type] = {
    "vanilla": VanillaProvider,
    "fabric": FabricProvider,
    "quilt": QuiltProvider,
    "forge": ForgeProvider,
    "neoforge": NeoForgeProvider,
}


def get_loader(loader_id: str) -> LoaderProvider:
    cls = _REGISTRY.get(loader_id)
    if not cls:
        raise AntaresError(codes.VALIDATION_FAILED, f"Unknown loader: {loader_id}")
    return cls()


def loader_ids() -> list[str]:
    return list(_REGISTRY)


def _callback(task: Task | None) -> dict:
    def set_status(text: str) -> None:
        if task:
            task.message = text

    def set_progress(value: float) -> None:
        if task:
            task.progress = max(task.progress, value)

    def set_max(value: float) -> None:
        pass

    return {"setStatus": set_status, "setProgress": set_progress, "setMax": set_max}
