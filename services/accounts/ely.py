"""ely.by provider — tách riêng provider layer (mục 23), refactored từ Spark.

Học từ authlib-injector source (authlib/): ely.by là Yggdrasil API compatible;
javaagent thay endpoint Mojang -> ely.by qua ConstantURLTransformUnit.
Skin whitelist cũng cần mở cho ely.by domain.
"""
from __future__ import annotations

import uuid

import requests

from core.errors import codes
from core.errors.base import AuthError
from core.logging.setup import get_logger

logger = get_logger("auth.ely")

AUTHSERVER = "https://authserver.ely.by"
# skin domain cần whitelist trong authlib-injector (học SkinWhitelistTransformUnit)
SKIN_DOMAINS = ["ely.by", "skins.ely.by"]

JAVAAGENT_JAR = "authlib/authlib-injector-1.1.39.jar"


class ElyAuthProvider:
    """ely.by auth — Yggdrasil compatible endpoints."""

    def login(self, username: str, password: str) -> dict:
        if not username or not password:
            raise AuthError(codes.AUTH_FAILED, "Username and password required.")
        client_token = str(uuid.uuid4())

        try:
            # verify user tồn tại (port từ Spark ely_authenticate)
            r = requests.get(
                f"{AUTHSERVER}/api/users/profiles/minecraft/{username}", timeout=15)
            if r.status_code == 404:
                raise AuthError(codes.AUTH_FAILED, "User does not exist on ely.by")
            r.raise_for_status()

            resp = requests.post(f"{AUTHSERVER}/auth/authenticate", json={
                "username": username, "password": password,
                "clientToken": client_token, "requestUser": True,
            }, timeout=15)
            if resp.status_code != 200:
                raise AuthError(codes.AUTH_FAILED,
                                f"Login failed (HTTP {resp.status_code})")
            data = resp.json()
        except AuthError:
            raise
        except requests.RequestException as e:
            raise AuthError(codes.NETWORK_OFFLINE, f"Network error: {e}") from e

        uid = data.get("user", {}).get("id")
        logger.info("ely.by login OK: %s", username)
        return {
            "id": uid or username,
            "type": "ely",
            "displayName": username,
            "minecraftUuid": uid,
            "token": data.get("accessToken", ""),
            "clientToken": client_token,
        }

    def javaagent_args(self) -> list[str]:
        """JVM args để inject authlib-injector cho ely.by (port từ Spark run_mc)."""
        import sys
        from pathlib import Path
        root = Path(sys.argv[0]).resolve().parent.parent if getattr(sys, "frozen", False) is False else Path(sys.executable).parent
        jar = root / JAVAAGENT_JAR
        if not jar.exists():
            # fallback: cwd
            jar = Path.cwd() / JAVAAGENT_JAR
        return [f"-javaagent:{jar}=ely.by"]


def skin_whitelist_domains() -> list[str]:
    """Domains cho skin server whitelist — dùng khi cấu hình authlib."""
    return list(SKIN_DOMAINS)
