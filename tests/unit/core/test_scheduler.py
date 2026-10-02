"""GameModeGovernor — game mode theo event + priority hook (mục 30, 36)."""
from __future__ import annotations

from core.events.bus import EventBus
from core.scheduler.policy import GameModeGovernor


def test_game_mode_toggles_on_events(ctx):
    bus = ctx.events
    gov = GameModeGovernor(bus)
    assert gov.active is False

    bus.publish("minecraft.started", {"instanceId": "i1"})
    assert gov.active is True
    assert gov.instance_id == "i1"

    bus.publish("minecraft.exited", {"exitCode": 0})
    assert gov.active is False
    assert gov.instance_id is None


def test_priority_hook_called_with_instance(ctx):
    calls = []
    gov = GameModeGovernor(ctx.events, process_priority_hook=calls.append)
    ctx.events.publish("minecraft.started", {"instanceId": "abc"})
    assert calls == ["abc"]


def test_priority_hook_error_does_not_break_bus(ctx):
    def boom(_):
        raise RuntimeError("hook failed")
    gov = GameModeGovernor(ctx.events, process_priority_hook=boom)
    # Không raise — governor nuốt lỗi hook
    ctx.events.publish("minecraft.started", {"instanceId": "x"})
    assert gov.active is True
