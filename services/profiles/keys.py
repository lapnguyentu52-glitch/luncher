"""Keys whitelist — mọi ghi options.txt của Profiles đi qua đây (mục 40).

GAME_KEYS = vanilla options.txt an toàn ghi trực tiếp (key:value).
           Coerce typed trước khi ghi; key lạ bị chặn ở validate_spec.

Giá trị RGB gamma/fov giữ kiểu số (vanilla hiểu 0..1). Hàm coerce() ở
service convert bool -> "true"/"false", số -> string khi ghi file.
"""
from __future__ import annotations

# (key, type) — type: int | float | bool | str
GAME_KEYS: dict[str, str] = {
    # video
    "gamma": "float",
    "fov": "float",
    "guiScale": "int",
    "maxFps": "int",
    "renderDistance": "int",
    "simulationDistance": "int",
    "brightness": "float",
    "fullscreen": "bool",
    "vsync": "bool",
    "cloudStatus": "str",
    "graphicsMode": "int",
    "ao": "bool",
    "entityShadows": "bool",
    "screenEffectScale": "float",
    "mipmapLevels": "int",
    "biomeBlendRadius": "int",
    "entityDistanceScaling": "float",
    "particles": "int",
    # controls
    "sensitivity": "float",
    "toggleSprint": "bool",
    "toggleCrouch": "bool",
    "invertYMouse": "bool",
    "mouseRawInput": "bool",
    # audio
    "soundCategory_master": "float",
    "soundCategory_music": "float",
    "soundCategory_hostile": "float",
    "soundCategory_players": "float",
    "soundCategory_weather": "float",
}

#: Thứ tự ổn định cho diff/preview (mục 10.5: preview không nhảy loạn).
GAME_KEY_ORDER: tuple[str, ...] = tuple(GAME_KEYS.keys())


def coerce(key: str, value) -> str:
    """Ép kiểu theo whitelist rồi stringify đúng vanilla (bool -> true/false)."""
    kind = GAME_KEYS.get(key, "str")
    if kind == "bool":
        if isinstance(value, str):
            return "true" if value.strip().lower() in ("true", "1", "yes", "on") else "false"
        return "true" if value else "false"
    if isinstance(value, bool):
        # bool đưa vào key không phải bool -> 1/0 thay vì True/False
        return "1" if value else "0"
    if kind == "int":
        return str(int(round(float(value))))
    if kind == "float":
        return str(float(value))
    return str(value)
