"""NotificationService — dedupe, auto-rules, mark/clear (mục 6, 80)."""
from __future__ import annotations

from services.notifications.service import NotificationService


def test_push_and_dedupe_window(ctx):
    svc = NotificationService(ctx.events)
    svc.error("taskFailed", "same message")
    svc.error("taskFailed", "same message")   # trong cửa sổ dedupe
    assert svc.unread_count() == 1
    svc.error("taskFailed", "different")      # message khác -> mới
    assert svc.unread_count() == 2


def test_auto_rules_from_events(ctx):
    svc = NotificationService(ctx.events)
    bus = ctx.events

    bus.publish("task.updated", {"id": "t1", "state": "running"})   # không sinh
    bus.publish("task.updated", {"id": "t2", "state": "failed"})    # sinh ERROR
    bus.publish("minecraft.started", {"instanceId": "i1"})          # SUCCESS
    bus.publish("minecraft.exited", {"exitCode": 0})                # im lặng
    bus.publish("minecraft.exited", {"exitCode": 1})                # ERROR crashed
    bus.publish("download.progress", {"taskId": "x", "progress": 50})  # không sinh

    items = svc.list()
    assert [i["severity"] for i in items] == ["ERROR", "SUCCESS", "ERROR"]
    assert all(i["read"] is False for i in items)


def test_mark_read_and_clear(ctx):
    svc = NotificationService(ctx.events)
    svc.info("taskFailed")
    svc.error("taskFailed")

    first_id = svc.list()[0]["id"]
    assert svc.mark_read(first_id) == 1
    assert svc.list()[0]["read"] is True
    assert svc.unread_count() == 1

    assert svc.mark_read(None) == 1     # đọc tất cả
    assert svc.unread_count() == 0
    assert svc.clear(first_id) == 1
    assert len(svc.list()) == 1


def test_notification_created_event_fires(ctx):
    seen = []
    ctx.events.subscribe("notification.created",
                         lambda e: seen.append(e.payload["title"]))
    NotificationService(ctx.events).info("taskFailed")
    assert seen == ["taskFailed"]
