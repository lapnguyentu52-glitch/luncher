"""JVM builder — typed object thay vì chuỗi (mục 14, 284-285).

`["-Xmx4096M", ...]` an toàn hơn `"-Xmx4096M ..."` về quoting/escaping.
"""
from __future__ import annotations

import os
from dataclasses import dataclass, field


@dataclass
class JvmConfig:
    min_memory_mb: int = 512
    max_memory_mb: int = 2048
    gc_mode: str = "auto"          # auto | g1 | balanced
    custom_args: list[str] = field(default_factory=list)
    system_properties: dict[str, str] = field(default_factory=dict)

    def __post_init__(self) -> None:
        if self.max_memory_mb < self.min_memory_mb:
            self.min_memory_mb = self.max_memory_mb


PRESETS = {
    "default": dict(gc_mode="auto"),
    "balanced": dict(gc_mode="balanced"),
    "low_memory": dict(gc_mode="g1"),
    "performance": dict(gc_mode="balanced"),
}


def validate(config: JvmConfig) -> list[str]:
    """Trả danh sách cảnh báo (mục 285) — không tự sửa silent."""
    warnings = []
    if config.max_memory_mb < 512:
        warnings.append("Max memory < 512MB — game có thể crash.")
    if config.max_memory_mb < config.min_memory_mb:
        warnings.append("Max memory < min memory.")
    for arg in config.custom_args:
        if arg.startswith(("-Xmx", "-Xms")):
            warnings.append(f"Custom arg '{arg}' conflicts with memory settings.")
    return warnings


def build_args(config: JvmConfig) -> list[str]:
    """JvmConfig -> argv list."""
    args: list[str] = [
        f"-Xms{config.min_memory_mb}M",
        f"-Xmx{config.max_memory_mb}M",
    ]
    if config.gc_mode == "g1" or (config.gc_mode == "auto" and _low_end()):
        args += ["-XX:+UseG1GC", "-XX:MaxGCPauseMillis=50"]
    elif config.gc_mode == "balanced":
        cpu = os.cpu_count() or 2
        args += ["-XX:+UnlockExperimentalVMOptions",
                 f"-XX:ParallelGCThreads={max(1, cpu // 2)}"]
    for k, v in config.system_properties.items():
        args.append(f"-D{k}={v}")
    args += [a for a in config.custom_args if a and not a.startswith(("-Xmx", "-Xms"))]
    return args


def _low_end() -> bool:
    try:
        import psutil
        return psutil.virtual_memory().total <= 4 * 1024**3 or (os.cpu_count() or 1) <= 2
    except Exception:
        return False
