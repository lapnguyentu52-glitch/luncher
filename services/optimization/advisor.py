"""Hardware-aware advisor — đề xuất memory + profile (spec 3.0 mục 72).

Input  : RAM, CPU threads, GPU class (thiếu dữ liệu -> không đoán — mục 72).
Output : recommended heap, profile gợi ý, warnings, bottlenecks.
"""
from __future__ import annotations

from core.logging.setup import get_logger

logger = get_logger("optimization.advisor")


def recommend() -> dict:
    """Đọc hardware thật (psutil) -> khuyến nghị an toàn."""
    import os

    ram_total_mb = 0
    cpu_threads = os.cpu_count() or 2
    try:
        import psutil
        ram_total_mb = int(psutil.virtual_memory().total / (1024 * 1024))
        cpu_threads = psutil.cpu_count(logical=True) or cpu_threads
    except Exception:
        logger.warning("psutil unavailable — dùng fallback tối thiểu")

    return {
        "hardware": {
            "ramTotalMb": ram_total_mb,
            "cpuThreads": cpu_threads,
        },
        "memory": _recommend_memory(ram_total_mb),
        "profile": _recommend_profile(ram_total_mb, cpu_threads),
        "warnings": _warnings(ram_total_mb, cpu_threads),
        "bottlenecks": _bottlenecks(ram_total_mb, cpu_threads),
    }


def _recommend_memory(ram_total_mb: int) -> dict | None:
    """Heap đúng mức: 25–50% RAM, trần 8GB, sàn 1GB. Không đủ dữ liệu -> None."""
    if ram_total_mb <= 0:
        return None
    max_mb = min(8192, max(1024, int(ram_total_mb * 0.35)))
    min_mb = max(512, max_mb // 2)
    return {"minMb": min_mb, "maxMb": max_mb}


def _recommend_profile(ram_total_mb: int, cpu_threads: int) -> str:
    if ram_total_mb and ram_total_mb <= 4096:
        return "low_end"
    if cpu_threads <= 4:
        return "performance"
    return "balanced"


def _warnings(ram_total_mb: int, cpu_threads: int) -> list[str]:
    out = []
    if ram_total_mb and ram_total_mb < 4096:
        out.append("lowRam")
    if cpu_threads <= 2:
        out.append("fewCores")
    return out


def _bottlenecks(ram_total_mb: int, cpu_threads: int) -> list[str]:
    out = []
    if ram_total_mb and ram_total_mb < 8192:
        out.append("ram")
    if cpu_threads <= 4:
        out.append("cpu")
    return out
