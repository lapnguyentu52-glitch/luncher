"""LaunchOrchestrator — điều phối launch pipeline (mục 277-278).

PLAY -> validate -> resolve -> ensure -> JVM -> command -> spawn -> monitor.
Orchestrator KHÔNG trực tiếp HTTP/file — gọi service con.
"""
from __future__ import annotations

import time
import uuid

from app.context import AppContext
from core.events import names as ev
from core.errors import codes
from core.errors.base import AntaresError, MinecraftError
from core.logging.setup import get_logger
from core.tasks.manager import Task
from services.java.manager import JavaManager
from services.loaders.registry import get_loader
from services.minecraft.launch.jvm import JvmConfig, build_args, validate as jvm_validate
from services.accounts.service import AccountService
from infrastructure.fs.locks import ResourceLock

logger = get_logger("minecraft.launch")


class LaunchOrchestrator:
    def __init__(self, ctx: AppContext, java: JavaManager, accounts: AccountService) -> None:
        self._ctx = ctx
        self._java = java
        self._accounts = accounts
        self._processes = ctx.get("process_manager")

    def launch(self, instance_id: str, task: Task) -> dict:
        """Chạy full pipeline. Trả session dict. Raise AntaresError có action."""
        ctx = self._ctx
        instance = self._load_instance(instance_id)
        if not instance:
            raise AntaresError(codes.INSTANCE_NOT_FOUND, f"Instance '{instance_id}' không tồn tại")

        lock = ResourceLock(ctx.paths.instances / instance_id / ".lock")
        if not lock.acquire():
            raise AntaresError(codes.INSTANCE_LOCKED,
                               "Instance is already running or installing.")

        try:
            return self._pipeline(instance_id, instance, task)
        finally:
            lock.release()

    def _pipeline(self, instance_id: str, instance: dict, task: Task) -> dict:
        ctx = self._ctx
        events = ctx.events
        mc_version = instance["minecraftVersion"]
        loader_id = instance.get("loader", "vanilla")
        game_dir = str(ctx.paths.instances / instance_id / "game")

        # 1. validate account
        task.message = "Validating account"
        account = self._accounts.get_current()
        if not account:
            raise MinecraftError(codes.AUTH_FAILED, "No account selected",
                                 action="OPEN_ACCOUNTS")
        # (đã có action ở dòng trên — giữ nguyên để mapper nhảy tab)

        # 2. resolve version + loader
        task.message = "Resolving version"
        events.publish(ev.MINECRAFT_VALIDATING, {"instanceId": instance_id}, task_id=task.id)
        loader = get_loader(loader_id)
        resolved = loader.resolve_launch_version(mc_version)

        # 3. ensure installed (nếu thiếu -> cài)
        task.message = "Ensuring installed"
        if not self._is_installed(resolved, game_dir, ctx.paths.instances / instance_id):
            task.message = f"Installing {resolved}"
            events.publish(ev.MINECRAFT_INSTALL_PROGRESS, {"step": "install"},
                           task_id=task.id)
            resolved = loader.install(mc_version, game_dir, task)

        # 4. resolve java
        task.message = "Resolving Java"
        java_exe = self._resolve_java(mc_version, game_dir, task)

        # 5. JVM args
        task.message = "Building JVM args"
        jvm = JvmConfig(
            max_memory_mb=instance.get("memory", {}).get("maxMb", 2048),
            min_memory_mb=instance.get("memory", {}).get("minMb", 512),
            gc_mode=instance.get("jvmPreset", "auto"),
            custom_args=instance.get("jvmArgs", []),
        )
        for w in jvm_validate(jvm):
            logger.warning("JVM warning: %s", w)

        # 6. build command qua minecraft-launcher-lib
        task.message = "Building command"
        options = self._build_options(account, jvm, game_dir)
        # Player Profiles (mục 40): launch options được remember khi apply profile
        # (server / quickPlay / resolution) — 1 click Play là vào thẳng đích.
        try:
            profiles_svc = ctx.get("profiles")
            hint = profiles_svc.launch_hint() if profiles_svc else None
        except Exception:
            hint = None
        if hint:
            options.update(self._profile_launch_overrides(hint, instance_id))
        import minecraft_launcher_lib.command as mlc
        cmd = mlc.get_minecraft_command(resolved, game_dir, options)
        cmd = self._patch_java(cmd, java_exe, mc_version)

        # 7. spawn
        task.message = "Starting Minecraft"
        events.publish(ev.MINECRAFT_LAUNCHING, {"version": resolved}, task_id=task.id)
        session_id = uuid.uuid4().hex
        proc = self._processes.spawn(f"minecraft:{instance_id}", cmd, cwd=game_dir,
                                     on_line=lambda line: events.publish(
                                         ev.MINECRAFT_OUTPUT, {"line": line},
                                         task_id=task.id))

        events.publish(ev.MINECRAFT_STARTED,
                       {"instanceId": instance_id, "pid": proc.pid},
                       task_id=task.id)

        # Companion auto-pairing (mục 12): ghi endpoint+token vào game dir
        # để companion mod (nếu cài) tự kết nối IPC telemetry.
        try:
            runtime = ctx.get("runtime")
            if runtime:
                runtime.write_pairing_for_instance(instance_id)
        except Exception:
            pass

        def on_exit(code: int) -> None:
            events.publish(ev.MINECRAFT_EXITED, {
                "instanceId": instance_id, "exitCode": code,
            }, task_id=task.id)

        return {"sessionId": session_id, "pid": proc.pid, "version": resolved}

    def _is_installed(self, resolved: str, game_dir: str, instance_root) -> bool:
        # version json tồn tại trong instance hoặc data root (shared artifacts)
        from pathlib import Path
        for base in (Path(game_dir), Path(game_dir).parent.parent):
            if (base / "versions" / resolved / f"{resolved}.json").is_file():
                return True
        return False

    def _resolve_java(self, mc_version: str, game_dir: str, task: Task) -> str:
        java_exe = self._java.find_compatible(mc_version)
        if java_exe:
            return java_exe
        # fallback: Mojang runtime (port từ Spark _fix_java_for_version)
        exe = self._java.mojang_runtime_executable(mc_version, game_dir)
        if exe:
            return exe
        exe = self._java.install_mojang_runtime(
            mc_version, game_dir,
            status_cb=lambda s: setattr(task, "message", s))
        if exe:
            return exe
        from core.errors.base import JavaError
        from core.errors import codes as c
        raise JavaError(c.JAVA_NOT_FOUND,
                        f"Java not found for Minecraft {mc_version}.",
                        action="OPEN_JAVA_SETTINGS")

    def _build_options(self, account: dict, jvm: JvmConfig, game_dir: str) -> dict:
        return {
            "username": account["displayName"],
            "uuid": account.get("minecraftUuid") or uuid.uuid4().hex,
            "token": account.get("token", ""),
            "jvmArguments": build_args(jvm),
            "executablePath": "java",
            "gameDirectory": game_dir,
        }

    def _profile_launch_overrides(self, hint: dict,
                                  launching_instance_id: str | None = None) -> dict:
        """Chuyển launch options nhớ từ ProfileService sang options MLL (mục 40).

        Vé an toàn:
        - Chỉ ăn khi launch đúng instance profile đang áp (không nhảy chàm).
        - server + quickPlayMultiplayer xung đột -> server thắng (giống
          validate() của LaunchOptions — mục 285).
        """
        if not isinstance(hint, dict):
            return {}
        instance_id = hint.get("instanceId")
        if (instance_id and launching_instance_id
                and instance_id != launching_instance_id):
            return {}
        out: dict = {}
        if hint.get("server"):
            out["server"] = str(hint["server"])
            if hint.get("port"):
                out["port"] = str(hint["port"])
        for key in ("quickPlaySingleplayer", "quickPlayMultiplayer", "quickPlayRealms"):
            if hint.get(key):
                out[key] = str(hint[key])
        if hint.get("customResolution"):
            out["customResolution"] = True
            out["resolutionWidth"] = str(hint.get("resolutionWidth") or "854")
            out["resolutionHeight"] = str(hint.get("resolutionHeight") or "480")
        if hint.get("server") and out.get("quickPlayMultiplayer"):
            out.pop("quickPlayMultiplayer", None)
        return out

    def _patch_java(self, cmd: list[str], java_exe: str, mc_version: str) -> list[str]:
        """Đổi executable + lọc flag không tương thích (port từ Spark)."""
        cmd[0] = java_exe
        major = self._java.detect_major(java_exe)
        if major is None:
            return cmd

        def supported(flag: str) -> bool:
            if flag.startswith("--sun-misc-unsafe-memory-access"):
                return major >= 23
            if flag.startswith("--enable-native-access"):
                return major >= 21
            return True

        return [a for i, a in enumerate(cmd) if i == 0 or supported(a)]

    def _load_instance(self, instance_id: str) -> dict | None:
        from core.config.reader import read_json
        return read_json(self._ctx.paths.instances / instance_id / "instance.json")
