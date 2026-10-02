"""Minecraft version manifest — cached, offline-graceful (mục 10.2)."""
from __future__ import annotations

from infrastructure.cache.manifests import DiskCache, TTL_MANIFEST
from infrastructure.http.client import HttpClient

MOJANG_MANIFEST = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json"


class ManifestService:
    def __init__(self, http: HttpClient, cache: DiskCache) -> None:
        self._http = http
        self._cache = cache

    def get_manifest(self, *, allow_stale: bool = False) -> list[dict]:
        cached = self._cache.get("mojang_manifest", TTL_MANIFEST, allow_stale=allow_stale)
        if cached:
            return cached
        data = self._http.get_json(MOJANG_MANIFEST)
        versions = data.get("versions", [])
        self._cache.put("mojang_manifest", versions)
        return versions

    def find(self, version_id: str) -> dict | None:
        for v in self.get_manifest():
            if v.get("id") == version_id:
                return v
        return None

    def download_size(self, version_id: str) -> int | None:
        """Ước lượng download size (port từ Spark get_vanilla_download_size)."""
        entry = self.find(version_id)
        if not entry:
            return None
        try:
            info = self._http.get_json(entry["url"])
        except Exception:
            return None
        total = 0
        client = info.get("downloads", {}).get("client", {})
        total += client.get("size") or 0
        for lib in info.get("libraries", []):
            dl = lib.get("downloads", {})
            if "artifact" in dl:
                total += dl["artifact"].get("size") or 0
            for cls in dl.get("classifiers", {}).values():
                total += cls.get("size") or 0
        total += info.get("assetIndex", {}).get("totalSize") or 0
        return total or None
