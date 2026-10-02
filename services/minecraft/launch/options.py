"""Launch options — học từ minecraft_loader/command.py get_minecraft_command docs.

Học được: options dict hỗ trợ customResolution, server+port connect after start,
demo mode, disableMultiplayer/disableChat, quickPlay* (1.20+).
"""
from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class LaunchOptions:
    """Options chuẩn hoá trước khi convert sang minecraft-launcher-lib options."""

    # auth (bắt buộc)
    username: str = ""
    uuid: str = ""
    token: str = ""

    # game
    game_directory: str | None = None
    demo: bool = False
    custom_resolution: bool = False
    resolution_width: str = "854"
    resolution_height: str = "480"

    # connect after start (học từ options["server"]/options["port"])
    server: str | None = None
    server_port: str | None = None

    # quick play (1.20+) — học từ quickPlayPath/Singleplayer/Multiplayer/Realms
    quick_play_singleplayer: str | None = None
    quick_play_multiplayer: str | None = None
    quick_play_realms: str | None = None

    # restrictions (học disableMultiplayer/disableChat)
    disable_multiplayer: bool = False
    disable_chat: bool = False

    # runtime
    executable_path: str = "java"
    jvm_arguments: list[str] = field(default_factory=list)

    def to_mll_options(self) -> dict:
        """Convert sang options dict của minecraft-launcher-lib (mục 0: lib ở adapter)."""
        opts: dict = {
            "username": self.username,
            "uuid": self.uuid,
            "token": self.token,
            "executablePath": self.executable_path,
            "jvmArguments": self.jvm_arguments,
            "launcherName": "Antares-Launcher",
            "launcherVersion": "2.0",
        }
        if self.game_directory:
            opts["gameDirectory"] = self.game_directory
        if self.demo:
            opts["demo"] = True
        if self.custom_resolution:
            opts["customResolution"] = True
            opts["resolutionWidth"] = self.resolution_width
            opts["resolutionHeight"] = self.resolution_height
        if self.server:
            opts["server"] = self.server
        if self.server_port:
            opts["port"] = self.server_port
        if self.disable_multiplayer:
            opts["disableMultiplayer"] = True
        if self.disable_chat:
            opts["disableChat"] = True
        if self.quick_play_singleplayer:
            opts["quickPlaySingleplayer"] = self.quick_play_singleplayer
        if self.quick_play_multiplayer:
            opts["quickPlayMultiplayer"] = self.quick_play_multiplayer
        if self.quick_play_realms:
            opts["quickPlayRealms"] = self.quick_play_realms
        return opts


def validate(options: LaunchOptions) -> list[str]:
    """Validate trước khi build command (mục 285)."""
    warnings = []
    if not options.username:
        warnings.append("Username is required.")
    if options.custom_resolution:
        try:
            w, h = int(options.resolution_width), int(options.resolution_height)
            if w < 1 or h < 1:
                warnings.append("Resolution must be positive integers.")
        except ValueError:
            warnings.append("Resolution must be numbers.")
    if options.quick_play_multiplayer and options.server:
        warnings.append("Both quickPlayMultiplayer and server set — server wins.")
    return warnings
