"""Modrinth provider — search + version filtering (mục 18, 491).

Port từ Spark _mods_search/_mods_install sang provider layer.
"""
from __future__ import annotations

import json

from infrastructure.http.client import HttpClient

API = "https://api.modrinth.com/v2"


class ModrinthClient:
    def __init__(self, http: HttpClient) -> None:
        self._http = http

    def search(self, query: str, *, loader: str, mc_version: str,
               limit: int = 40) -> list[dict]:
        facets = json.dumps([["categories:" + loader], ["versions:" + mc_version]])
        params: dict = {"facets": facets, "limit": limit}
        if query:
            params["query"] = query
        else:
            params["index"] = "downloads"
        return self._http.get_json(f"{API}/search", params=params).get("hits", [])

    def get_versions(self, project_id: str, *, loader: str, mc_version: str) -> list[dict]:
        params = {
            "game_versions": json.dumps([mc_version]),
            "loaders": json.dumps([loader]),
        }
        return self._http.get_json(f"{API}/project/{project_id}/version", params=params)

    def pick_file(self, version: dict) -> dict | None:
        files = version.get("files") or []
        return next((f for f in files if f.get("primary")), files[0] if files else None)
