"""Base error model — structured, có code/recoverable/action.

Quy tắc: service core KHÔNG dùng `except Exception: pass`.
"""
from __future__ import annotations

from core.errors import codes


class AntaresError(Exception):
    """Lỗi có cấu trúc. Mọi service raise lớp này hoặc con của nó."""

    def __init__(self, code: str, message: str, *,
                 recoverable: bool = True, action: str | None = None) -> None:
        super().__init__(message)
        self.code = code
        self.message = message
        self.recoverable = recoverable
        self.action = action

    def to_dict(self) -> dict:
        return {
            "code": self.code,
            "message": self.message,
            "recoverable": self.recoverable,
            "action": self.action,
        }


class NetworkError(AntaresError):
    pass


class DownloadError(AntaresError):
    pass


class MinecraftError(AntaresError):
    pass


class AuthError(AntaresError):
    pass


class InstanceError(AntaresError):
    pass


class ServerError(AntaresError):
    pass


class JavaError(AntaresError):
    pass


def internal_error(exc: Exception) -> AntaresError:
    """Wrap unexpected exception — dùng ở rìa service, sau khi logger.exception."""
    return AntaresError(codes.INTERNAL_ERROR, str(exc), recoverable=False)
