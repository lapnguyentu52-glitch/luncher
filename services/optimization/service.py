"""GameOptimizationService — plan/apply/rollback có snapshot (spec 3.0 mục 3, 70).

Flow (mục 3.1):
  scan -> recommendation -> preview diff -> backup snapshot -> apply -> rollback (tuỳ chọn)

Nguyên tắc an toàn:
- Mọi thay đổi là instance-local (instance.json + game/options.txt) — KHÔNG
  đụng system-wide (mục 74).
- Trước khi ghi luôn tạo snapshot `optimization-backup.json` — rollback 1 click
  (mục 70: Before / Changes / After / Rollback).
- Không sửa file khi instance đang chạy (lock bởi launch pipeline).
"""
from __future__ import annotations

import time
from pathlib import Path

from app.context import AppContext
from core.errors import codes
from core.errors.base import AntaresError
from core.events import names as ev
from core.logging.setup import get_logger
from services.optimization import advisor, profiles

logger = get_logger("optimization")

#: key Minecraft hiển thị diff theo thứ tự cố định để preview ổn định.
_OPTIONS_KEYS = (
    "renderDistance", "simulationDistance", "particles", "clouds",
    "entityShadows", "mipmapLevels", "vsync", "maxFps",
    "biomeBlendRadius", "entityDistanceScaling", "guiScale", "fullscreen",
)


class GameOptimizationService:
    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx

    # ------------------------------------------------------------------
    # Scan + recommendation
    # ------------------------------------------------------------------

    def scan(self, instance_id: str | None = None) -> dict:
        """Hardware scan + tình trạng instance + recommendation (mục 3.1)."""
        rec = advisor.recommend()
        instance = None
        current_profile = None
        if instance_id:
            instance = self._ctx.get("instances").get(instance_id)
            if not instance:
                raise AntaresError(codes.INSTANCE_NOT_FOUND, "Instance not found")
            current_profile = self._load_state(instance_id).get("profile")

        return {
            "hardware": rec["hardware"],
            "memory": rec["memory"],
            "profile": rec["profile"],
            "warnings": rec["warnings"],
            "bottlenecks": rec["bottlenecks"],
            "instance": instance and {
                "id": instance["id"], "name": instance.get("name"),
                "memory": instance.get("memory"), "jvmPreset": instance.get("jvmPreset"),
            },
            "currentProfile": current_profile,
            "profiles": [
                {"id": pid, **{k: profiles.PROFILES[pid][k] for k in ("labelKey", "descKey")}}
                for pid in profiles.PROFILE_ORDER
            ],
        }

    # ------------------------------------------------------------------
    # Plan (preview diff — không ghi gì)
    # ------------------------------------------------------------------

    def plan(self, instance_id: str, profile_id: str) -> dict:
        """Diff thay đổi dự kiến: instance.json + game/options.txt (mục 3.1 preview)."""
        instance = self._require(instance_id)
        prof = profiles.get(profile_id)
        if not prof:
            raise AntaresError(codes.VALIDATION_FAILED, f"Unknown profile: {profile_id}")

        memory = profiles.memory_for_profile(profile_id, self._scan_memory())
        jvm_changes = self._diff_jvm(instance, memory, prof["jvm"])
        mc_changes = self._diff_options(instance_id, prof["minecraft"])

        return {
            "instanceId": instance_id,
            "profileId": profile_id,
            "jvm": jvm_changes,
            "minecraft": mc_changes,
            "hasChanges": bool(jvm_changes or mc_changes),
        }

    def _diff_jvm(self, instance: dict, memory: dict | None, jvm_patch: dict) -> list[dict]:
        changes = []
        target_mem = memory or instance.get("memory")
        if target_mem and instance.get("memory") != target_mem:
            changes.append({
                "field": "memory", "before": instance.get("memory"),
                "after": target_mem,
            })
        for key in ("jvmPreset", "jvmArgs"):
            if jvm_patch.get(key) is not None and instance.get(key) != jvm_patch[key]:
                changes.append({"field": key, "before": instance.get(key), "after": jvm_patch[key]})
        return changes

    def _diff_options(self, instance_id: str, mc_patch: dict) -> list[dict]:
        current = self._read_options(instance_id)
        changes = []
        for key in _OPTIONS_KEYS:
            if key in mc_patch and current.get(key) != mc_patch[key]:
                changes.append({"field": key, "before": current.get(key), "after": mc_patch[key]})
        return changes

    # ------------------------------------------------------------------
    # Apply (snapshot -> ghi) + rollback
    # ------------------------------------------------------------------

    def apply(self, instance_id: str, profile_id: str) -> dict:
        """Snapshot rồi apply profile. Trả {snapshot, changes} (mục 70)."""
        plan = self.plan(instance_id, profile_id)  # validate trước
        snapshot = self._snapshot(instance_id)

        instance = self._require(instance_id)
        patch: dict = {}
        for ch in plan["jvm"]:
            if ch["field"] == "memory":
                patch["memory"] = ch["after"]
            else:
                patch[ch["field"]] = ch["after"]
        if patch:
            self._ctx.get("instances").update(instance_id, patch)

        merged = {**self._read_options(instance_id)}
        for ch in plan["minecraft"]:
            merged[ch["field"]] = ch["after"]
        self._write_options(instance_id, merged)

        self._save_state(instance_id, {
            "profile": profile_id,
            "appliedAt": time.time(),
            "snapshotFile": snapshot["file"],
        })

        self._ctx.events.publish(ev.OPTIMIZATION_APPLIED, {
            "instanceId": instance_id, "profile": profile_id,
        })
        logger.info("Applied profile %s -> %s (%d jvm, %d mc changes)",
                    profile_id, instance_id, len(plan["jvm"]), len(plan["minecraft"]))
        return {"snapshot": snapshot, "plan": plan, "profile": profile_id}

    def rollback(self, instance_id: str) -> dict:
        """Phục hồi từ snapshot gần nhất (mục 70)."""
        state = self._load_state(instance_id)
        snap_file = state.get("snapshotFile")
        if not snap_file:
            raise AntaresError(codes.VALIDATION_FAILED, "No optimization snapshot to restore")
        path = self._snapshots_dir(instance_id) / snap_file
        if not path.is_file():
            raise AntaresError(codes.FILE_NOT_FOUND, "Snapshot file missing")

        from core.config.reader import read_json
        snap = read_json(path)

        # Restore instance.json fields đã backup
        if snap.get("instance"):
            self._ctx.get("instances").update(instance_id, snap["instance"])
        # Restore options.txt nguyên vẹn
        if "optionsRaw" in snap:
            options_path = self._options_path(instance_id)
            options_path.write_text(snap["optionsRaw"], encoding="utf-8")

        self._save_state(instance_id, {"profile": None, "rolledBackAt": time.time()})
        self._ctx.events.publish(ev.OPTIMIZATION_ROLLED_BACK, {"instanceId": instance_id})
        logger.info("Rolled back optimization for %s", instance_id)
        return {"restored": True}

    def snapshot_info(self, instance_id: str) -> dict | None:
        state = self._load_state(instance_id)
        if not state.get("snapshotFile"):
            return None
        from core.config.reader import read_json
        path = self._snapshots_dir(instance_id) / state["snapshotFile"]
        if not path.is_file():
            return None
        snap = read_json(path)
        return {
            "file": state["snapshotFile"],
            "appliedAt": snap.get("appliedAt"),
            "profile": state.get("profile"),
            "instanceFields": sorted((snap.get("instance") or {}).keys()),
        }

    # ------------------------------------------------------------------
    # Helpers: instance / options.txt / snapshot / state
    # ------------------------------------------------------------------

    def _require(self, instance_id: str) -> dict:
        inst = self._ctx.get("instances").get(instance_id)
        if not inst:
            raise AntaresError(codes.INSTANCE_NOT_FOUND, "Instance not found")
        return inst

    def _scan_memory(self) -> dict | None:
        try:
            return advisor.recommend()["memory"]
        except Exception:
            return None

    def _options_path(self, instance_id: str) -> Path:
        return Path(self._ctx.paths.instances) / instance_id / "game" / "options.txt"

    def _read_options(self, instance_id: str) -> dict:
        """Parse options.txt dạng key:value — chưa có key = chưa set (diff so với default patch)."""
        path = self._options_path(instance_id)
        out: dict = {}
        if path.is_file():
            try:
                for line in path.read_text(encoding="utf-8", errors="replace").splitlines():
                    if ":" in line and not line.startswith("#"):
                        k, _, v = line.partition(":")
                        out[k.strip()] = v.strip()
                return out
            except Exception:
                logger.warning("options.txt parse failed for %s", instance_id)
        # Chưa có file (chưa chạy lần nào) -> dùng giá trị "hiện tại" = rỗng;
        # diff sẽ coi mọi key patch là change mới (before=None) — preview rõ ràng.
        return out

    def _coerce(self, v) -> str:
        if isinstance(v, bool):
            return "true" if v else "false"
        return str(v)

    def _write_options(self, instance_id: str, values: dict) -> None:
        """Ghi merged options: giữ nguyên dòng cũ, cập nhật/append key patch."""
        path = self._options_path(instance_id)
        path.parent.mkdir(parents=True, exist_ok=True)
        patch = {k: self._coerce(v) for k, v in values.items()
                 if k in _OPTIONS_KEYS}

        lines: list[str] = []
        if path.is_file():
            try:
                lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
            except Exception:
                lines = []
        seen = set()
        out = []
        for line in lines:
            key = line.partition(":")[0].strip()
            if key in patch:
                if key in seen:
                    continue  # duplicate — chỉ giữ 1 (mục 10.5)
                out.append(f"{key}:{patch[key]}")
                seen.add(key)
            else:
                out.append(line)
        for key, val in patch.items():
            if key not in seen:
                out.append(f"{key}:{val}")
        path.write_text("\n".join(out) + "\n", encoding="utf-8")

    def _snapshots_dir(self, instance_id: str) -> Path:
        d = Path(self._ctx.paths.instances) / instance_id / "optimization-snapshots"
        d.mkdir(parents=True, exist_ok=True)
        return d

    def _snapshot(self, instance_id: str) -> dict:
        from core.config.writer import write_json_atomic
        inst = self._require(instance_id)
        raw = ""
        p = self._options_path(instance_id)
        if p.is_file():
            raw = p.read_text(encoding="utf-8", errors="replace")
        fname = f"opt-{int(time.time())}.json"
        write_json_atomic(self._snapshots_dir(instance_id) / fname, {
            "appliedAt": time.time(),
            "instance": {k: inst.get(k) for k in ("memory", "jvmPreset", "jvmArgs")},
            "optionsRaw": raw,
        })
        return {"file": fname, "appliedAt": time.time()}

    def _state_path(self, instance_id: str) -> Path:
        return Path(self._ctx.paths.instances) / instance_id / "optimization.json"

    def _load_state(self, instance_id: str) -> dict:
        from core.config.reader import read_json
        p = self._state_path(instance_id)
        if not p.is_file():
            return {}
        try:
            return read_json(p) or {}
        except Exception:
            return {}

    def _save_state(self, instance_id: str, patch: dict) -> None:
        from core.config.writer import write_json_atomic
        state = {**self._load_state(instance_id), **patch}
        write_json_atomic(self._state_path(instance_id), state)
