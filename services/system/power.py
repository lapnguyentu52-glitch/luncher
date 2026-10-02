"""Power plan — hint-first, không tự sửa (spec 3.0 mục 4.3 Power, 35).

- Đọc plan hiện tại qua `powercfg /getactivescheme` (Windows) — read-only.
- Đề xuất High performance khi game chạy — chỉ HINT; đổi plan là action
  người dùng bấm (requiresAdmin=True, reversible), KHÔNG tự áp (mục 74).
- Trên OS khác: báo unsupported, không fail app.
"""
from __future__ import annotations

import re
import subprocess

from core.logging.setup import get_logger

logger = get_logger("system.power")

_PLANS = {
    "a1841308-3541-4fab-bc81-f71556f20b4a": "powerSaver",
    "381b4222-f694-41f0-9685-ff5bb260df2e": "balanced",
    "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c": "highPerformance",
}


class PowerService:
    def __init__(self) -> None:
        import os
        self._windows = os.name == "nt"

    def status(self) -> dict:
        if not self._windows:
            return {"supported": False, "plan": None, "recommendation": None}
        plan_id, plan_name = self._current_plan()
        rec = self._recommendation(plan_id)
        return {
            "supported": True,
            "plan": {"id": plan_id, "name": plan_name,
                     "labelKey": _PLANS.get(plan_id.lower(), "") if plan_id else ""},
            "recommendation": rec,   # None = không cần đổi
        }

    # ------------------------------------------------------------------

    def _current_plan(self) -> tuple[str | None, str | None]:
        try:
            r = subprocess.run(["powercfg", "/getactivescheme"],
                               capture_output=True, text=True, timeout=5)
            out = (r.stdout or "") + (r.stderr or "")
            m = re.search(r"([0-9a-fA-F-]{36})\s*\(([^)]+)\)", out)
            if m:
                return m.group(1).lower(), m.group(2).strip()
        except Exception as e:
            logger.debug("powercfg failed: %s", e)
        return None, None

    @staticmethod
    def _recommendation(plan_id: str | None) -> dict | None:
        """Hint đổi plan khi đang Power Saver (an toàn, reversible — mục 35)."""
        if not plan_id:
            return None
        if plan_id.lower() == "a1841308-3541-4fab-bc81-f71556f20b4a":
            return {
                "to": "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c",
                "toLabelKey": "highPerformance",
                "reasonKey": "powerSaverHint",
                "risk": "LOW", "reversible": True, "requiresAdmin": True,
            }
        return None

    # ------------------------------------------------------------------
    # Action — chỉ chạy khi user bấm (UI confirm); requiresAdmin (mục 35)
    # ------------------------------------------------------------------

    def set_plan(self, plan_id: str) -> bool:
        if not self._windows:
            return False
        if plan_id.lower() not in _PLANS:
            return False
        try:
            r = subprocess.run(["powercfg", "/setactive", plan_id],
                               capture_output=True, text=True, timeout=10)
            ok = r.returncode == 0
            if not ok:
                logger.warning("powercfg setactive failed: %s", (r.stderr or "").strip())
            return ok
        except Exception as e:
            logger.warning("powercfg setactive error: %s", e)
            return False
