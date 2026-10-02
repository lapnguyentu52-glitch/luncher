"""Shutdown helpers tách khỏi runtime (spec 126)."""
from __future__ import annotations

from app.context import AppContext
from core.logging.setup import get_logger

logger = get_logger("shutdown")


def execute(ctx: AppContext) -> None:
    """Flush stores, flush logs, dọn task registry (spec 42)."""
    bridge = ctx.get("event_bridge")
    if bridge is not None:                      # dừng thread đẩy event trước tiên
        try:
            bridge.close()
        except Exception:
            logger.exception("Failed to close event bridge")
    try:
        ctx.config.flush()
    except Exception:
        logger.exception("Failed to flush config")
    ctx.tasks.cleanup(keep_recent=10)
    ctx.events.clear()
    logger.info("Shutdown sequence executed")
