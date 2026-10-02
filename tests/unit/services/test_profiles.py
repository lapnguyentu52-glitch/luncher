"""Player Profiles — CRUD/capture/apply/revert/export-import + launch overrides (mục 40)."""
from __future__ import annotations

import json

import pytest

from core.errors import codes
from core.errors.base import AntaresError
from core.events import names as ev
from services.profiles import ProfileService
from services.profiles.keys import GAME_KEYS, coerce


@pytest.fixture()
def svc(ctx):
    ctx.set("profiles", ProfileService(ctx))
    return ctx.get("profiles")


@pytest.fixture()
def account_id(ctx):
    acc = ctx.get("accounts").create_offline("PlayerOne") if ctx.get("accounts") else None
    if acc is None:
        from services.accounts.service import AccountService
        ctx.set("accounts", AccountService(ctx))
        acc = ctx.get("accounts").create_offline("PlayerOne")
    return acc["id"]


SPEC_FULL = {
    "account": {"id": "ACC"},
    "instance": {"id": "INST"},
    "jvm": {"memory": {"minMb": 1024, "maxMb": 4096}, "jvmPreset": "balanced",
            "jvmArgs": ["-XX:+UseG1GC"]},
    "game": {"gamma": 1.0, "fov": 0.0, "guiScale": 0, "maxFps": 240,
             "renderDistance": 8, "particles": 0, "vsync": False,
             "soundCategory_master": 0.8},
    "launch": {"server": "mc.hypixel.net", "port": 25565},
}


# ------------------------------------------------------------------
# CRUD + validation
# ------------------------------------------------------------------

def test_create_list_get_roundtrip(svc):
    p = svc.create("PvP Setup", SPEC_FULL)
    assert p["name"] == "PvP Setup"
    assert p["spec"]["launch"]["server"] == "mc.hypixel.net"
    items = svc.list()
    assert len(items) == 1 and items[0]["id"] == p["id"]
    assert svc.get(p["id"])["name"] == "PvP Setup"
    assert svc.get("ghost") is None


def test_create_rejects_bad_names(svc):
    for bad in ("", "  ", "a/b", "..", "con:", "x" * 65):
        with pytest.raises(AntaresError) as ei:
            svc.create(bad)
        assert ei.value.code == codes.VALIDATION_FAILED


def test_unknown_game_key_rejected_at_create(svc):
    with pytest.raises(AntaresError) as ei:
        svc.create("Bad", {"game": {"notARealKey": 1}})
    assert ei.value.code == codes.VALIDATION_FAILED
    assert "notARealKey" in ei.value.message


def test_unknown_launch_key_rejected(svc):
    with pytest.raises(AntaresError):
        svc.create("Bad", {"launch": {"shell": "rm -rf /"}})


def test_unknown_jvm_key_rejected(svc):
    with pytest.raises(AntaresError):
        svc.create("Bad", {"jvm": {"-Xmx": "9999M"}})


def test_duplicate_suffixes(svc):
    p = svc.create("Combo")
    c1 = svc.duplicate(p["id"])
    c2 = svc.duplicate(p["id"])
    assert c1["name"] == "Combo copy"
    assert c2["name"] == "Combo copy 2"
    assert c1["id"] != c2["id"]


def test_duplicate_missing_raises(svc):
    with pytest.raises(AntaresError) as ei:
        svc.duplicate("ghost")
    assert ei.value.code == codes.PROFILE_NOT_FOUND


def test_update_name_and_spec(svc):
    p = svc.create("Old", {"game": {"gamma": 0.5}})
    out = svc.update(p["id"], {"name": "New", "spec": {"game": {"gamma": 1.0}}})
    assert out["name"] == "New"
    assert out["spec"]["game"]["gamma"] == "1.0"  # spec lưu string đã coerce
    assert out["updatedAt"] >= out["createdAt"]


def test_update_rejects_bad_spec(svc):
    p = svc.create("Keep")
    with pytest.raises(AntaresError):
        svc.update(p["id"], {"spec": {"game": {"hax": 1}}})


def test_delete_requires_confirm(svc):
    p = svc.create("Bye")
    with pytest.raises(AntaresError):
        svc.delete(p["id"])
    assert svc.delete(p["id"], confirm=True) is True
    assert svc.get(p["id"]) is None
    assert svc.delete(p["id"], confirm=True) is False


def test_state_persists_across_instances(svc, ctx):
    p = svc.create("Persist", SPEC_FULL)
    svc2 = ProfileService(ctx)
    assert svc2.get(p["id"])["name"] == "Persist"


# ------------------------------------------------------------------
# Capture
# ------------------------------------------------------------------

def test_capture_jvm_game_account(ctx, svc, instance_id, account_id):
    inst = ctx.get("instances")
    inst.update(instance_id, {"memory": {"minMb": 1024, "maxMb": 4096},
                              "jvmPreset": "balanced", "jvmArgs": ["-XX:+UseG1GC"]})
    game_dir = ctx.paths.instances / instance_id / "game"
    game_dir.mkdir(parents=True, exist_ok=True)
    (game_dir / "options.txt").write_text(
        "gamma:1.0\nversion:3805\nfoo:bar\n", encoding="utf-8")

    p = svc.capture(instance_id, "My Setup")
    spec = p["spec"]
    assert spec["jvm"]["memory"] == {"minMb": 1024, "maxMb": 4096}
    assert spec["jvm"]["jvmPreset"] == "balanced"
    assert spec["game"] == {"gamma": "1.0"}
    assert spec["account"] == {"id": account_id}
    assert spec["instance"] == {"id": instance_id}


def test_capture_unknown_instance(ctx, svc):
    with pytest.raises(AntaresError) as ei:
        svc.capture("ghost")
    assert ei.value.code == codes.INSTANCE_NOT_FOUND


# ------------------------------------------------------------------
# Plan / Apply / Revert
# ------------------------------------------------------------------

def test_plan_reports_diffs_without_writing(ctx, svc, instance_id, account_id):
    ctx.config.set("selectedInstance", instance_id, flush_now=True)
    p = svc.create("Full", {**SPEC_FULL,
                            "account": {"id": account_id},
                            "instance": {"id": instance_id}})
    plan = svc.plan(p["id"])
    assert plan["hasChanges"] is True
    jvm_fields = {c["field"] for c in plan["changes"]["jvm"]}
    game_fields = {c["field"] for c in plan["changes"]["game"]}
    assert "memory" in jvm_fields and "jvmPreset" in jvm_fields
    assert "gamma" in game_fields
    # không ghi gì
    assert not (ctx.paths.instances / instance_id / "game" / "options.txt").exists()


def test_plan_empty_when_everything_matches(ctx, svc, instance_id, account_id):
    ctx.config.set("selectedInstance", instance_id, flush_now=True)
    ctx.config.set("selectedAccount", account_id, flush_now=True)
    inst = ctx.get("instances")
    inst.update(instance_id, {"memory": {"minMb": 1024, "maxMb": 4096},
                              "jvmPreset": "balanced", "jvmArgs": ["-XX:+UseG1GC"]})
    p = svc.create("Match", {"jvm": {"memory": {"minMb": 1024, "maxMb": 4096},
                                     "jvmPreset": "balanced",
                                     "jvmArgs": ["-XX:+UseG1GC"]},
                             "game": {"gamma": 0.5}})
    game_dir = ctx.paths.instances / instance_id / "game"
    game_dir.mkdir(parents=True, exist_ok=True)
    (game_dir / "options.txt").write_text("gamma:0.5\n", encoding="utf-8")
    plan = svc.plan(p["id"])
    assert plan["hasChanges"] is False


def test_apply_switches_account_instance_patches_jvm_and_options(
        ctx, svc, instance_id, account_id, tmp_path):
    other = ctx.get("instances").create("Other", "1.21")
    ctx.config.set("selectedInstance", other["id"], flush_now=True)
    p = svc.create("Full", {**SPEC_FULL,
                            "account": {"id": account_id},
                            "instance": {"id": instance_id}})
    r = svc.apply(p["id"])
    assert ctx.config.get("selectedInstance") == instance_id
    assert ctx.config.get("selectedAccount") == account_id
    inst = ctx.get("instances").get(instance_id)
    assert inst["memory"] == {"minMb": 1024, "maxMb": 4096}
    assert inst["jvmPreset"] == "balanced"
    assert inst["jvmArgs"] == ["-XX:+UseG1GC"]
    txt = (ctx.paths.instances / instance_id / "game" / "options.txt").read_text()
    assert "gamma:1.0" in txt and "vsync:false" in txt
    assert r["appliedAt"] > 0


def test_apply_merges_preserving_unknown_lines(ctx, svc, instance_id, account_id):
    ctx.config.set("selectedInstance", instance_id, flush_now=True)
    game_dir = ctx.paths.instances / instance_id / "game"
    game_dir.mkdir(parents=True, exist_ok=True)
    (game_dir / "options.txt").write_text(
        "version:3805\nretryOnOld:attempt:3\n", encoding="utf-8")
    p = svc.create("G", {"game": {"gamma": 1.0}})
    svc.apply(p["id"])
    txt = (game_dir / "options.txt").read_text()
    assert "version:3805" in txt and "retryOnOld:attempt:3" in txt and "gamma:1.0" in txt


def test_apply_then_revert_restores_raw(ctx, svc, instance_id, account_id):
    ctx.config.set("selectedInstance", instance_id, flush_now=True)
    game_dir = ctx.paths.instances / instance_id / "game"
    game_dir.mkdir(parents=True, exist_ok=True)
    original = "version:3805\n_gamma_weird:keep\n"
    (game_dir / "options.txt").write_text(original, encoding="utf-8")
    p = svc.create("G", {"game": {"gamma": 1.0}})
    svc.apply(p["id"])
    assert "gamma:1.0" in (game_dir / "options.txt").read_text()
    assert svc.revert() is True
    assert (game_dir / "options.txt").read_text() == original
    with pytest.raises(AntaresError):
        svc.revert()  # không còn apply nào để revert


def test_apply_remember_launch_hint_and_revert_clears(ctx, svc, instance_id, account_id):
    ctx.config.set("selectedInstance", instance_id, flush_now=True)
    p = svc.create("Jump", {"launch": {"server": "play.example.net", "port": 25565}})
    svc.apply(p["id"])
    hint = svc.launch_hint()
    assert hint["server"] == "play.example.net"
    assert hint["instanceId"] == instance_id
    svc.revert()
    assert svc.launch_hint() is None


def test_apply_publishes_profile_applied(ctx, svc, instance_id, account_id):
    ctx.config.set("selectedInstance", instance_id, flush_now=True)
    seen = []
    ctx.events.subscribe(ev.PROFILE_APPLIED, lambda e: seen.append(e.payload))
    p = svc.create("G", {"game": {"gamma": 1.0}})
    svc.apply(p["id"])
    assert seen and seen[0]["profileId"] == p["id"]


def test_crud_publishes_profiles_changed(ctx, svc):
    seen = []
    ctx.events.subscribe(ev.PROFILES_CHANGED, lambda e: seen.append(e.payload["action"]))
    p = svc.create("A")
    svc.update(p["id"], {"name": "B"})
    svc.delete(p["id"], confirm=True)
    assert seen == ["create", "update", "delete"]


def test_apply_active_flag_roundtrip(ctx, svc, instance_id, account_id):
    ctx.config.set("selectedInstance", instance_id, flush_now=True)
    p = svc.create("G", {"game": {"gamma": 1.0}})
    assert svc.list()[0]["active"] is False
    svc.apply(p["id"])
    assert svc.list()[0]["active"] is True


def test_plan_missing_profile(svc):
    with pytest.raises(AntaresError) as ei:
        svc.plan("ghost")
    assert ei.value.code == codes.PROFILE_NOT_FOUND


def test_apply_without_selected_instance_still_switches_account(ctx, svc, account_id):
    ctx.config.set("selectedInstance", None, flush_now=True)
    p = svc.create("Acc", {"account": {"id": account_id}})
    svc.apply(p["id"])
    assert ctx.config.get("selectedAccount") == account_id


def test_apply_game_without_instance_raises(ctx, svc):
    ctx.config.set("selectedInstance", None, flush_now=True)
    p = svc.create("G", {"game": {"gamma": 1.0}})
    with pytest.raises(AntaresError):
        svc.apply(p["id"])


# ------------------------------------------------------------------
# Export / Import
# ------------------------------------------------------------------

def test_export_import_roundtrip(ctx, svc):
    p = svc.create("Share", SPEC_FULL)
    data = svc.export_data(p["id"])
    assert data["format"] == "antares-profile"
    blob = json.dumps(data)
    p2 = svc.import_data(json.loads(blob))
    assert p2["name"] == "Share"
    assert p2["spec"] == p["spec"]


def test_import_rejects_bad_payload(svc):
    with pytest.raises(AntaresError):
        svc.import_data({"hello": "world"})
    with pytest.raises(AntaresError):
        svc.import_data({"format": "antares-profile"})


def test_import_rejects_bad_spec_keys(svc):
    with pytest.raises(AntaresError):
        svc.import_data({"format": "antares-profile", "name": "X",
                         "spec": {"game": {"gamma;evil": 1}}})


# ------------------------------------------------------------------
# keys.py — whitelist + coerce
# ------------------------------------------------------------------

def test_whitelist_has_28_keys():
    assert len(GAME_KEYS) == 28


def test_coerce_types():
    assert coerce("guiScale", "3") == "3"
    assert coerce("guiScale", 2.6) == "3"
    assert coerce("vsync", True) == "true"
    assert coerce("vsync", "false") == "false"
    assert coerce("gamma", 1) == "1.0"
    assert coerce("sensitivity", "0.5") == "0.5"


def test_validate_spec_reports_issues(svc):
    issues = svc.validate_spec({"game": {"nope": 1}, "launch": {"bad": 2},
                                "jvm": {"weird": 3}})
    assert len(issues) == 3
    assert svc.validate_spec({"game": {"gamma": 1}}) == []
