"""Version adapter — pack metadata theo Minecraft version (spec v3 mục 6).

ĐÂY LÀ NƠI DUY NHẤT trong codebase biết về pack_format / pack.mcmeta schema /
item model mode. Mọi module cần format phải gọi API ở đây, không hardcode:

    versioning.parse(mc_version)         -> (major, minor, patch)
    versioning.pack_meta(mc_version)     -> dict cho pack.mcmeta
    versioning.pack_format(mc_version)   -> int major format (legacy API)
    versioning.item_model_mode(v)        -> "legacy" | "modern" | "definition"
    versioning.mcmeta_mode(v)            -> "pack_format" | "min_max"

Policy version lạ (mục 6.2):
- Chưa có trong bảng nhưng LỚN HƠN mốc mới nhất  -> clamp về mốc mới nhất
  + WARNING. Không fail, không fallback âm thầm về version khác khi build.
- CŨ HƠN mốc cũ nhất (pre-1.6.1)                 -> pack_format 1 + WARNING.

Bảng format: xác minh 2026-09-26 từ minecraft.wiki/w/Pack_format,
packsmc.com/articles/minecraft-pack-format-list và changelog chính thức
Mojang (misode.github.io/changelog). Sửa bảng phải kèm test mới + ghi nguồn.
"""
from __future__ import annotations

import re
from dataclasses import dataclass

#: (version_tuple, resource pack format) — sắp xếp GIẢM dần, chọn mục đầu
#: tiên thoả `target >= known`. Format 69.0+ có minor; lưu minor ở cột thứ 3.
#: Xác minh 2026-09-26 — nguồn xem docstring module.
_FORMATS: tuple[tuple[tuple[int, int, int], int, int], ...] = (
    ((26, 2, 0), 88, 0),
    ((26, 1, 0), 84, 0),
    ((1, 21, 11), 75, 0),
    ((1, 21, 9), 69, 0),
    ((1, 21, 7), 64, 0),
    ((1, 21, 6), 63, 0),
    ((1, 21, 5), 55, 0),
    ((1, 21, 4), 46, 0),
    ((1, 21, 2), 42, 0),
    ((1, 21, 0), 34, 0),
    ((1, 20, 5), 32, 0),
    ((1, 20, 3), 22, 0),
    ((1, 20, 2), 18, 0),
    ((1, 20, 0), 15, 0),
    ((1, 19, 4), 13, 0),
    ((1, 19, 3), 12, 0),
    ((1, 19, 0), 9, 0),
    ((1, 18, 0), 8, 0),
    ((1, 17, 0), 7, 0),
    ((1, 16, 2), 6, 0),
    ((1, 15, 0), 5, 0),
    ((1, 13, 0), 4, 0),
    ((1, 11, 0), 3, 0),
    ((1, 9, 0), 2, 0),
    ((1, 6, 1), 1, 0),
)

#: Format thấp nhất đọc được min_format/max_format (1.21.9 = 69).
#: Trước đó: supported_formats (1.20.2+), trước nữa: chỉ pack_format.
MIN_MAX_MIN_FORMAT = 69

#: 1.21.4 (format 46) tách item model definition sang assets/<ns>/items/.
ITEM_MODEL_DEFINITION_FORMAT = 46

#: 1.20.2 (format 18) đổi texture path: gui/crosshair.png -> gui/sprites/icon/.
SPRITE_ICON_FORMAT = 18


@dataclass(frozen=True)
class VersionInfo:
    """Kết quả parse + policy cho 1 MC version."""

    version: str
    parsed: tuple[int, int, int]
    pack_format: int                 # int major format
    format_minor: int                # minor cho format 69.0+
    clamped: bool                    # True nếu version chưa có trong bảng
    warning: str | None              # warning cho policy version lạ

    @property
    def format_float(self) -> float:
        """Format dạng major.minor (69.0). Dùng khi hiển thị/test."""
        return self.pack_format + self.format_minor / 10.0


def parse(version: str) -> tuple[int, int, int]:
    """'1.21.4' -> (1, 21, 4); '1.21' -> (1, 21, 0). Chấp nhận '26.2'."""
    parts = [int(x) for x in re.findall(r"\d+", str(version))[:3]]
    while len(parts) < 3:
        parts.append(0)
    return (parts[0], parts[1], parts[2])


def _lookup(target: tuple[int, int, int]) -> tuple[int, int, bool]:
    """Trả về (major_format, minor_format, clamped)."""
    known, fmt_major, fmt_minor = _FORMATS[0]
    if target >= known:
        return fmt_major, fmt_minor, target != known
    for known, fmt_major, fmt_minor in _FORMATS:
        if target >= known:
            return fmt_major, fmt_minor, False
    return 1, 0, True   # pre-1.6.1


_INFO_CACHE: dict[str, VersionInfo] = {}


def info(mc_version: str) -> VersionInfo:
    """Parse + policy 1 lần — mọi API khác đi qua đây.

    Batch 9: cache theo version string — hot path (mọi build/validate gọi)
    tránh re.findall mỗi lần. Version set bounded (UI chọn từ list) nên dict
    cache an toàn; cap 512 phòng input lạ.
    """
    key = str(mc_version)
    cached = _INFO_CACHE.get(key)
    if cached is not None:
        return cached
    v = parse(key)
    major, minor, clamped = _lookup(v)
    warning = None
    if clamped:
        latest = _FORMATS[0][0]
        if v > latest:
            warning = (f"Minecraft {mc_version} mới hơn mốc đã xác minh "
                       f"({latest[0]}.{latest[1]}.{latest[2]}): dùng format "
                       f"{major} — Mojang có thể yêu cầu format mới hơn.")
        else:
            warning = (f"Minecraft {mc_version} cũ hơn phiên bản hỗ trợ "
                       f"(pre-1.6.1): dùng pack_format 1.")
    vi = VersionInfo(version=key, parsed=v, pack_format=major,
                     format_minor=minor, clamped=clamped, warning=warning)
    if len(_INFO_CACHE) < 512:
        _INFO_CACHE[key] = vi
    return vi


# ----------------------------------------------------------------------
# Public API (mục 6.1 — mọi nơi gọi qua đây)
# ----------------------------------------------------------------------

def pack_format(mc_version: str) -> int:
    """Major resource pack format cho MC version."""
    return info(mc_version).pack_format


def pack_format_value(mc_version: str) -> int | list[int]:
    """Giá trị ghi vào field pack_format.

    Từ format 69.0 (1.21.9+) game hiểu cả int lẫn mảng [major, minor] —
    ghi chuẩn hoá dạng [major, minor] để khớp cách game hiển thị (69.0);
    trước đó là int thuần.
    """
    vi = info(mc_version)
    if vi.pack_format >= MIN_MAX_MIN_FORMAT:
        return [vi.pack_format, vi.format_minor]
    return vi.pack_format


def mcmeta_mode(mc_version: str) -> str:
    """'min_max' từ format 69 (1.21.9+); 'pack_format' cho version cũ."""
    return "min_max" if info(mc_version).pack_format >= MIN_MAX_MIN_FORMAT else "pack_format"


def pack_meta(mc_version: str, description: str | None = None) -> dict:
    """pack.mcmeta payload version-aware (mục 6.3).

    - < 1.21.9:  { pack_format, description }
    - >= 1.21.9: { pack_format, min_format, max_format, description }
      (pack_format giữ lại để pack vẫn load được trên game cũ hơn format 65)
    """
    vi = info(mc_version)
    desc = description or default_description(mc_version)
    pack: dict = {"pack_format": pack_format_value(mc_version), "description": desc}
    if vi.pack_format >= MIN_MAX_MIN_FORMAT:
        pack["min_format"] = vi.pack_format
        # max cao hơn 1 major: pack vẫn sạch trên bản tiếp theo (mục 90 packs)
        pack["max_format"] = vi.pack_format + 10
    return {"pack": pack}


def default_description(mc_version: str) -> str:
    return f"Antares resource pack (MC {mc_version})"


def item_model_mode(mc_version: str) -> str:
    """'definition' (1.21.4+): assets/<ns>/items/<item>.json + models/item/.
    'legacy': chỉ models/item/<item>.json với override/predicate."""
    return "definition" if info(mc_version).pack_format >= ITEM_MODEL_DEFINITION_FORMAT else "legacy"


def crosshair_path(mc_version: str) -> str:
    """Relative path texture crosshair trong pack (1.20.2+ đổi sang sprites/)."""
    return ("assets/minecraft/textures/gui/sprites/icon/crosshair.png"
            if info(mc_version).pack_format >= SPRITE_ICON_FORMAT
            else "assets/minecraft/textures/gui/crosshair.png")


def supported_versions() -> list[str]:
    """Danh sách version đã xác minh, mới nhất trước — cho wizard UI."""
    seen: list[str] = []
    for known, _f, _m in _FORMATS:
        s = f"{known[0]}.{known[1]}" + (f".{known[2]}" if known[2] else "")
        if s not in seen:
            seen.append(s)
    return seen


def version_warnings(mc_version: str) -> list[str]:
    """Warning cho policy version lạ ([] nếu version đã xác minh)."""
    w = info(mc_version).warning
    return [w] if w else []
