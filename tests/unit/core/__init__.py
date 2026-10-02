"""EventBus — publish/subscribe, wildcard, unsubscriber, lỗi handler không giết publisher."""
from __future__ import annotations

from core.events.bus import EventBus


def test_publish_subscribe_exact():
    bus = EventBus()
    seen = []
    bus.subscribe("a.b", lambda e: seen.append(e.payload))
    bus.publish("a.b", {"x": 1})
    assert seen == [{"x": 1}]


def test_wildcard_receives_all():
    bus = EventBus()
    seen = []
    bus.subscribe("*", lambda e: seen.append(e.name))
    bus.publish("a", {})
    bus.publish("b", {})
    assert seen == ["a", "b"]


def test_unsubscribe():
    bus = EventBus()
    seen = []
    off = bus.subscribe("evt", lambda e: seen.append(1))
    bus.publish("evt", {})
    off()
    bus.publish("evt", {})
    assert seen == [1]


def test_handler_error_does_not_break_publish():
    bus = EventBus()
    seen = []
    bus.subscribe("evt", lambda e: 1 / 0)  # hỏng
    bus.subscribe("evt", lambda e: seen.append(e.payload["ok"]))
    env = bus.publish("evt", {"ok": True})
    assert seen == [True]
    assert env.name == "evt"


def test_envelope_has_request_id_and_timestamp():
    bus = EventBus()
    env = bus.publish("evt", {})
    assert env.request_id and env.timestamp > 0
