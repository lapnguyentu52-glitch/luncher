"""AccountService — offline + ely.by provider (mục 23, 177-178).

Không lưu plaintext password trong settings (mục 23): mật khẩu ely chỉ
dùng trong phiên, token lưu tạm trong account file.
"""
from __future__ import annotations

import uuid

from app.context import AppContext
from core.errors import codes
from core.errors.base import AuthError
from core.logging.setup import get_logger

logger = get_logger("accounts")


class AccountService:
    def __init__(self, ctx: AppContext) -> None:
        self._ctx = ctx

    def list(self) -> list[dict]:
        return list(self._ctx.config.get("accounts", []) or [])

    def get(self, account_id: str) -> dict | None:
        for a in self.list():
            if a.get("id") == account_id:
                return a
        return None

    def get_current(self) -> dict | None:
        sel = self._ctx.config.get("selectedAccount")
        return self.get(sel) if sel else None

    def create_offline(self, display_name: str) -> dict:
        """Offline profile — stable UUID theo username (mục 178)."""
        if not display_name or len(display_name) < 3:
            raise AuthError(codes.VALIDATION_FAILED,
                            "Username must be at least 3 characters.",
                            action=None)  # người dùng tự sửa input
        stable_uuid = str(uuid.uuid5(uuid.NAMESPACE_OID, f"offline:{display_name}"))
        account = {
            "id": stable_uuid,
            "type": "offline",
            "displayName": display_name,
            "minecraftUuid": stable_uuid,
        }
        self._save(account)
        return account

    def login_ely(self, username: str, password: str) -> dict:
        """ely.by auth — provider riêng, không hardcode trong controller (mục 23)."""
        if not username or not password:
            raise AuthError(codes.AUTH_FAILED, "Username and password required.",
                            action="OPEN_ACCOUNTS")
        import requests

        try:
            r = requests.get(
                f"https://authserver.ely.by/api/users/profiles/minecraft/{username}",
                timeout=15)
            if r.status_code == 404:
                raise AuthError(codes.AUTH_FAILED, "User does not exist on ely.by")
            r.raise_for_status()

            resp = requests.post(
                "https://authserver.ely.by/auth/authenticate",
                json={"username": username, "password": password,
                      "clientToken": str(uuid.uuid4()), "requestUser": True},
                timeout=15)
            if resp.status_code != 200:
                raise AuthError(codes.AUTH_FAILED,
                                f"Login failed (HTTP {resp.status_code})")
            data = resp.json()
        except AuthError:
            raise
        except Exception as e:
            raise AuthError(codes.AUTH_FAILED, f"Network error: {e}") from e

        account = {
            "id": data.get("user", {}).get("id") or username,
            "type": "ely",
            "displayName": username,
            "minecraftUuid": data.get("user", {}).get("id"),
            # token lưu trong account store — KHÔNG log, KHÔNG đưa lên UI (mục 24)
            "token": data.get("accessToken", ""),
        }
        self._save(account)
        logger.info("ely.by login OK: %s (token redacted)", username)
        return account

    def remove(self, account_id: str) -> bool:
        accounts = self.list()
        new = [a for a in accounts if a.get("id") != account_id]
        if len(new) == len(accounts):
            return False
        self._ctx.config.set("accounts", new, flush_now=True)
        if self._ctx.config.get("selectedAccount") == account_id:
            self._ctx.config.set("selectedAccount", None, flush_now=True)
        return True

    def select(self, account_id: str) -> bool:
        if not self.get(account_id):
            return False
        self._ctx.config.set("selectedAccount", account_id, flush_now=True)
        return True

    def _save(self, account: dict) -> None:
        accounts = self.list()
        accounts = [a for a in accounts if a.get("id") != account["id"]]
        accounts.append(account)
        self._ctx.config.set("accounts", accounts, flush_now=True)
        self._ctx.config.set("selectedAccount", account["id"], flush_now=True)
