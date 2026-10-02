"""SystemOptimizationService — overview hợp nhất (spec 3.0 mục 4.1-4.3, 35).

Gộp 3 module con (cleaner/power/process) thành một surface cho API.
Mọi action trả về theo safety model mục 35: risk / reversible / requiresAdmin.
"""
from __future__ import annotations

from app.context import AppContext
from services.system.cleaner import DiskCleaner
from services.system.power import PowerService
from services.system.process import ProcessService


class SystemOptimizationService:
    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx
        self.cleaner = DiskCleaner(ctx)
        self.power = PowerService()
        self.process = ProcessService(ctx.get("process_manager"))

    # ------------------------------------------------------------------
    # Overview (mục 4.2 hardware info — tái dùng advisor của game optimization)
    # ------------------------------------------------------------------

    def overview(self) -> dict:
        from services.optimization.advisor import recommend

        rec = recommend()
        hw = rec["hardware"]
        extra: dict = {}
        try:
            import psutil
            vm = psutil.virtual_memory()
            extra = {
                "ramUsedMb": int(vm.used / (1024 * 1024)),
                "ramPercent": round(vm.percent, 1),
            }
            if getattr(psutil, "sensors_battery", None):
                batt = psutil.sensors_battery()
                if batt is not None:
                    extra["battery"] = {
                        "percent": round(batt.percent, 0),
                        "plugged": bool(batt.power_plugged),
                    }
        except Exception:
            pass

        return {
            "hardware": {
                "ramTotalMb": hw["ramTotalMb"],
                "cpuThreads": hw["cpuThreads"],
                **extra,
            },
            "power": self.power.status(),
            "cleanupPreview": self.cleaner.scan(),
        }

    # ------------------------------------------------------------------
    # Delegates
    # ------------------------------------------------------------------

    def cleanup_scan(self) -> dict:
        return self.cleaner.scan()

    def cleanup_clean(self, paths: list[str]) -> dict:
        return self.cleaner.clean(paths)

    def cleanup_undo(self, clean_id: str) -> dict:
        return self.cleaner.undo(clean_id)

    def cleanup_empty_trash(self) -> int:
        return self.cleaner.empty_trash()

    def power_status(self) -> dict:
        return self.power.status()

    def power_set_plan(self, plan_id: str) -> bool:
        return self.power.set_plan(plan_id)

    def process_info(self, pid: int) -> dict | None:
        return self.process.info(pid)

    def process_apply_priority(self, key: str, level: str) -> bool:
        return self.process.apply_priority(key, level)

    def process_apply_affinity(self, key: str, cores: list[int] | None) -> bool:
        return self.process.apply_affinity(key, cores)
