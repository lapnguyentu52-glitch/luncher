"""Microsoft auth provider — port flow chuẩn từ minecraft-launcher-lib.

Flow (học từ minecraft_loader/microsoft_account.py):
  PKCE login url -> auth code -> token -> XBL -> XSTS -> MC services -> profile

Học được: secure login dùng PKCE (code_verifier S256) + state chống CSRF.
"""
from __future__ import annotations

import base64
import secrets
import urllib.parse
from hashlib import sha256

from core.errors import codes
from core.errors.base import AuthError
from core.logging.setup import get_logger

logger = get_logger("auth.microsoft")

AUTH_URL = "https://login.microsoftonline.com/consumers/oauth2/v2.0/authorize"
TOKEN_URL = "https://login.microsoftonline.com/consumers/oauth2/v2.0/token"
SCOPE = "XboxLive.signin offline_access"

XBL_URL = "https://user.auth.xboxlive.com/user/authenticate"
XSTS_URL = "https://xsts.auth.xboxlive.com/xsts/authorize"
MC_AUTH_URL = "https://api.minecraftservices.com/authentication/login_with_xbox"
MC_STORE_URL = "https://api.minecraftservices.com/entitlements/mcstore"
MC_PROFILE_URL = "https://api.minecraftservices.com/minecraft/profile"


def generate_pkce() -> tuple[str, str]:
    """(code_verifier, code_challenge) — S256 method (port _generate_pkce_data)."""
    code_verifier = secrets.token_urlsafe(128)[:128]
    digest = sha256(code_verifier.encode("ascii")).digest()
    code_challenge = base64.urlsafe_b64encode(digest).decode("ascii")[:-1]
    return code_verifier, code_challenge


def generate_state() -> str:
    return secrets.token_urlsafe(16)


def build_login_url(client_id: str, redirect_uri: str, *,
                    code_challenge: str, state: str) -> str:
    """Secure login URL với PKCE + state (port get_secure_login_data)."""
    params = {
        "client_id": client_id,
        "response_type": "code",
        "redirect_uri": redirect_uri,
        "response_mode": "query",
        "scope": SCOPE,
        "code_challenge": code_challenge,
        "code_challenge_method": "S256",
        "state": state,
    }
    return urllib.parse.urlparse(AUTH_URL)._replace(
        query=urllib.parse.urlencode(params)).geturl()


def parse_auth_code_url(url: str, expected_state: str) -> str:
    """Lấy auth code từ redirect URL, verify state chống CSRF (port parse_auth_code_url)."""
    qs = urllib.parse.parse_qs(urllib.parse.urlparse(url).query)
    if expected_state is not None and qs.get("state", [None])[0] != expected_state:
        raise AuthError(codes.AUTH_FAILED, "OAuth state mismatch (possible CSRF)")
    code = qs.get("code", [None])[0]
    if not code:
        raise AuthError(codes.AUTH_FAILED, "No auth code in redirect URL")
    return code


class MicrosoftAuthProvider:
    """Microsoft OAuth flow — dùng minecraft_launcher_lib helpers khi có."""

    def __init__(self, client_id: str, redirect_uri: str = "http://localhost:antares") -> None:
        self.client_id = client_id
        self.redirect_uri = redirect_uri

    def start_login(self) -> dict:
        """Bước 1: sinh login data. Trả {url, state, code_verifier}."""
        verifier, challenge = generate_pkce()
        state = generate_state()
        url = build_login_url(self.client_id, self.redirect_uri,
                              code_challenge=challenge, state=state)
        return {"url": url, "state": state, "code_verifier": verifier}

    def finish_login(self, redirect_url: str, *, state: str,
                     code_verifier: str) -> dict:
        """Bước 2: từ redirect URL -> account dict (complete_login flow)."""
        auth_code = parse_auth_code_url(redirect_url, state)

        try:
            import requests
            # 1. authorization token
            token_resp = requests.post(TOKEN_URL, data={
                "client_id": self.client_id,
                "scope": SCOPE,
                "code": auth_code,
                "redirect_uri": self.redirect_uri,
                "grant_type": "authorization_code",
                "code_verifier": code_verifier,
            }, headers={"Content-Type": "application/x-www-form-urlencoded"},
                timeout=30)
            token_resp.raise_for_status()
            token_data = token_resp.json()
            access_token = token_data["access_token"]
            refresh_token = token_data.get("refresh_token", "")

            # 2. XBL
            xbl = requests.post(XBL_URL, json={
                "Properties": {"AuthMethod": "RPS",
                               "SiteName": "user.auth.xboxlive.com",
                               "RpsTicket": f"d={access_token}"},
                "RelyingParty": "http://auth.xboxlive.com",
                "TokenType": "JWT",
            }, timeout=30).json()
            xbl_token = xbl["Token"]
            userhash = xbl["DisplayClaims"]["xui"][0]["uhs"]

            # 3. XSTS
            xsts = requests.post(XSTS_URL, json={
                "Properties": {"SandboxId": "RETAIL", "UserTokens": [xbl_token]},
                "RelyingParty": "rp://api.minecraftservices.com/",
                "TokenType": "JWT",
            }, timeout=30).json()

            # 4. Minecraft services
            mc = requests.post(MC_AUTH_URL, json={
                "identityToken": f"XBL3.0 x={userhash};{xsts['Token']}",
            }, timeout=30).json()
            if "access_token" not in mc:
                raise AuthError(codes.AUTH_FAILED,
                                "Azure app not permitted for Minecraft API")

            mc_token = mc["access_token"]

            # 5. Profile
            profile = requests.get(MC_PROFILE_URL, headers={
                "Authorization": f"Bearer {mc_token}",
            }, timeout=30).json()
            if profile.get("error") == "NOT_FOUND":
                raise AuthError(codes.AUTH_FAILED, "Account does not own Minecraft")

            return {
                "id": profile["id"],
                "type": "microsoft",
                "displayName": profile["name"],
                "minecraftUuid": profile["id"],
                "token": mc_token,
                "_refresh_token": refresh_token,  # internal, không lên UI
            }
        except AuthError:
            raise
        except Exception as e:
            logger.warning("MS login failed: %s", e)
            raise AuthError(codes.AUTH_FAILED, f"Microsoft login failed: {e}") from e

    def refresh(self, refresh_token: str) -> dict:
        """Refresh token flow (port complete_refresh)."""
        try:
            import requests
            token_resp = requests.post(
                "https://login.live.com/oauth20_token.srf", data={
                    "client_id": self.client_id,
                    "scope": SCOPE,
                    "refresh_token": refresh_token,
                    "grant_type": "refresh_token",
                }, timeout=30)
            data = token_resp.json()
            if "error" in data:
                raise AuthError(codes.AUTH_FAILED, "Invalid refresh token")
            return self.finish_login_flow_from_token(data["access_token"],
                                                     data.get("refresh_token", ""))
        except AuthError:
            raise
        except Exception as e:
            raise AuthError(codes.AUTH_FAILED, f"Refresh failed: {e}") from e

    def finish_login_flow_from_token(self, access_token: str, refresh_token: str) -> dict:
        import requests
        xbl = requests.post(XBL_URL, json={
            "Properties": {"AuthMethod": "RPS",
                           "SiteName": "user.auth.xboxlive.com",
                           "RpsTicket": f"d={access_token}"},
            "RelyingParty": "http://auth.xboxlive.com", "TokenType": "JWT",
        }, timeout=30).json()
        xsts = requests.post(XSTS_URL, json={
            "Properties": {"SandboxId": "RETAIL",
                           "UserTokens": [xbl["Token"]]},
            "RelyingParty": "rp://api.minecraftservices.com/", "TokenType": "JWT",
        }, timeout=30).json()
        mc = requests.post(MC_AUTH_URL, json={
            "identityToken": f"XBL3.0 x={xbl['DisplayClaims']['xui'][0]['uhs']};{xsts['Token']}",
        }, timeout=30).json()
        profile = requests.get(MC_PROFILE_URL, headers={
            "Authorization": f"Bearer {mc['access_token']}"}, timeout=30).json()
        return {
            "id": profile["id"], "type": "microsoft",
            "displayName": profile["name"], "minecraftUuid": profile["id"],
            "token": mc["access_token"], "_refresh_token": refresh_token,
        }
