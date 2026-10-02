"""Multi-pack layering (master plan v3 mục 25 Batch 7 — install order).

Minecraft áp resource packs theo thứ tự: pack Ở DƯỚI trong list override
pack ở trên (pack cuối cùng trong options.txt `resourcePacks` thắng cho
path trùng). Antares lưu "layer order" = danh sách pack từ THẤP đến CAO
priority, và:

- `effective_asset(path)`: resolves path -> pack nào cung cấp, pack có
  priority CAO HƠN thắng (deterministic, mục 25 gate: deterministic output).
- `install_order()`: trả list file ZIP theo thứ tự ghi vào options.txt —
  pack thấp priority trước, cao sau (pack cao cuối cùng = thắng).
- Sync: sau reorder/uninstall, options.txt của instance được cập nhật.

Persistence: data/layering.json — {instance_id: {order: [zip_name,...]}}.
Atomic write; entry unknown (pack đã xoá) được lọc ra khi đọc.
"""
from __future__ import annotations

import json
import re
import zipfile
from pathlib import Path

from app.context import AppContext
from core.config.writer import write_json_atomic
from core.errors import codes
from core.errors.base import AntaresError
from core.logging.setup import get_logger

logger = get_logger("resources.layering")

_NAME_SAFE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9 ._-]{0,119}\.zip$")


class LayerStore:
    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx

    # ------------------------------------------------------------------
    # Persistence
    # ------------------------------------------------------------------

    @property
    def _path(self) -> Path:
        return self._ctx.paths.data / "layering.json"

    def _load(self) -> dict:
        if not self._path.is_file():
            return {}
        try:
            data = json.loads(self._path.read_text(encoding="utf-8"))
            return data if isinstance(data, dict) else {}
        except Exception:
            logger.warning("Corrupt layering.json — reset")
            return {}

    def _save(self, data: dict) -> None:
        write_json_atomic(self._path, data)

    # ------------------------------------------------------------------
    # Order API
    # ------------------------------------------------------------------

    def _rp_dir(self, instance_id: str) -> Path:
        inst = self._ctx.get("instances").get(instance_id)
        if not inst:
            raise AntaresError(codes.INSTANCE_NOT_FOUND, "Instance not found")
        return Path(self._ctx.paths.instances) / instance_id / "game" / "resourcepacks"

    def _installed_zips(self, instance_id: str) -> set[str]:
        rp = self._rp_dir(instance_id)
        return {p.name for p in rp.glob("*.zip")} if rp.exists() else set()

    def get_order(self, instance_id: str) -> list[str]:
        """Layer order hiện tại — pack THẤP priority trước.

        Lọc pack không còn tồn tại; pack mới install chưa có trong order
        được thêm vào cuối (priority thấp nhất).
        """
        self._rp_dir(instance_id)          # validate instance
        data = self._load()
        entry = data.get(instance_id) or {}
        order = [n for n in entry.get("order", [])
                 if isinstance(n, str) and _NAME_SAFE.match(n)]
        installed = self._installed_zips(instance_id)
        order = [n for n in order if n in installed]        # pack đã xoá -> loại
        for n in sorted(installed - set(order)):
            order.append(n)                                  # pack mới -> cuối
        return order

    def set_order(self, instance_id: str, order: list[str]) -> list[str]:
        """Lưu layer order (validate: đúng pack đã install, không dup) + sync."""
        rp = self._rp_dir(instance_id)
        installed = self._installed_zips(instance_id)
        clean: list[str] = []
        for n in order:
            if not isinstance(n, str) or not _NAME_SAFE.match(n):
                raise AntaresError(codes.VALIDATION_FAILED,
                                   f"Invalid pack name: {n!r}")
            if n not in installed:
                raise AntaresError(codes.FILE_NOT_FOUND,
                                   f"Pack chưa install: {n}")
            if n in clean:
                raise AntaresError(codes.VALIDATION_FAILED,
                                   f"Duplicate trong order: {n}")
            clean.append(n)
        # pack mới chưa có trong list -> thêm cuối
        for n in sorted(installed - set(clean)):
            clean.append(n)
        data = self._load()
        data[instance_id] = {"order": clean, "updatedAt": __import__("time").time()}
        self._save(data)
        self._sync_options(instance_id, clean)
        logger.info("Layer order %s: %s", instance_id, clean)
        return clean

    def move(self, instance_id: str, filename: str, delta: int) -> list[str]:
        """Đẩy pack lên/xuống trong layer (delta âm = lên = tăng priority)."""
        order = self.get_order(instance_id)
        if filename not in order:
            raise AntaresError(codes.FILE_NOT_FOUND, f"Pack không có: {filename}")
        i = order.index(filename)
        j = max(0, min(len(order) - 1, i + (-delta if delta < 0 else delta)))
        # delta=-1 -> lên 1 bậc priority
        step = -1 if delta < 0 else 1
        j = max(0, min(len(order) - 1, i + step * abs(delta)))
        order[i], order[j] = order[j], order[i]
        return self.set_order(instance_id, order)

    # ------------------------------------------------------------------
    # Effective asset resolution (deterministic — mục 25 gate)
    # ------------------------------------------------------------------

    def effective_asset(self, instance_id: str, asset_path: str) -> dict | None:
        """Resolve 1 asset path -> pack cung cấp có priority CAO NHẤT.

        asset_path dạng 'assets/minecraft/textures/item/apple.png'.
        Return {"pack": zip_name, "priority": rank} hoặc None nếu không pack
        nào cung cấp.
        """
        order = self.get_order(instance_id)
        rp = self._rp_dir(instance_id)
        # Duyệt từ CAO xuống THẤP — pack đầu tiên có file = thắng
        for rank in range(len(order) - 1, -1, -1):
            zp = rp / order[rank]
            try:
                with zipfile.ZipFile(zp) as zf:
                    names = set(zf.namelist())
                    if asset_path in names:
                        return {"pack": order[rank], "priority": rank}
            except (zipfile.BadZipFile, OSError):
                continue
        return None

    def preview_effective(self, instance_id: str) -> dict:
        """Preview effective asset cho TOÀN BỘ path trong các pack (mục 25).

        Return {"assets": {path: {"pack", "priority"}}, "conflicts":
        [{path, packs: [...]}]} — conflicts = path do >1 pack cung cấp.
        """
        order = self.get_order(instance_id)
        return self._preview_for(instance_id, order)

    def _preview_for(self, instance_id: str, order: list[str]) -> dict:
        rp = self._rp_dir(instance_id)
        providers: dict[str, list[dict]] = {}
        for rank, name in enumerate(order):
            zp = rp / name
            try:
                with zipfile.ZipFile(zp) as zf:
                    for n in zf.namelist():
                        if n.endswith("/"):
                            continue
                        providers.setdefault(n, []).append(
                            {"pack": name, "priority": rank})
            except (zipfile.BadZipFile, OSError):
                findings = {"code": "zip_malformed", "severity": "WARNING",
                            "path": name}
                logger.warning("Skipping corrupt pack %s: %s", name, findings)
                continue
        assets = {path: max(plist, key=lambda p: p["priority"])
                  for path, plist in providers.items()}
        conflicts = [{"path": p, "packs": [x["pack"] for x in plist]}
                     for p, plist in providers.items() if len(plist) > 1]
        return {"order": order, "assets": assets, "conflicts": conflicts}

    # ------------------------------------------------------------------
    # Install order — thứ tự ghi vào options.txt (deterministic)
    # ------------------------------------------------------------------

    def install_order(self, instance_id: str) -> list[str]:
        """Thứ tự pack cho options.txt resourcePacks: THẤP priority trước.

        Minecraft đọc list từ trên xuống; pack cuối cùng override pack trước
        cho path trùng — nên pack CAO priority phải đứng CUỐI.
        """
        return self.get_order(instance_id)      # order đã là thấp->cao

    def _sync_options(self, instance_id: str, order: list[str]) -> None:
        """Ghi resourcePacks vào options.txt của instance (giữ key lạ — merge)."""
        opts = Path(self._ctx.paths.instances) / instance_id / "game" / "options.txt"
        if not opts.parent.exists():
            return
        lines: list[str] = []
        if opts.is_file():
            lines = opts.read_text(encoding="utf-8", errors="replace").splitlines()
        # Minecraft list pack: ["file1","file2",...] — cao priority CUỐI
        value = "[" + ",".join(f'"{n}"' for n in order) + "]"
        out: list[str] = []
        found = False
        for line in lines:
            if line.startswith("resourcePacks:"):
                out.append(f"resourcePacks:{value}")
                found = True
            elif line.startswith("incompatibleResourcePacks:"):
                out.append("incompatibleResourcePacks:[]")
            else:
                out.append(line)
        if not found:
            out.append(f"resourcePacks:{value}")
        opts.write_text("\n".join(out) + "\n", encoding="utf-8")
