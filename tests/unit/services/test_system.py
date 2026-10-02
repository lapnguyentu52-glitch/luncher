"""SystemOptimization — cleaner (mục 37), power shape, process guard (mục 36)."""
from __future__ import annotations

import os
import time


def _svc(ctx):
    from services.system import SystemOptimizationService
    svc = SystemOptimizationService(ctx)
    ctx.set("system_opt", svc)
    return svc


def test_cleaner_scan_respects_age(ctx):
    svc = _svc(ctx)
    old = ctx.paths.logs / "old.log"
    old.write_text("x" * 100)
    new = ctx.paths.logs / "new.log"
    new.write_text("y")
    os.utime(old, (time.time() - 20 * 86400,) * 2)

    scan = svc.cleanup_scan()
    groups = {g["id"]: g for g in scan["safe"]}
    assert groups["old_logs"]["count"] == 1
    assert scan["totalCleanableBytes"] == 100


def test_clean_move_to_trash_and_undo(ctx):
    svc = _svc(ctx)
    f = ctx.paths.logs / "doomed.log"
    f.write_text("data")

    r = svc.cleanup_clean([str(f)])
    assert r["moved"] == 1 and r["bytes"] == 4
    assert not f.exists()

    u = svc.cleanup_undo(r["cleanId"])
    assert u["restored"] == 1 and f.exists() and f.read_text() == "data"


def test_clean_refuses_outside_data(ctx):
    svc = _svc(ctx)
    try:
        svc.cleanup_clean([str(ctx.paths.root / "outside.txt")])
        assert False, "phải chặn path ngoài data"
    except Exception as e:
        assert "outside" in str(e) or "Refusing" in str(e)


def test_power_status_shape(ctx):
    status = _svc(ctx).power_status()
    assert isinstance(status["supported"], bool)
    assert "recommendation" in status


def test_process_priority_only_launcher_owned(ctx):
    import subprocess
    import sys
    svc = _svc(ctx)
    proc = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(2)"])
    try:
        assert svc.process.apply_priority(str(proc.pid), "high") is False
        assert svc.process.apply_priority(str(proc.pid), "realtime") is False  # không trong ALLOWED
    finally:
        proc.kill()
