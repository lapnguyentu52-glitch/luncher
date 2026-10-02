"""GameOptimizationService — plan/apply/rollback (mục 3, 70)."""
from __future__ import annotations

from services.optimization import GameOptimizationService
from services.optimization.profiles import PROFILE_ORDER


def _svc(ctx):
    opt = GameOptimizationService(ctx)
    ctx.set("optimization", opt)
    return opt


def test_scan_returns_hardware_and_profiles(ctx, instance_id):
    scan = _svc(ctx).scan(instance_id)
    assert scan["hardware"]["ramTotalMb"] > 0
    assert [p["id"] for p in scan["profiles"]] == list(PROFILE_ORDER)
    assert scan["instance"]["id"] == instance_id


def test_plan_is_dry_run(ctx, instance_id):
    opt = _svc(ctx)
    options = ctx.paths.instances / instance_id / "game" / "options.txt"
    options.write_text("renderDistance:12\n", encoding="utf-8")

    plan = opt.plan(instance_id, "performance")
    assert plan["hasChanges"]
    fields = {c["field"]: c["after"] for c in plan["minecraft"]}
    assert fields["renderDistance"] == 8
    # Dry-run: file không đổi
    assert options.read_text().startswith("renderDistance:12")


def test_apply_then_rollback_roundtrip(ctx, instance_id):
    opt = _svc(ctx)
    options = ctx.paths.instances / instance_id / "game" / "options.txt"
    options.write_text("renderDistance:12\nfov:0.5\n", encoding="utf-8")

    r = opt.apply(instance_id, "performance")
    assert r["snapshot"]["file"]
    txt = options.read_text()
    assert "renderDistance:8" in txt and "fov:0.5" in txt   # merge giữ key lạ

    r2 = opt.apply(instance_id, "low_end")
    assert "renderDistance:6" in options.read_text()

    opt.rollback(instance_id)
    txt = options.read_text()
    assert "renderDistance:8" in txt and "fov:0.5" in txt   # về state trước low_end


def test_validation(ctx, instance_id):
    opt = _svc(ctx)
    for bad_call in (
        lambda: opt.plan("ghost", "performance"),
        lambda: opt.plan(instance_id, "ghost-profile"),
    ):
        try:
            bad_call()
            assert False, "phải raise"
        except Exception:
            pass
