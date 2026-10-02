"""Realtime stress test — spec 3.0 mục 48.

Scenario mục 48:
    500 events/sec backend
    1000 logs/sec
    20 notifications/sec
UI phải: coalesce, drop stale telemetry, preserve critical errors,
         không chặn publisher thread (EventBus sync nhưng handler
         EventBridge chỉ buffer + lock — O(1) mỗi event).

Policy mục 48 map thực tế:
    CRITICAL events  -> không đi qua coalescing (NOTIFICATION_CREATED là
                        event thường -> outbox, không bao giờ bị drop trừ
                        khi outbox tràn — test burst đảm bảo không tràn)
    STATE events     -> keep latest (LATEST_EVENTS)
    PROGRESS/METRICS -> coalesce latest theo khoá task/instance
    LOG              -> batch (ring buffer + LOG_LINES)
"""
from __future__ import annotations

import time

from core.events import names as ev
from core.events.bus import EventBus
from api.events.bridge import EventBridge


def _make_bridge(bus: EventBus) -> EventBridge:
    return EventBridge(bus, log_reader=lambda c: ([], c), log_cursor=0)


def test_500_eps_coalesce_latest_and_state_events_preserved():
    """500 event/giây trong 2 giây: telemetry coalesce còn lại vài mẫu,
    event đổi state (created/completed) phải nguyên vẹn 100%."""
    bus = EventBus()
    bridge = _make_bridge(bus)
    try:
        n_tel = 1000                     # 500/s * 2s
        for i in range(n_tel):
            bus.publish(ev.PERFORMANCE_TELEMETRY, {
                "cpu": 40 + (i % 30), "ram": 50 + (i % 20),
            })
            if i % 50 == 0:              # 20 state events/giây xen kẽ
                bus.publish(ev.INSTANCE_CREATED, {"id": f"inst-{i}"})
            if i % 100 == 0:             # notification "critical" xen kẽ
                bus.publish(ev.NOTIFICATION_CREATED, {
                    "id": f"n-{i}", "severity": "ERROR",
                    "title": "Launch failed", "message": "inst",
                })

        # Không publish xong là mất — bridge buffer đồng bộ trong publish()
        batch = bridge.flush()

        state = [e for e in batch if e["event"] == ev.INSTANCE_CREATED]
        notif = [e for e in batch if e["event"] == ev.NOTIFICATION_CREATED]
        tel = [e for e in batch if e["event"] == ev.PERFORMANCE_TELEMETRY]

        # State + notification: không được mất cái nào (mục 48: preserve errors)
        assert len(state) == 20
        assert len(notif) == 10
        # Telemetry: coalesce về 1 mẫu duy nhất (keep latest)
        assert len(tel) == 1
        assert tel[0]["payload"]["cpu"] == 40 + ((n_tel - 1) % 30)  # mẫu MỚI nhất
        # Không drop gì cả — outbox không tràn (cap 500, but coalescing giữ nhỏ)
        assert bridge.stats["dropped"] == 0
    finally:
        bridge.close()


def test_1000_log_lines_per_second_batched():
    """1000 dòng log/giây qua APPEND event — gom batch, không vượt cap."""
    bus = EventBus()
    bridge = _make_bridge(bus)
    try:
        for i in range(2000):            # 1000/s * 2s
            bus.publish(ev.MINECRAFT_OUTPUT, {"line": f"[INFO] line {i}"})

        batch = bridge.flush()
        out = [e for e in batch if e["event"] == ev.MINECRAFT_OUTPUT]
        assert len(out) == 1, "append events phải gộp thành 1 batch"
        payload = out[0]["payload"]
        # cap MAX_LINES_PER_BATCH=400 — count = số dòng giao trong batch
        assert payload["count"] == len(payload["lines"]) == 400
        # giữ dòng MỚI nhất (drop stale — mục 48)
        assert payload["lines"][-1] == "[INFO] line 1999"
        assert payload["lines"][0] == "[INFO] line 1600"
    finally:
        bridge.close()


def test_latest_coalesce_keyed_by_task_no_cross_drop():
    """Nhiều task download song song: coalesce theo khoá — không đè chéo."""
    bus = EventBus()
    bridge = _make_bridge(bus)
    try:
        for task in range(4):            # 4 concurrent downloads (mục 48)
            for pct in range(0, 101, 5):  # 21 progress mỗi task
                bus.publish(ev.DOWNLOAD_PROGRESS, {"pct": pct},
                            task_id=f"task-{task}")

        batch = bridge.flush()
        prog = [e for e in batch if e["event"] == ev.DOWNLOAD_PROGRESS]
        assert len(prog) == 4, "mỗi task giữ đúng 1 mẫu mới nhất"
        # mỗi envelope giữ task_id riêng — không đè chéo giữa 4 task
        task_ids = {e.get("taskId") for e in prog}
        assert task_ids == {f"task-{t}" for t in range(4)}
    finally:
        bridge.close()


def test_publisher_thread_never_blocked_long():
    """Publish 500 evt/s: handler bridge phải O(1) — không block publisher."""
    bus = EventBus()
    bridge = _make_bridge(bus)
    try:
        worst = 0.0
        for i in range(1000):
            t0 = time.perf_counter()
            bus.publish(ev.PERFORMANCE_TELEMETRY, {"cpu": i % 100})
            worst = max(worst, time.perf_counter() - t0)
        # handler chỉ buffer + lock — chắc chắn dưới 5ms mỗi publish
        assert worst < 0.005, f"publish chặn quá lâu: {worst * 1000:.1f}ms"
    finally:
        bridge.close()


def test_notification_burst_20_per_second_no_loss():
    """20 notifications/giây trong 5s (mục 48): NotificationService dedupe +
    history cap; event NOTIFICATION_CREATED không bị drop."""
    from services.notifications.service import NotificationService

    bus = EventBus()
    bridge = _make_bridge(bus)
    svc = NotificationService(bus)
    try:
        created = 0
        for i in range(100):             # 20/s * 5s
            svc.push(severity="WARNING", category="download",
                     title=f"Retry {i % 10}", message=f"attempt {i}")
            created += 1

        batch = bridge.flush()
        evs = [e for e in batch if e["event"] == ev.NOTIFICATION_CREATED]
        # dedupe theo (severity, category, title, message): 10 title × 10 msg
        assert len(evs) == created, "notification đã tạo phải tới UI đầy đủ"
        hist = svc.list(limit=200)
        assert len(hist) <= 100          # cap MAX_HISTORY
    finally:
        bridge.close()
        svc.close()
