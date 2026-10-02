"""HUD Studio backend — widget layout schema (spec 3.0 mục 11).

HUD runtime hiển thị là việc của companion mod (mục 12); launcher quản lý
**layout profile**: widget nào, toạ độ, scale, visible. Validate chặt (mục 76):
tọa độ trong 0..819, scale 0.5..4, widget id trong danh sách cho phép.

Layout lưu trong visual project (mục 34: JSON, không binary).
"""
from __future__ import annotations

from core.errors import codes
from core.errors.base import AntaresError

#: Widgets hỗ trợ (mục 11).
WIDGETS = (
    "fps", "cps", "coordinates", "armor", "potions", "clock", "ping",
    "server", "biome", "facing", "memory", "keystrokes", "target_info",
    "session_time",
)

#: Widget mặc định bật (gọn — mục 25.4: không crowded).
DEFAULT_LAYOUT = [
    {"id": "fps", "x": 4, "y": 4, "scale": 1.0, "visible": True},
    {"id": "coordinates", "x": 4, "y": 20, "scale": 1.0, "visible": True},
    {"id": "clock", "x": 4, "y": 36, "scale": 1.0, "visible": False},
]

LIMITS = {"x": (0, 819), "y": (0, 459), "scale": (0.5, 4.0)}


def validate(layout) -> list[dict]:
    """Trả danh sách lỗi rỗng nếu hợp lệ."""
    errors: list[dict] = []
    if not isinstance(layout, list):
        return [{"widget": "?", "error": "layout must be a list"}]
    seen = set()
    for i, w in enumerate(layout):
        if not isinstance(w, dict) or w.get("id") not in WIDGETS:
            errors.append({"widget": f"[{i}]", "error": "unknown widget id"})
            continue
        wid = w["id"]
        if wid in seen:
            errors.append({"widget": wid, "error": "duplicate widget"})
        seen.add(wid)
        for key, (lo, hi) in LIMITS.items():
            try:
                v = float(w.get(key, 0 if key != "scale" else 1.0))
            except (TypeError, ValueError):
                errors.append({"widget": wid, "error": f"{key} not a number"})
                continue
            if not (lo <= v <= hi):
                errors.append({"widget": wid, "error": f"{key}={v} out of {lo}..{hi}"})
    return errors


def sanitize(layout) -> list[dict]:
    """Clamp về khoảng hợp lệ — dùng khi persist."""
    if not isinstance(layout, list):
        return [dict(w) for w in DEFAULT_LAYOUT]
    out, seen = [], set()
    for w in layout:
        if not isinstance(w, dict) or w.get("id") not in WIDGETS or w["id"] in seen:
            continue
        clean = {"id": w["id"], "visible": bool(w.get("visible", True))}
        for key, (lo, hi) in LIMITS.items():
            default = 0 if key != "scale" else 1.0
            try:
                v = float(w.get(key, default))
            except (TypeError, ValueError):
                v = default
            v = min(hi, max(lo, v))
            clean[key] = round(v, 2) if key == "scale" else int(v)
        out.append(clean)
        seen.add(w["id"])
    return out


def merge_default(layout) -> list[dict]:
    """Sanitize + đảm bảo đủ widget mặc định (widget chưa cấu hình -> default)."""
    clean = sanitize(layout)
    have = {w["id"] for w in clean}
    for w in DEFAULT_LAYOUT:
        if w["id"] not in have:
            clean.append(dict(w))
    return clean
