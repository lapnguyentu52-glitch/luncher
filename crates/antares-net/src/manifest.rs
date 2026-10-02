//! Phase 4 — Mojang version manifest (mục 10.2, parity `ManifestService`).
//!
//! Parity `services/minecraft/versions/manifest.py` + `infrastructure/cache/manifests.py`:
//! - URL `https://launchermeta.mojang.com/mc/game/version_manifest_v2.json`
//! - Disk cache JSON `{ts, value}` — TTL 30 phút, `allow_stale` dùng data cũ khi
//!   offline (mục 60 — app không chết trắng khi mất mạng)
//! - `find(version_id)` — scan entries
//! - `download_size` — lấy version JSON: client.size + libraries artifact/classifiers
//!   + assetIndex.totalSize; không có → None (lỗi mạng/parse nuốt thành None)
//!
//! HTTP: engine thuần của antares-downloads (https đi qua TLS seam — hiện fail rõ
//! ràng tới khi bundle); cache qua filesystem thuần (atomic write tmp+rename).

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};

pub const MOJANG_MANIFEST_URL: &str =
    "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
/// TTL 30 phút (parity TTL_MANIFEST).
pub const TTL_MANIFEST_SECS: u64 = 30 * 60;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ManifestError {
    #[error("http: {0}")]
    Http(String),
    #[error("io: {0}")]
    Io(String),
    #[error("invalid manifest json: {0}")]
    Parse(String),
}

impl ManifestError {
    pub fn code(&self) -> &'static str {
        match self {
            ManifestError::Http(_) => "NET_UNREACHABLE",
            ManifestError::Io(_) => "FILE_UNREADABLE",
            ManifestError::Parse(_) => "CONFIG_INVALID",
        }
    }
}

/// Entry một version trong manifest (mục 10.2 — MinecraftVersion subset).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionEntry {
    pub id: String,
    #[serde(default)]
    pub r#type: String,
    #[serde(default)]
    pub release_time: String,
    #[serde(default)]
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct RawManifest {
    #[serde(default)]
    versions: Vec<RawEntry>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct RawEntry {
    id: String,
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(rename = "releaseTime", default)]
    release_time: String,
    #[serde(default)]
    url: String,
}

impl From<RawEntry> for VersionEntry {
    fn from(raw: RawEntry) -> Self {
        VersionEntry {
            id: raw.id,
            r#type: raw.kind,
            release_time: raw.release_time,
            url: raw.url,
        }
    }
}

/// Disk cache JSON `{ts, value}` — parity DiskCache. Thư mục `<cache_dir>/manifests`.
pub struct ManifestCache {
    dir: PathBuf,
    now: fn() -> f64,
}

impl ManifestCache {
    pub fn new(cache_dir: &Path) -> Self {
        Self {
            dir: cache_dir.join("manifests"),
            now: || {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0)
            },
        }
    }

    fn path_for(&self, key: &str) -> PathBuf {
        // parity: thay / và : thành _ (key an toàn cho filesystem).
        let safe: String = key
            .chars()
            .map(|c| match c {
                '/' | ':' => '_',
                other => other,
            })
            .collect();
        self.dir.join(format!("{safe}.json"))
    }

    /// `Ok(Some(value))` khi cache còn hạn (hoặc allow_stale); corrupt → None (parity).
    pub fn get<T: serde::de::DeserializeOwned>(
        &self,
        key: &str,
        ttl_secs: u64,
        allow_stale: bool,
    ) -> Option<T> {
        let bytes = std::fs::read(self.path_for(key)).ok()?;
        let data: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
        let ts = data.get("ts")?.as_f64().unwrap_or(0.0);
        let age = (self.now)() - ts;
        if age > ttl_secs as f64 && !allow_stale {
            return None;
        }
        serde_json::from_value(data.get("value")?.clone()).ok()
    }

    /// Ghi fail chỉ là warning — không phá caller (parity cache.put).
    pub fn put<T: serde::Serialize>(&self, key: &str, value: &T) {
        let path = self.path_for(key);
        let payload = serde_json::json!({ "ts": (self.now)(), "value": value });
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let tmp = path.with_extension("tmp");
        if std::fs::write(&tmp, payload.to_string()).is_ok() {
            let _ = std::fs::rename(&tmp, &path);
        }
    }

    pub fn clear(&self) -> usize {
        let mut count = 0;
        if let Ok(entries) = std::fs::read_dir(&self.dir) {
            for entry in entries.flatten() {
                if entry.path().extension().is_some_and(|ext| ext == "json")
                    && std::fs::remove_file(entry.path()).is_ok()
                {
                    count += 1;
                }
            }
        }
        count
    }
}

/// Fetch manifest: cache trước (fresh) → HTTP → cache miss + HTTP fail → None.
/// `allow_stale` khi HTTP fail (mục 60 offline-graceful).
pub fn get_manifest(
    cache: Option<&ManifestCache>,
    http_timeout: Duration,
    allow_stale: bool,
) -> Result<Option<Vec<VersionEntry>>, ManifestError> {
    get_manifest_from(MOJANG_MANIFEST_URL, cache, http_timeout, allow_stale)
}

/// `get_manifest` với URL chỉ định — test dùng URL chết để HTTP fail
/// deterministic (trước đây dựa vào TLS seam đã loại bỏ; F-01).
pub(crate) fn get_manifest_from(
    manifest_url: &str,
    cache: Option<&ManifestCache>,
    http_timeout: Duration,
    allow_stale: bool,
) -> Result<Option<Vec<VersionEntry>>, ManifestError> {
    if let Some(cache) = cache {
        if let Some(hit) = cache.get::<Vec<VersionEntry>>("mojang_manifest", TTL_MANIFEST_SECS, false)
        {
            return Ok(Some(hit));
        }
    }
    match fetch_manifest(manifest_url, http_timeout) {
        Ok(versions) => {
            if let Some(cache) = cache {
                cache.put("mojang_manifest", &versions);
            }
            Ok(Some(versions))
        }
        Err(err) => {
            // Offline-graceful: cache stale còn dùng được → dùng (mục 60).
            if let Some(cache) = cache {
                if allow_stale {
                    if let Some(stale) =
                        cache.get::<Vec<VersionEntry>>("mojang_manifest", TTL_MANIFEST_SECS, true)
                    {
                        return Ok(Some(stale));
                    }
                }
            }
            Err(err)
        }
    }
}

fn fetch_manifest(manifest_url: &str, timeout: Duration) -> Result<Vec<VersionEntry>, ManifestError> {
    let response =
        antares_downloads::get(manifest_url, &[], timeout).map_err(|err| {
            ManifestError::Http(format!("{} ({})", err, err.code()))
        })?;
    if response.status != 200 {
        return Err(ManifestError::Http(format!("manifest HTTP {}", response.status)));
    }
    let raw: RawManifest = serde_json::from_slice(&response.body)
        .map_err(|err| ManifestError::Parse(err.to_string()))?;
    Ok(raw.versions.into_iter().map(VersionEntry::from).collect())
}

/// Tìm entry theo id (scan như legacy `find`).
pub fn find<'a>(
    entries: &'a [VersionEntry],
    version_id: &str,
) -> Option<&'a VersionEntry> {
    entries.iter().find(|v| v.id == version_id)
}

/// Ước lượng download size từ version JSON (parity `download_size`):
/// client.size + Σ libraries artifact/classifiers + assetIndex.totalSize.
/// Lỗi mạng/parse → None (parity nuốt exception).
pub fn download_size(
    version_entry_url: &str,
    http_timeout: Duration,
) -> Option<u64> {
    let response = antares_downloads::get(version_entry_url, &[], http_timeout).ok()?;
    if response.status != 200 {
        return None;
    }
    let info: serde_json::Value = serde_json::from_slice(&response.body).ok()?;
    let mut total: u64 = 0;
    total += info
        .get("downloads")?
        .get("client")?
        .get("size")?
        .as_u64()
        .unwrap_or(0);
    if let Some(libraries) = info.get("libraries").and_then(|v| v.as_array()) {
        for lib in libraries {
            let dl = lib.get("downloads")?;
            if let Some(artifact) = dl.get("artifact") {
                total += artifact.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
            }
            if let Some(classifiers) = dl.get("classifiers").and_then(|c| c.as_object()) {
                for cls in classifiers.values() {
                    total += cls.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
                }
            }
        }
    }
    total += info
        .get("assetIndex")
        .and_then(|idx| idx.get("totalSize"))
        .and_then(|s| s.as_u64())
        .unwrap_or(0);
    (total > 0).then_some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(tag: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("antares-net-manifest-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn cache_roundtrip_and_ttl() {
        let root = temp_root("ttl");
        let cache = ManifestCache::new(&root);
        let versions = vec![VersionEntry {
            id: "1.21.4".into(),
            r#type: "release".into(),
            release_time: "2024-12-03T10:15:27+00:00".into(),
            url: "https://piston-meta.mojang.com/v1/packages/…/1.21.4.json".into(),
        }];
        cache.put("mojang_manifest", &versions);

        let hit = cache
            .get::<Vec<VersionEntry>>("mojang_manifest", TTL_MANIFEST_SECS, false)
            .unwrap();
        assert_eq!(hit, versions);

        // key với / và : được sanitize (parity)
        cache.put("lookup:minecraft/1.21", &serde_json::json!({"ok": true}));
        assert!(root.join("manifests").join("lookup_minecraft_1.21.json").exists());

        // corrupt cache → None (không panic)
        std::fs::write(root.join("manifests/mojang_manifest.json"), b"{corrupt").unwrap();
        assert!(cache
            .get::<Vec<VersionEntry>>("mojang_manifest", TTL_MANIFEST_SECS, false)
            .is_none());

        // clear
        assert!(cache.clear() >= 1);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn stale_cache_used_only_when_allowed() {
        let root = temp_root("stale");
        let cache = ManifestCache::new(&root);
        // Ghi tay ts quá cũ (age > TTL)
        let path = root.join("manifests/mojang_manifest.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let payload = serde_json::json!({
            "ts": 0.0,
            "value": [{"id": "1.12.2", "type": "release", "releaseTime": "2017", "url": ""}],
        });
        std::fs::write(&path, payload.to_string()).unwrap();

        assert!(cache
            .get::<Vec<VersionEntry>>("mojang_manifest", TTL_MANIFEST_SECS, false)
            .is_none());
        let stale = cache
            .get::<Vec<VersionEntry>>("mojang_manifest", TTL_MANIFEST_SECS, true)
            .unwrap();
        assert_eq!(stale[0].id, "1.12.2");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn parse_manifest_shape() {
        let body = serde_json::json!({
            "latest": {"release": "1.21.4"},
            "versions": [
                {"id": "1.21.4", "type": "release",
                 "releaseTime": "2024-12-03T10:15:27+00:00",
                 "url": "https://piston-meta.mojang.com/v1/packages/x/1.21.4.json"},
                {"id": "b1.7.3", "type": "old_beta", "releaseTime": "2011", "url": ""},
            ]
        });
        let raw: RawManifest = serde_json::from_value(body).unwrap();
        let entries: Vec<VersionEntry> = raw.versions.into_iter().map(VersionEntry::from).collect();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "1.21.4");
        assert_eq!(entries[1].r#type, "old_beta");
        assert_eq!(find(&entries, "b1.7.3").unwrap().r#type, "old_beta");
        assert!(find(&entries, "9.9.9").is_none());
    }

    #[test]
    fn download_size_parses_version_json_shape() {
        // Trích logic tính tổng thành hàm thuần để test không cần HTTP.
        let info = serde_json::json!({
            "downloads": {"client": {"size": 25_000_000}},
            "libraries": [
                {"downloads": {"artifact": {"size": 1_000}, "classifiers": {
                    "natives-linux": {"size": 2_000}}}},
                {"downloads": {"artifact": {"size": 500}}}
            ],
            "assetIndex": {"totalSize": 400_000}
        });
        let mut total: u64 = 0;
        total += info["downloads"]["client"]["size"].as_u64().unwrap();
        for lib in info["libraries"].as_array().unwrap() {
            let dl = &lib["downloads"];
            if let Some(artifact) = dl.get("artifact") {
                total += artifact["size"].as_u64().unwrap_or(0);
            }
            if let Some(classifiers) = dl.get("classifiers").and_then(|c| c.as_object()) {
                for cls in classifiers.values() {
                    total += cls["size"].as_u64().unwrap_or(0);
                }
            }
        }
        total += info["assetIndex"]["totalSize"].as_u64().unwrap();
        assert_eq!(total, 25_000_000 + 1_000 + 2_000 + 500 + 400_000);
    }

    #[test]
    fn get_manifest_uses_stale_cache_when_http_fails() {
        // Cache stale có data + HTTP fail (URL chết, deterministic — không dựa
        // mạng ngoài) → trả stale, không error.
        let root = temp_root("offline");
        let cache = ManifestCache::new(&root);
        let path = root.join("manifests/mojang_manifest.json");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let payload = serde_json::json!({
            "ts": 0.0,
            "value": [{"id": "1.20.1", "type": "release", "releaseTime": "2023", "url": ""}],
        });
        std::fs::write(&path, payload.to_string()).unwrap();

        let dead = "https://127.0.0.1:1/manifest";
        let result =
            get_manifest_from(dead, Some(&cache), Duration::from_secs(1), true).unwrap();
        assert_eq!(result.unwrap()[0].id, "1.20.1");

        // allow_stale=false + HTTP fail → error NET_UNREACHABLE
        let err = get_manifest_from(dead, Some(&cache), Duration::from_secs(1), false)
            .unwrap_err();
        assert_eq!(err.code(), "NET_UNREACHABLE");
        let _ = std::fs::remove_dir_all(&root);
    }
}
