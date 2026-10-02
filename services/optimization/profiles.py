"""Optimization profiles — preset cho từng mục tiêu (spec 3.0 mục 3.2, 3.3).

Mỗi profile gồm 2 phần:
- `jvm`     : patch lên instance.json (memory.minMb/maxMb, jvmPreset, jvmArgs).
- `minecraft`: các key ghi vào game/options.txt (Render Distance, Particles...).

Nguyên tắc (mục 74 Safe defaults): KHÔNG profile nào đụng system-wide;
mọi thay đổi đều instance-local + có snapshot rollback (mục 70).
"""
from __future__ import annotations

PROFILES: dict[str, dict] = {
    "balanced": {
        "labelKey": "balanced",
        "descKey": "balancedDesc",
        "jvm": {"memory": None, "jvmPreset": "auto", "jvmArgs": []},
        "minecraft": {
            "renderDistance": 10, "simulationDistance": 10,
            "particles": 0, "clouds": "true", "entityShadows": True,
            "mipmapLevels": 4, "vsync": False, "maxFps": 120,
        },
    },
    "low_end": {
        "labelKey": "lowEnd",
        "descKey": "lowEndDesc",
        "jvm": {"memory": {"minMb": 512, "maxMb": 1536}, "jvmPreset": "g1", "jvmArgs": []},
        "minecraft": {
            "renderDistance": 6, "simulationDistance": 6,
            "particles": 2, "clouds": "false", "entityShadows": False,
            "mipmapLevels": 0, "vsync": False, "maxFps": 60,
            "biomeBlendRadius": 0, "entityDistanceScaling": 0.75,
        },
    },
    "performance": {
        "labelKey": "performance",
        "descKey": "performanceDesc",
        "jvm": {"memory": None, "jvmPreset": "balanced", "jvmArgs": []},
        "minecraft": {
            "renderDistance": 8, "simulationDistance": 8,
            "particles": 1, "clouds": "false", "entityShadows": False,
            "mipmapLevels": 2, "vsync": False, "maxFps": 260,
        },
    },
    "competitive": {
        "labelKey": "competitive",
        "descKey": "competitiveDesc",
        "jvm": {"memory": None, "jvmPreset": "balanced", "jvmArgs": []},
        "minecraft": {
            "renderDistance": 7, "simulationDistance": 6,
            "particles": 2, "clouds": "false", "entityShadows": False,
            "mipmapLevels": 0, "vsync": False, "maxFps": 260,
            "biomeBlendRadius": 0, "entityDistanceScaling": 0.75,
            "guiScale": 2,
        },
    },
    "visual": {
        "labelKey": "visual",
        "descKey": "visualDesc",
        "jvm": {"memory": None, "jvmPreset": "auto", "jvmArgs": []},
        "minecraft": {
            "renderDistance": 14, "simulationDistance": 12,
            "particles": 0, "clouds": "fast", "entityShadows": True,
            "mipmapLevels": 4, "vsync": True, "maxFps": 120,
            "entityDistanceScaling": 1.5,
        },
    },
    "battery": {
        "labelKey": "battery",
        "descKey": "batteryDesc",
        "jvm": {"memory": None, "jvmPreset": "g1", "jvmArgs": []},
        "minecraft": {
            "renderDistance": 6, "simulationDistance": 6,
            "particles": 2, "clouds": "false", "entityShadows": False,
            "mipmapLevels": 1, "vsync": True, "maxFps": 60,
        },
    },
}

#: Thứ tự hiển thị trong UI.
PROFILE_ORDER = ("balanced", "performance", "competitive", "low_end", "visual", "battery")


def get(profile_id: str) -> dict | None:
    return PROFILES.get(profile_id)


def memory_for_profile(profile_id: str, recommended: dict | None) -> dict | None:
    """Profile có `memory: None` = dùng memory recommended từ advisor (mục 72).

    Không tự đoán khi thiếu dữ liệu: recommended=None -> giữ nguyên memory.
    """
    mem = PROFILES.get(profile_id, {}).get("jvm", {}).get("memory")
    return mem if mem is not None else (recommended or None)
