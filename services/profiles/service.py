"""ProfileService — Player Profiles: 1 preset, 1 click apply (mục 40).

Profile = {account + instance + jvm + game + launch} đặt tên, lưu JSON.
Apply  = snapshot -> switch account/instance -> patch JVM -> ghi options.txt
         -> remember raw options để Revert từng chữ cái (không phải file snapshot).
"""
from __future__ import annotations

import time
import uuid
from pathlib import Path

from app.context import AppContext
from core.errors import codes
from core.errors.base import AntaresError
from core.events import names as ev
from core.logging.setup import get_logger
from core.utils.paths import is_safe_name
from services.profiles.keys import GAME_KEY_ORDER, GAME_KEYS, coerce

logger = get_logger("profiles")

_STATE_FILE = "profiles-state.json"
_STATE_VERSION = 1
_MAX_NAME_LEN = 64

#: Key launch options hợp lệ trong spec (đối chiếu LaunchOptions — mục 40).
_LAUNCH_KEYS = ("server", "port", "quickPlaySingleplayer", "quickPlayMultiplayer",
                "quickPlayRealms", "customResolution", "resolutionWidth",
                "resolutionHeight")


class ProfileService:
    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx

    # ------------------------------------------------------------------
    # CRUD (theo thuật ngữ UI: list/get/create/duplicate/update/delete)
    # ------------------------------------------------------------------

    def list(self) -> list[dict]:
        """Danh sách + đã apply cờ appliedAt + đang active (so khớp raw)."""
        state = self._load_state()
        out = []
        for p in state.get("profiles", []):
            item = {k: v for k, v in p.items() if k != "gameRaw"}  # payload nhẹ
            pid = p["id"]
            active = False
            applied_at = state.get("applied", {}).get(pid)
            if applied_at:
                try:
                    active = self._current_raw_matches(p)
                except Exception:
                    active = False
            item["active"] = active
            item["lastAppliedAt"] = applied_at
            out.append(item)
        return out

    def get(self, profile_id: str) -> dict | None:
        for p in self._load_state().get("profiles", []):
            if p.get("id") == profile_id:
                return p
        return None

    def create(self, name: str, spec: dict | None = None) -> dict:
        name = str(name or "").strip()
        if not name or not is_safe_name(name) or len(name) > _MAX_NAME_LEN:
            raise AntaresError(codes.VALIDATION_FAILED,
                               f"Invalid profile name: {name!r}")
        spec = self._sanitize_spec(spec or {})
        p = {
            # uuid suffix: tạo 2 profile cùng mili-giây vẫn không trùng ID
            "id": time.strftime("prof-%Y%m%d-%H%M%S") + f"-{uuid.uuid4().hex[:6]}",
            "name": name,
            "createdAt": time.time(),
            "updatedAt": time.time(),
            "spec": spec,
        }
        state = self._load_state()
        state.setdefault("profiles", []).append(p)
        self._save_state(state)
        self._ctx.events.publish(ev.PROFILES_CHANGED, {"action": "create", "profileId": p["id"]})
        logger.info("Profile created: %s (%s)", name, p["id"])
        return p

    def duplicate(self, profile_id: str) -> dict | None:
        src = self.get(profile_id)
        if not src:
            raise AntaresError(codes.PROFILE_NOT_FOUND, f"Profile not found: {profile_id}")
        name = f"{src['name']} copy"
        existing = {p["name"] for p in self._load_state().get("profiles", [])}
        n = 2
        while name in existing:
            name = f"{src['name']} copy {n}"
            n += 1
        return self.create(name, src.get("spec") or {})

    def update(self, profile_id: str, patch: dict) -> dict | None:
        """Chỉ đổi `name` và/hoặc `spec` (sanitize). Không đụng runtime state."""
        p = self.get(profile_id)
        if not p:
            raise AntaresError(codes.PROFILE_NOT_FOUND, f"Profile not found: {profile_id}")
        if not isinstance(patch, dict):
            raise AntaresError(codes.VALIDATION_FAILED, "patch must be a dict")
        state = self._load_state()
        target = next(x for x in state["profiles"] if x["id"] == profile_id)
        if "name" in patch:
            name = str(patch["name"] or "").strip()
            if not name or not is_safe_name(name) or len(name) > _MAX_NAME_LEN:
                raise AntaresError(codes.VALIDATION_FAILED, f"Invalid profile name: {name!r}")
            target["name"] = name
        if "spec" in patch:
            if not isinstance(patch["spec"], dict):
                raise AntaresError(codes.VALIDATION_FAILED, "spec must be a dict")
            target["spec"] = self._sanitize_spec(patch["spec"])
        target["updatedAt"] = time.time()
        self._save_state(state)
        self._ctx.events.publish(ev.PROFILES_CHANGED, {"action": "update", "profileId": profile_id})
        return target

    def delete(self, profile_id: str, *, confirm: bool = False) -> bool:
        """Xoá profile — bắt buộc confirm=True (nhất quán InstanceService)."""
        if not confirm:
            raise AntaresError(codes.VALIDATION_FAILED,
                               "Delete requires explicit confirmation")
        state = self._load_state()
        profiles = state.get("profiles", [])
        new = [p for p in profiles if p.get("id") != profile_id]
        if len(new) == len(profiles):
            return False
        state["profiles"] = new
        state.get("applied", {}).pop(profile_id, None)
        self._save_state(state)
        self._ctx.events.publish(ev.PROFILES_CHANGED, {"action": "delete", "profileId": profile_id})
        return True

    # ------------------------------------------------------------------
    # Capture — tạo profile từ cài đặt hiện tại (1 lần, không track runtime)
    # ------------------------------------------------------------------

    def capture(self, instance_id: str, name: str | None = None) -> dict:
        """Chụp JVM (instance.json) + game (options.txt) + account đang chọn.

        Đây là 'tạo profile từ cài đặt hiện tại' — đọc 1 lần, xong không
        còn ràng buộc runtime nào. apply() sau đó patch vô điều kiện.
        """
        inst = self._ctx.get("instances").get(instance_id)
        if not inst:
            raise AntaresError(codes.INSTANCE_NOT_FOUND, "Instance not found")
        raw = self._read_options_raw(instance_id)
        game = {}
        for line in raw.splitlines():
            line = line.strip()
            if not line or line.startswith("#") or ":" not in line:
                continue
            k, _, v = line.partition(":")
            if k in GAME_KEYS:
                game[k] = coerce(k, v)
        inst_patch = {
            "memory": inst.get("memory"),
            "jvmPreset": inst.get("jvmPreset"),
            "jvmArgs": inst.get("jvmArgs") or [],
        }
        acc = self._ctx.get("accounts").get_current() if self._ctx.get("accounts") else None
        spec = {
            "account": {"id": acc.get("id")} if acc else {},
            "instance": {"id": instance_id},
            "jvm": {k: v for k, v in inst_patch.items() if v is not None},
            "game": game,
            "launch": {},
        }
        return self.create(name or f"{inst.get('name', 'instance')} snapshot", spec)

    # ------------------------------------------------------------------
    # Plan / Apply / Revert
    # ------------------------------------------------------------------

    def plan(self, profile_id: str) -> dict:
        """Diff dự kiến — không ghi gì (What will change?, mục 39)."""
        p = self._require(profile_id)
        spec = p["spec"]
        changes = {"account": None, "instance": None, "jvm": [], "game": [], "launch": []}

        acc_id = (spec.get("account") or {}).get("id")
        if acc_id:
            current = self._ctx.get("accounts").get_current()
            if not current or current.get("id") != acc_id:
                target = self._ctx.get("accounts").get(acc_id)
                changes["account"] = {
                    "before": current and current.get("displayName"),
                    "after": target and target.get("displayName") or acc_id,
                }

        inst_id = (spec.get("instance") or {}).get("id")
        if inst_id:
            current = self._ctx.config.get("selectedInstance")
            if current != inst_id:
                target = self._ctx.get("instances").get(inst_id)
                changes["instance"] = {
                    "before": current,
                    "after": inst_id,
                    "afterName": target and target.get("name") or inst_id,
                }

        inst = self._current_instance()
        jvm_spec = spec.get("jvm") or {}
        if inst is not None:
            for key in ("memory", "jvmPreset", "jvmArgs"):
                if key in jvm_spec and inst.get(key) != jvm_spec[key]:
                    changes["jvm"].append({
                        "field": key, "before": inst.get(key), "after": jvm_spec[key]})

        raw = self._read_options_raw(self._active_instance_id())
        current_game = self._parse_options(raw)
        for key in GAME_KEY_ORDER:
            if key in spec.get("game", {}) and current_game.get(key) != spec["game"][key]:
                changes["game"].append({
                    "field": key, "before": current_game.get(key),
                    "after": spec["game"][key]})

        launch_spec = spec.get("launch") or {}
        if launch_spec:
            changes["launch"] = [
                {"field": k, "before": "(mặc định)", "after": v}
                for k, v in sorted(launch_spec.items()) if v
            ]

        has = bool(changes["account"] or changes["instance"]
                   or changes["jvm"] or changes["game"] or changes["launch"])
        return {"profileId": profile_id, "profileName": p["name"],
                "changes": changes, "hasChanges": has}

    def apply(self, profile_id: str) -> dict:
        """1 click: switch account/instance + JVM + game settings + remember launch.

        KHÔNG launch game — user bấm Play sau đó. Launch options chỉ được
        nhớ lại khi launch instance được chọn (orchestrator đọc state).
        """
        plan = self.plan(profile_id)          # validate trước, không ghi
        p = self._require(profile_id)
        spec = p["spec"]

        # 0. Lưu raw options TRƯỚC khi ghi — revert về đúng từng chữ cái.
        saved_raw = self._remember_raw(self._active_instance_id())

        accounts = self._ctx.get("accounts")
        # 1. Account
        if plan["changes"]["account"] and accounts:
            accounts.select((spec.get("account") or {}).get("id"))
            try:
                self._ctx.events.publish(ev.SELECTION_CHANGED,
                                         {"accountId": (spec.get("account") or {}).get("id"),
                                          "via": "profile"})
            except Exception:
                pass

        # 2. Instance switch
        if plan["changes"]["instance"]:
            self._ctx.config.set("selectedInstance",
                                 (spec.get("instance") or {}).get("id"), flush_now=True)
            try:
                self._ctx.events.publish(ev.SELECTION_CHANGED,
                                         {"instanceId": (spec.get("instance") or {}).get("id"),
                                          "via": "profile"})
            except Exception:
                pass

        # 3. JVM patch (vô điều kiện theo spec — capture có jvm là đủ điều kiện apply)
        inst_id = self._active_instance_id()
        inst = self._ctx.get("instances").get(inst_id) if inst_id else None
        jvm_patch = {k: v for k, v in (spec.get("jvm") or {}).items()
                     if k in ("memory", "jvmPreset", "jvmArgs")}
        if inst is not None and jvm_patch:
            self._ctx.get("instances").update(inst_id, jvm_patch)

        # 4. Game settings (merge giữ dòng lạ — vanilla tự dọn key không hiểu)
        if spec.get("game"):
            self._write_options(inst_id, spec["game"])

        # 5. State: remember raw + launch (ghi lần 2 — ghi đè remember ở bước 0)
        state = self._load_state()
        if saved_raw is not None:
            state["lastRaw"] = saved_raw
        state["applied"] = {profile_id: time.time()}
        launch_spec = {k: v for k, v in (spec.get("launch") or {}).items() if v}
        state["launch"] = ({"instanceId": inst_id, **launch_spec}
                           if launch_spec else None)
        self._save_state(state)

        self._ctx.events.publish(ev.PROFILE_APPLIED, {
            "profileId": profile_id, "name": p["name"], "instanceId": inst_id,
            "hasLaunchOptions": bool(spec.get("launch")),
        })
        logger.info("Profile applied: %s (instance=%s, %d game keys)",
                    profile_id, inst_id, len(spec.get("game") or {}))
        return {"profileId": profile_id, "plan": plan,
                "appliedAt": state["applied"][profile_id]}

    def revert(self) -> bool:
        """Khôi phục raw options.txt của lần apply gần nhất (từng chữ cái)."""
        state = self._load_state()
        raw = state.get("lastRaw")
        if raw is None:
            raise AntaresError(codes.VALIDATION_FAILED, "No profile applied to revert")
        inst_id = self._active_instance_id()
        if not inst_id:
            raise AntaresError(codes.INSTANCE_NOT_FOUND, "No active instance")
        path = self._options_path(inst_id)
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(raw, encoding="utf-8")
        state["lastRaw"] = None
        state["applied"] = {}
        state["launch"] = None
        self._save_state(state)
        self._ctx.events.publish(ev.PROFILE_REVERTED, {"instanceId": inst_id})
        logger.info("Profile reverted (options restored) for %s", inst_id)
        return True

    def launch_hint(self) -> dict | None:
        """Launch options đang remember — orchestrator đọc lúc bấm Play."""
        state = self._load_state()
        launch = state.get("launch")
        return launch or None

    # ------------------------------------------------------------------
    # Export / Import — chia sẻ profile qua file JSON (mục 40)
    # ------------------------------------------------------------------

    def export_data(self, profile_id: str) -> dict:
        p = self._require(profile_id)
        return {
            "format": "antares-profile",
            "version": 1,
            "name": p["name"],
            "spec": p["spec"],
        }

    def import_data(self, data: dict) -> dict:
        if not isinstance(data, dict) or data.get("format") != "antares-profile":
            raise AntaresError(codes.VALIDATION_FAILED,
                               "Not an antares-profile export (missing format tag)")
        spec = data.get("spec")
        if not isinstance(spec, dict):
            raise AntaresError(codes.VALIDATION_FAILED, "Missing spec")
        return self.create(str(data.get("name") or "imported"), spec)

    # ------------------------------------------------------------------
    # Validate spec — chặn key lạ, hostiles (mục 24: chống junk injection)
    # ------------------------------------------------------------------

    def validate_spec(self, spec: dict) -> list[str]:
        issues: list[str] = []
        if not isinstance(spec, dict):
            return ["spec must be a dict"]
        launch = spec.get("launch") or {}
        for k in launch:
            if k not in _LAUNCH_KEYS:
                issues.append(f"launch: unknown key {k}")
        for k in spec.get("game", {}):
            if k not in GAME_KEYS:
                issues.append(f"game: unknown key {k}")
        for k in spec.get("jvm", {}):
            if k not in ("memory", "jvmPreset", "jvmArgs"):
                issues.append(f"jvm: unknown key {k}")
        return issues

    def _sanitize_spec(self, spec: dict) -> dict:
        """Chỉ giữ section/key hợp lệ, coerce giá trị game — chặn junk (mục 24)."""
        if not isinstance(spec, dict):
            raise AntaresError(codes.VALIDATION_FAILED, "spec must be a dict")
        unknown = [k for k in spec if k not in ("account", "instance", "jvm", "game", "launch")]
        if unknown:
            raise AntaresError(codes.VALIDATION_FAILED,
                               f"Unknown spec sections: {unknown}")
        out: dict = {}
        for section in ("account", "instance"):
            val = spec.get(section)
            if val is None:
                continue
            if not isinstance(val, dict) or not isinstance(val.get("id"), str):
                raise AntaresError(codes.VALIDATION_FAILED,
                                   f"{section}: must be {{'id': str}}")
            out[section] = {"id": val["id"]}
        jvm = spec.get("jvm")
        if jvm is not None:
            if not isinstance(jvm, dict):
                raise AntaresError(codes.VALIDATION_FAILED, "jvm must be a dict")
            bad = [k for k in jvm if k not in ("memory", "jvmPreset", "jvmArgs")]
            if bad:
                raise AntaresError(codes.VALIDATION_FAILED,
                                   f"jvm: unknown key {bad}")
            if "jvmArgs" in jvm:
                if not isinstance(jvm["jvmArgs"], list) \
                        or not all(isinstance(a, str) for a in jvm["jvmArgs"]):
                    raise AntaresError(codes.VALIDATION_FAILED,
                                       "jvm.jvmArgs must be list[str]")
            out["jvm"] = jvm
        game = spec.get("game")
        if game is not None:
            if not isinstance(game, dict):
                raise AntaresError(codes.VALIDATION_FAILED, "game must be a dict")
            bad = [k for k in game if k not in GAME_KEYS]
            if bad:
                raise AntaresError(codes.VALIDATION_FAILED,
                                   f"game: unknown key {bad}")
            out["game"] = {k: coerce(k, v) for k, v in game.items()}
        launch = spec.get("launch")
        if launch is not None:
            if not isinstance(launch, dict):
                raise AntaresError(codes.VALIDATION_FAILED, "launch must be a dict")
            bad = [k for k in launch if k not in _LAUNCH_KEYS]
            if bad:
                raise AntaresError(codes.VALIDATION_FAILED,
                                   f"launch: unknown key {bad}")
            out["launch"] = {k: v for k, v in launch.items() if v}
        return out

    # ------------------------------------------------------------------
    # Helpers
    # ------------------------------------------------------------------

    def _require(self, profile_id: str) -> dict:
        p = self.get(profile_id)
        if not p:
            raise AntaresError(codes.PROFILE_NOT_FOUND, f"Profile not found: {profile_id}")
        return p

    def _current_instance(self) -> dict | None:
        inst_id = self._ctx.config.get("selectedInstance")
        if not inst_id:
            return None
        return self._ctx.get("instances").get(inst_id)

    def _active_instance_id(self) -> str | None:
        return self._ctx.config.get("selectedInstance")

    def _options_path(self, instance_id: str) -> Path:
        return Path(self._ctx.paths.instances) / instance_id / "game" / "options.txt"

    def _state_path(self) -> Path:
        return self._ctx.paths.config / _STATE_FILE

    def _load_state(self) -> dict:
        from core.config.reader import read_json
        data = read_json(self._state_path()) or {}
        if data.get("version") != _STATE_VERSION:
            data = {}
        data.setdefault("profiles", [])
        data.setdefault("applied", {})
        data.setdefault("lastRaw", None)
        data.setdefault("launch", None)
        return data

    def _save_state(self, state: dict) -> None:
        from core.config.writer import write_json_atomic
        state["version"] = _STATE_VERSION
        write_json_atomic(self._state_path(), state)

    def _read_options_raw(self, instance_id: str | None) -> str:
        if not instance_id:
            return ""
        path = self._options_path(instance_id)
        if path.is_file():
            try:
                return path.read_text(encoding="utf-8", errors="replace")
            except Exception:
                logger.warning("options.txt read failed for %s", instance_id)
        return ""

    def _parse_options(self, raw: str) -> dict:
        out: dict = {}
        for line in raw.splitlines():
            line = line.strip()
            if not line or line.startswith("#") or ":" not in line:
                continue
            k, _, v = line.partition(":")
            if k in GAME_KEYS:
                out[k] = coerce(k, v)
        return out

    def _write_options(self, instance_id: str, values: dict) -> None:
        """Ghi merged: giữ dòng cũ, cập nhật/append key trong whitelist, dedupe."""
        if not instance_id:
            raise AntaresError(codes.INSTANCE_NOT_FOUND, "No active instance")
        path = self._options_path(instance_id)
        path.parent.mkdir(parents=True, exist_ok=True)
        patch = {k: coerce(k, v) for k, v in values.items() if k in GAME_KEYS}
        lines: list[str] = []
        if path.is_file():
            try:
                lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
            except Exception:
                lines = []
        seen: set[str] = set()
        out: list[str] = []
        for line in lines:
            key = line.partition(":")[0].strip()
            if key in patch:
                if key in seen:
                    continue
                out.append(f"{key}:{patch[key]}")
                seen.add(key)
            else:
                out.append(line)
        for key, val in patch.items():
            if key not in seen:
                out.append(f"{key}:{val}")
        path.write_text("\n".join(out) + "\n", encoding="utf-8")

    def _remember_raw(self, instance_id: str | None) -> str | None:
        raw = self._read_options_raw(instance_id)
        return raw if instance_id else None

    def _current_raw_matches(self, p: dict) -> bool:
        """Profile được coi active nếu raw options khớp 100% giá trị trong spec.game."""
        spec_game = (p.get("spec") or {}).get("game") or {}
        if not spec_game:
            return False
        raw = self._read_options_raw(self._active_instance_id())
        current = self._parse_options(raw)
        return all(current.get(k) == v for k, v in spec_game.items())
