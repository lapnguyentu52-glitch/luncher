"""HTTP client dùng chung — persistent session, retry/backoff (mục 10, 592).

Kế thừa kinh nghiệm từ Spark: session lâu sống, retry 429/5xx, timeout tách
connect/read. TLS workaround KHÔNG bật global (mục 159) — chỉ khi cần.
"""
from __future__ import annotations

import time
from typing import Any

import requests
from requests.adapters import HTTPAdapter
from urllib3.util.retry import Retry

from app.version import USER_AGENT
from core.errors import codes
from core.errors.base import NetworkError
from core.logging.setup import get_logger

logger = get_logger("http")

DEFAULT_TIMEOUT = (15, 45)  # (connect, read)


def build_session(*, retries: int = 4) -> requests.Session:
    s = requests.Session()
    retry = Retry(
        total=retries, connect=retries, read=max(1, retries // 2),
        backoff_factor=1.5,
        status_forcelist=(429, 500, 502, 503, 504),
        allowed_methods=None,
    )
    adapter = HTTPAdapter(max_retries=retry)
    s.mount("https://", adapter)
    s.mount("http://", HTTPAdapter())
    s.headers["user-agent"] = USER_AGENT
    return s


class HttpClient:
    """Wrapper mỏng: mọi network error -> NetworkError có code."""

    def __init__(self, session: requests.Session | None = None) -> None:
        self.session = session or build_session()

    def get_json(self, url: str, *, timeout: tuple = DEFAULT_TIMEOUT,
                 params: dict | None = None, headers: dict | None = None) -> Any:
        return self._request_json("GET", url, timeout=timeout, params=params, headers=headers)

    def post_json(self, url: str, *, json: dict | None = None,
                  timeout: tuple = DEFAULT_TIMEOUT, headers: dict | None = None) -> Any:
        return self._request_json("POST", url, timeout=timeout, json=json, headers=headers)

    def stream(self, url: str, *, timeout: tuple = DEFAULT_TIMEOUT,
               headers: dict | None = None):
        try:
            resp = self.session.get(url, stream=True, timeout=timeout, headers=headers)
            resp.raise_for_status()
            return resp
        except requests.Timeout as e:
            raise NetworkError(codes.NETWORK_TIMEOUT, f"Timeout: {url}") from e
        except requests.RequestException as e:
            raise NetworkError(codes.NETWORK_OFFLINE, f"Request failed: {e}") from e

    def _request_json(self, method: str, url: str, **kw) -> Any:
        try:
            resp = self.session.request(method, url, timeout=kw.pop("timeout"), **kw)
            resp.raise_for_status()
            return resp.json()
        except requests.Timeout as e:
            raise NetworkError(codes.NETWORK_TIMEOUT, f"Timeout: {url}") from e
        except requests.RequestException as e:
            raise NetworkError(codes.NETWORK_OFFLINE, f"Request failed: {e}") from e

    def close(self) -> None:
        self.session.close()


def with_backoff(fn, *, attempts: int = 3, base_delay: float = 1.0,
                 retry_on: tuple = (NetworkError,)):
    """Retry helper có exponential backoff — không retry mù (mục 9.2)."""
    last: Exception | None = None
    for attempt in range(1, attempts + 1):
        try:
            return fn()
        except retry_on as e:
            last = e
            delay = min(base_delay * (2 ** (attempt - 1)), 30.0)
            logger.warning("Attempt %d/%d failed (%s) — retry in %.1fs",
                           attempt, attempts, e, delay)
            time.sleep(delay)
    assert last is not None
    raise last
