//! Batch 07c — **install pipeline + loader fabric/forge** parity
//! `minecraft_launcher_lib` (`install.py` / `fabric.py` / `forge.py` /
//! `_helper.inherit_json`) để `play_launch` cài instance mới và `play_install`
//! cài riêng, không cần Python sidecar.
//!
//! Parity map (khác có chủ đích được ghi rõ ngay trong code):
//!
//! | MLL | native |
//! |-----|--------|
//! | `install_minecraft_version` | [`Installer::install_version`] |
//! | `do_version_install` | [`Installer::do_version_install`] |
//! | `install_libraries` / `install_assets` | same name — download song song 8 worker (MLL ThreadPoolExecutor) |
//! | `fabric.install_fabric` (installer jar) | [`Installer::install_fabric`] — **bỏ installer jar**: lấy profile JSON trực tiếp từ meta |
//! | `forge.install_forge_version` + `forge_processors` | [`Installer::install_forge`] + [`Installer::run_processors`] |
//! | `fabric.get_stable_minecraft_versions` / `forge.list_forge_versions` | [`list_loader_versions`] |
//! | `fabric.get_latest_loader_version` | [`Installer::latest_fabric_loader`] |
//! | `ForgeProvider.resolve_launch_version` | [`resolve_loader_version`] (qua [`Installer::resolve`]) |
//!
//! Khác biệt chủ đích:
//! - MLL nuốt lỗi download Maven nhánh `try/except: pass` → ta vẫn nuốt
//!   (`optional` job, single-attempt) để parity im lặng; artifact/classifier/
//!   version-json lỗi thật → typed §117 (mạnh hơn MLL crash ngầm).
//! - Processor Forge: MLL bỏ qua exit code (`subprocess.run` không check) →
//!   ta **fail** `LOADER_INSTALL_FAILED` khi exit != 0 (an toàn hơn — cài dở
//!   sẽ crash game khó hiểu); legacy cũng retry ×3 quanh LOADER_INSTALL_FAILED.
//! - Forge < 1.13 (`supports_automatic_install` = false): legacy mở installer
//!   GUI → ta trả `LOADER_INSTALL_FAILED` rõ ràng (không GUI trong launcher).
//! - `javaVersion` runtime (Mojang JRE download) — Batch 07d
//!   [`crate::mojang_runtime`].

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use antares_core::tasks::TaskPriority;
use antares_core::TaskRegistry;
use antares_launch::mcjson::{
    library_path, natives_classifier, rules_pass, Library, OsInfo, RuleOptions, VersionJson,
};
use antares_launch::inherit_json;
use antares_mods::zip_read_entry;
use antares_net::manifest::{find, get_manifest, ManifestCache};

use crate::error::{codes, AppError, AppResult};
use crate::instances::Instance;
use crate::lock::ResourceLock;
use crate::AppServices;

/// Maven base mặc định parity MLL (`i["url"]` vắng → libraries.minecraft.net).
pub const LIBRARIES_URL: &str = "https://libraries.minecraft.net";
/// Asset objects CDN parity `install_assets`.
pub const RESOURCES_URL: &str = "https://resources.download.minecraft.net";
/// Fabric meta — profile JSON thay cho installer jar (parity installer ghi
/// đúng file `/v2/versions/loader/{mc}/{loader}/profile/json`).
pub const FABRIC_META_URL: &str = "https://meta.fabricmc.net";
/// Forge maven — installer jar + maven-metadata.xml.
pub const FORGE_MAVEN_URL: &str = "https://maven.minecraftforge.net";
/// Danh sách Mojang JRE runtime (all.json) — parity MLL
/// `runtime._JVM_MANIFEST_URL` (hash = path version cố định của Mojang).
pub const JAVA_RUNTIME_MANIFEST_URL: &str =
    "https://launchermeta.mojang.com/v1/products/java-runtime/2ec0cc96c44e5a76b9c8b7c39df7210883d12871/all.json";

const HTTP_TIMEOUT: Duration = Duration::from_secs(30);
/// Download song song (MLL ThreadPoolExecutor không giới hạn — ta cap 8).
const DOWNLOAD_WORKERS: usize = 8;
/// Guard chuỗi `inheritsFrom` vòng lặp.
const MAX_INHERIT_DEPTH: usize = 8;

/// Retry download mặc định (Installer + Mojang runtime).
pub const DOWNLOAD_MAX_ATTEMPTS: u32 = 8;
/// Backoff mặc định — false khi test offline (fail nhanh).
pub const DOWNLOAD_BACKOFF: bool = true;

/// Callback tiến độ `(status, fraction 0..1)` — `Sync` vì download song song.
pub type Progress<'a> = &'a (dyn Fn(&str, f64) + Sync);

/// Progress no-op (play/caller không cần báo cáo).
pub fn no_progress(_status: &str, _fraction: f64) {}

// ---------------------------------------------------------------------------
// Endpoints — test inject local server / dead URL
// ---------------------------------------------------------------------------

/// Base URL các nguồn metadata/download. Production = [`Endpoints::default`];
/// test truyền `http://127.0.0.1:1` (fail nhanh, offline-deterministic).
#[derive(Debug, Clone)]
pub struct Endpoints {
    pub fabric_meta: String,
    pub forge_maven: String,
    pub libraries: String,
    pub resources: String,
    /// URL all.json của Mojang JRE runtime (parity MLL `_JVM_MANIFEST_URL`).
    /// URL manifest platform bên trong là ABSOLUTE (từ chính all.json) — test
    /// nhúng URL local server vào all.json đã serve.
    pub java_runtime: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            fabric_meta: FABRIC_META_URL.into(),
            forge_maven: FORGE_MAVEN_URL.into(),
            libraries: LIBRARIES_URL.into(),
            resources: RESOURCES_URL.into(),
            java_runtime: JAVA_RUNTIME_MANIFEST_URL.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// HTTP helpers — metadata fetch (không qua download pipeline: body nhỏ)
// ---------------------------------------------------------------------------

fn http_get(url: &str) -> AppResult<Vec<u8>> {
    let response = antares_downloads::get(url, &[], HTTP_TIMEOUT)
        .map_err(|err| AppError::new(codes::NETWORK_UNAVAILABLE, format!("GET {url}: {err}")))?;
    if response.status != 200 {
        return Err(AppError::new(
            codes::NETWORK_UNAVAILABLE,
            format!("GET {url}: HTTP {}", response.status),
        ));
    }
    Ok(response.body)
}

/// `http_get` + parse JSON (manifest Mojang nhỏ) — `pub(crate)` cho
/// [`crate::mojang_runtime`].
pub(crate) fn http_get_json(url: &str) -> AppResult<serde_json::Value> {
    let body = http_get(url)?;
    serde_json::from_slice(&body)
        .map_err(|err| AppError::new(codes::NETWORK_UNAVAILABLE, format!("GET {url}: {err}")))
}

/// Map lỗi download pipeline → catalog §117.
fn map_download(err: antares_downloads::DownloadPipelineError) -> AppError {
    use antares_downloads::DownloadPipelineError as E;
    match err {
        E::Io(msg) => AppError::new(codes::STORAGE_WRITE_FAILED, msg),
        E::Cancelled(attempt) => AppError::internal(format!("download cancelled (attempt {attempt})")),
        other => AppError::new(codes::NETWORK_UNAVAILABLE, other.to_string()),
    }
}

/// Tải một file qua pipeline (retry + sha1 + atomic) parity `download_file`.
/// `pub(crate)` — Mojang runtime tải lzma container qua pipeline rồi tự
/// decompress ([`crate::mojang_runtime`]).
pub(crate) fn fetch_file(
    url: &str,
    target: &Path,
    sha1: Option<&str>,
    optional: bool,
    tuning: (u32, bool),
) -> AppResult<()> {
    let mut request = antares_downloads::DownloadRequest::new(url, target);
    if let Some(sha1) = sha1.filter(|s| !s.is_empty()) {
        request.sha1 = Some(sha1.to_string());
    }
    if optional {
        // parity MLL maven branch: 1 lần, không backoff, lỗi → nuốt (log warn).
        request.max_attempts = 1;
        request.backoff = false;
    } else {
        request.max_attempts = tuning.0;
        request.backoff = tuning.1;
    }
    antares_downloads::download(&request)
        .map_err(map_download)
        .map(|_| ())
}

// ---------------------------------------------------------------------------
// File job runner — download song song + tiến độ
// ---------------------------------------------------------------------------

/// Một job download — `pub(crate)` + fields pub(crate) để
/// [`crate::mojang_runtime`] build job raw/lzma và truyền vào
/// [`run_jobs_with`] với closure fetch riêng.
pub(crate) struct FileJob {
    pub(crate) url: String,
    pub(crate) target: PathBuf,
    pub(crate) sha1: Option<String>,
    /// true = lỗi nuốt (parity MLL `except: pass` nhánh Maven).
    pub(crate) optional: bool,
}

/// Gộp job trùng target — version JSON thật có lib TRÙNG (vd 1.0 liệt kê
/// `jinput-platform` 2 lần) → 2 download song song cùng `.part` sẽ triệt hạ
/// nhau (File::create truncate + cleanup giữa verify). Cùng file → 1 job,
/// ưu tiên bản bắt buộc có sha1.
fn dedup_jobs(jobs: Vec<FileJob>) -> Vec<FileJob> {
    let mut seen: std::collections::HashMap<PathBuf, usize> = std::collections::HashMap::new();
    let mut out: Vec<FileJob> = Vec::new();
    for job in jobs {
        match seen.get(&job.target) {
            Some(&index) => {
                let existing = &out[index];
                let better = !job.optional && (existing.optional || existing.sha1.is_none());
                if better {
                    out[index] = job;
                }
            }
            None => {
                seen.insert(job.target.clone(), out.len());
                out.push(job);
            }
        }
    }
    out
}

/// Chạy `jobs` tối đa [`DOWNLOAD_WORKERS`] thread với [`fetch_file`] +
/// `tuning` — wrapper của [`run_jobs_with`].
fn run_jobs(
    jobs: &[FileJob],
    progress: Progress<'_>,
    status: &str,
    base: f64,
    span: f64,
    tuning: (u32, bool),
) -> AppResult<()> {
    run_jobs_with(
        jobs,
        |job| fetch_file(&job.url, &job.target, job.sha1.as_deref(), job.optional, tuning),
        progress,
        status,
        base,
        span,
    )
}

/// Runner cốt lõi — `fetch` closure để caller thay cách tải (Mojang runtime
/// tải lzma rồi giải nén, parity MLL `download_file(..., lzma_compressed=True)`).
/// Trả lỗi ĐẦU TIÊN của job bắt buộc (job optional chỉ warn).
/// `progress(status, base + span * done/total)`.
pub(crate) fn run_jobs_with(
    jobs: &[FileJob],
    fetch: impl Fn(&FileJob) -> AppResult<()> + Sync,
    progress: Progress<'_>,
    status: &str,
    base: f64,
    span: f64,
) -> AppResult<()> {
    let total = jobs.len();
    if total == 0 {
        return Ok(());
    }
    progress(status, base);
    let next = Mutex::new(0usize);
    let done = AtomicUsize::new(0);
    let first_err: Mutex<Option<AppError>> = Mutex::new(None);
    let workers = total.min(DOWNLOAD_WORKERS);

    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = {
                        let mut slot = next.lock().unwrap_or_else(|e| e.into_inner());
                        if *slot >= total {
                            break;
                        }
                        let index = *slot;
                        *slot += 1;
                        index
                    };
                    if first_err.lock().unwrap_or_else(|e| e.into_inner()).is_some() {
                        break; // đã có lỗi bắt buộc — dừng sớm
                    }
                    let job = &jobs[index];
                    if let Err(err) = fetch(job) {
                        if job.optional {
                            log::warn!("optional download bỏ qua {} ({err})", job.url);
                        } else {
                            let mut slot = first_err.lock().unwrap_or_else(|e| e.into_inner());
                            if slot.is_none() {
                                *slot = Some(err);
                            }
                            break;
                        }
                    }
                    let finished = done.fetch_add(1, Ordering::Relaxed) + 1;
                    progress(status, base + span * (finished as f64 / total as f64));
                }
            });
        }
    });

    first_err
        .into_inner()
        .unwrap_or_else(|e| e.into_inner())
        .map_or(Ok(()), Err)
}

// ---------------------------------------------------------------------------
// Maven helpers — parity _helper.get_library_path / install_libraries URL build
// ---------------------------------------------------------------------------

struct MavenName<'a> {
    group: &'a str,
    artifact: &'a str,
    /// version component (đã tách `@suffix`) — MLL BỎ parts[3..] khi build URL.
    version: &'a str,
    suffix: &'a str,
}

/// Parse `group:artifact:version[:classifier][@suffix]` theo đúng cách
/// `install_libraries` slice `[0:3]` (classifier bị bỏ — parity quirk).
fn parse_maven_name(name: &str) -> Option<MavenName<'_>> {
    let (name, suffix) = match name.split_once('@') {
        Some((n, s)) => (n, s),
        None => (name, "jar"),
    };
    let mut parts = name.split(':');
    let group = parts.next()?;
    let artifact = parts.next()?;
    let version = parts.next()?;
    if group.is_empty() || artifact.is_empty() || version.is_empty() {
        return None;
    }
    Some(MavenName { group, artifact, version, suffix })
}

/// Parity `install_libraries`: thư mục + URL Maven cho một lib.
/// Trả `(relative_path, url)` — relative dùng cho cả 2.
fn maven_rel_and_url(name: &str, base: &str) -> Option<(String, String)> {
    let parsed = parse_maven_name(name)?;
    let rel = format!(
        "{}/{artifact}/{version}/{artifact}-{version}.{suffix}",
        parsed.group.replace('.', "/"),
        artifact = parsed.artifact,
        version = parsed.version,
        suffix = parsed.suffix,
    );
    let base = base.trim_end_matches('/');
    Some((rel.clone(), format!("{base}/{rel}")))
}

// ---------------------------------------------------------------------------
// Installer
// ---------------------------------------------------------------------------

/// Một lần cài/repair — giữ đường dẫn + endpoints (test inject).
pub struct Installer {
    game_dir: PathBuf,
    cache_dir: PathBuf,
    endpoints: Endpoints,
    /// Retry download (mặc định = engine 8/backoff). Test offline dùng
    /// [`Installer::with_download_tuning`] để fail nhanh, không chờ backoff.
    download_max_attempts: u32,
    download_backoff: bool,
}

impl Installer {
    pub fn new(game_dir: &Path, cache_dir: &Path, endpoints: Endpoints) -> Self {
        Self {
            game_dir: game_dir.to_path_buf(),
            cache_dir: cache_dir.to_path_buf(),
            endpoints,
            download_max_attempts: DOWNLOAD_MAX_ATTEMPTS,
            download_backoff: DOWNLOAD_BACKOFF,
        }
    }

    /// Knob cho test/dev: giảm số lần retry + tắt backoff (endpoint chết →
    /// fail ngay thay vì ~44s retry).
    pub fn with_download_tuning(mut self, max_attempts: u32, backoff: bool) -> Self {
        self.download_max_attempts = max_attempts.max(1);
        self.download_backoff = backoff;
        self
    }

    pub fn game_dir(&self) -> &Path {
        &self.game_dir
    }

    /// `fetch_file` với tuning của installer (test offline fail nhanh).
    fn fetch(&self, url: &str, target: &Path, sha1: Option<&str>, optional: bool) -> AppResult<()> {
        fetch_file(
            url,
            target,
            sha1,
            optional,
            (self.download_max_attempts, self.download_backoff),
        )
    }

    fn versions_dir(&self, id: &str) -> PathBuf {
        self.game_dir.join("versions").join(id)
    }

    fn version_json(&self, id: &str) -> PathBuf {
        self.versions_dir(id).join(format!("{id}.json"))
    }

    fn version_jar(&self, id: &str) -> PathBuf {
        self.versions_dir(id).join(format!("{id}.jar"))
    }

    // -----------------------------------------------------------------------
    // Resolve — parity providers `resolve_launch_version`
    // -----------------------------------------------------------------------

    /// Parity `Provider.resolve_launch_version`:
    /// - vanilla → identity
    /// - fabric → `fabric-loader-{latest}-{mc}` (meta `/v2/versions/loader[0]`;
    ///   offline grace: quét `versions/` đã cài — dev parity, không fail khi
    ///   mất mạng nhưng instance đã cài)
    /// - forge → `{mc}-forge-{build}` từ `1.20.1-47.2.0` (không network)
    /// - khác → `VALIDATION_FAILED Unknown loader` (parity `get_loader`)
    pub fn resolve(&self, instance: &Instance) -> AppResult<String> {
        match instance.loader.as_str() {
            "vanilla" => Ok(instance.minecraft_version.clone()),
            "fabric" => {
                let mc = instance.minecraft_version.as_str();
                match self.latest_fabric_loader() {
                    Ok(loader) => Ok(format!("fabric-loader-{loader}-{mc}")),
                    Err(err) => match self.find_installed_fabric(mc) {
                        Some(installed) => {
                            log::warn!("fabric meta không tới được ({err}) — dùng version đã cài: {installed}");
                            Ok(installed)
                        }
                        None => Err(err),
                    },
                }
            }
            "forge" => Ok(forge_to_installed_version(&instance.minecraft_version)),
            other => Err(AppError::new(
                codes::VALIDATION_FAILED,
                format!("Unknown loader: {other}"),
            )),
        }
    }

    /// Parity `fabric.get_latest_loader_version` — `[0].version` của
    /// `/v2/versions/loader` (MLL lấy index 0, không filter stable).
    pub fn latest_fabric_loader(&self) -> AppResult<String> {
        let url = format!("{}/v2/versions/loader", self.endpoints.fabric_meta);
        let body = http_get(&url)?;
        let value: serde_json::Value = serde_json::from_slice(&body).map_err(|err| {
            AppError::new(codes::NETWORK_UNAVAILABLE, format!("fabric loader list: {err}"))
        })?;
        value
            .get(0)
            .and_then(|entry| entry.get("version"))
            .and_then(|v| v.as_str())
            .map(str::to_string)
            .ok_or_else(|| AppError::new(codes::NETWORK_UNAVAILABLE, "fabric loader list rỗng"))
    }

    /// Offline grace — tìm `versions/fabric-loader-*-{mc}` đã cài (giữ
    /// instance launch được khi mất mạng; parity không có nhánh này).
    fn find_installed_fabric(&self, mc: &str) -> Option<String> {
        let suffix = format!("-{mc}");
        let entries = std::fs::read_dir(self.game_dir.join("versions")).ok()?;
        let mut found: Vec<String> = entries
            .flatten()
            .filter_map(|entry| entry.file_name().to_string_lossy().into_owned().into())
            .filter(|name: &String| {
                name.starts_with("fabric-loader-") && name.ends_with(&suffix)
            })
            .collect();
        found.sort();
        found.pop()
    }

    /// Parity `forge.forge_to_installed_version` — `1.20.1-47.2.0` →
    /// `1.20.1-forge-47.2.0`; không đủ `-` → giữ nguyên (parity
    /// `ForgeProvider.resolve_launch_version` fallback).
    pub fn forge_to_installed_version(forge_version: &str) -> String {
        match forge_version.split_once('-') {
            Some((mc, build)) => format!("{mc}-forge-{build}"),
            None => forge_version.to_string(),
        }
    }

    /// `versions/<resolved>/<resolved>.json` — parity `_is_installed`.
    pub fn is_installed(&self, resolved: &str) -> bool {
        self.version_json(resolved).is_file()
    }

    // -----------------------------------------------------------------------
    // ensure_installed — parity orchestrator step 3 (loader.install)
    // -----------------------------------------------------------------------

    /// Cài/repair theo loader. JSON đã có trên đĩa vẫn được verify/repair
    /// (parity `install_minecraft_version` — "verifies and repairs").
    pub fn ensure_installed(
        &self,
        loader: &str,
        mc_version: &str,
        resolved: &str,
        progress: Progress<'_>,
    ) -> AppResult<()> {
        match loader {
            "vanilla" => self.install_version(resolved, progress),
            "fabric" => self.install_fabric(mc_version, resolved, progress),
            "forge" => self.install_forge(mc_version, resolved, progress),
            other => Err(AppError::new(
                codes::VALIDATION_FAILED,
                format!("Unknown loader: {other}"),
            )),
        }
    }

    // -----------------------------------------------------------------------
    // install_minecraft_version — parity install.py
    // -----------------------------------------------------------------------

    /// Parity `install_minecraft_version(version, path)`:
    /// JSON đã có → repair trực tiếp; thiếu → manifest (cache 30' + stale
    /// offline) tìm URL+sha1 → tải; không có trong manifest →
    /// `MINECRAFT_VERSION_NOT_FOUND` (parity `VersionNotFound`).
    pub fn install_version(&self, version_id: &str, progress: Progress<'_>) -> AppResult<()> {
        if self.version_json(version_id).is_file() {
            return self.do_version_install(version_id, None, None, 0, progress);
        }
        progress("Resolving version", 0.02);
        let entry = self.manifest_entry(version_id)?;
        let sha1 = (!entry.sha1.is_empty()).then_some(entry.sha1.as_str());
        self.do_version_install(version_id, Some(entry.url.as_str()), sha1, 0, progress)
    }

    fn manifest_entry(&self, version_id: &str) -> AppResult<antares_net::manifest::VersionEntry> {
        let cache = ManifestCache::new(&self.cache_dir);
        let entries = get_manifest(Some(&cache), HTTP_TIMEOUT, true)
            .map_err(|err| AppError::new(codes::NETWORK_UNAVAILABLE, err.to_string()))?
            .unwrap_or_default();
        find(&entries, version_id).cloned().ok_or_else(|| {
            AppError::new(
                codes::MINECRAFT_VERSION_NOT_FOUND,
                format!("Minecraft version {version_id} not found in manifest"),
            )
        })
    }

    /// Parity `do_version_install` — merge `inheritsFrom` (đệ quy, có cài
    /// parent trước) rồi: libraries → assets → logging → client jar → copy
    /// jar cho version kế thừa. `javaVersion` runtime → `resolve_java_for`
    /// (`launch.rs` → `mojang_runtime.rs`, Batch 07d).
    fn do_version_install(
        &self,
        version_id: &str,
        url: Option<&str>,
        sha1: Option<&str>,
        depth: usize,
        progress: Progress<'_>,
    ) -> AppResult<()> {
        if depth > MAX_INHERIT_DEPTH {
            return Err(AppError::new(
                codes::VALIDATION_FAILED,
                format!("inheritsFrom quá sâu: {version_id}"),
            ));
        }
        std::fs::create_dir_all(self.versions_dir(version_id)).map_err(|err| {
            AppError::new(codes::STORAGE_WRITE_FAILED, format!("mkdir versions: {err}"))
        })?;

        // 1. Version JSON — parity download_file(url, ..., sha1) + open.
        if let Some(url) = url {
            progress("Download version json", 0.03);
            self.fetch(url, &self.version_json(version_id), sha1, false)?;
        }
        let bytes = std::fs::read(self.version_json(version_id)).map_err(|err| {
            AppError::new(
                codes::MC_VERSION_UNKNOWN,
                format!("version {version_id} chưa được cài: {err}"),
            )
        })?;
        let child: serde_json::Value = serde_json::from_slice(&bytes).map_err(|err| {
            AppError::new(
                codes::MC_VERSION_UNKNOWN,
                format!("version {version_id} json hỏng: {err}"),
            )
        })?;

        // 2. inheritsFrom — parity: cài parent trước rồi merge (MLL nuốt
        //    VersionNotFound của parent rồi crash đọc file — ta báo typed).
        let merged = match child.get("inheritsFrom").and_then(|v| v.as_str()) {
            None => child,
            Some(parent_id) => {
                self.install_version(parent_id, progress)?;
                let parent_bytes = std::fs::read(self.version_json(parent_id)).map_err(|err| {
                    AppError::new(
                        codes::MC_VERSION_UNKNOWN,
                        format!("version {parent_id} chưa được cài: {err}"),
                    )
                })?;
                let parent: serde_json::Value = serde_json::from_slice(&parent_bytes)
                    .map_err(|err| {
                        AppError::new(
                            codes::MC_VERSION_UNKNOWN,
                            format!("version {parent_id} json hỏng: {err}"),
                        )
                    })?;
                inherit_json(&child, &parent)
            }
        };
        let vj: VersionJson = serde_json::from_value(merged).map_err(|err| {
            AppError::new(
                codes::MC_VERSION_UNKNOWN,
                format!("version {version_id} json không hợp lệ: {err}"),
            )
        })?;

        // 3. Libraries — parity install_libraries (song song, natives extract).
        self.install_libraries(version_id, &vj.libraries, progress)?;

        // 4. Assets — parity install_assets (index + objects content-addressed).
        self.install_assets(&vj, progress)?;

        // 5. Logging config — parity `if "logging" in versiondata`.
        self.install_logging(&vj);

        // 6. Client jar — parity `if "downloads" in versiondata`.
        if let Some(client) = vj.downloads.as_ref().and_then(|d| d.client.as_ref()) {
            if let Some(url) = client.url.as_deref().filter(|u| !u.is_empty()) {
                progress("Download client jar", 0.95);
                self.fetch(
                    url,
                    &self.version_jar(version_id),
                    client.sha1.as_deref(),
                    false,
                )?;
            }
        }

        // 7. Copy jar cho version kế thừa khi jar của id chưa có — parity
        //    nhánh "copy jar for old forge" (MLL viết src→dst HƯỚNG SAI →
        //    FileNotFoundError; ta copy đúng hướng: parent → child).
        if let Some(parent_id) = &vj.inherits_from {
            let jar = self.version_jar(version_id);
            if !jar.is_file() {
                let parent_jar = self.version_jar(parent_id);
                if parent_jar.is_file() {
                    if let Err(err) = std::fs::copy(&parent_jar, &jar) {
                        log::warn!("copy jar kế thừa thất bại ({} → {}): {err}", parent_jar.display(), jar.display());
                    }
                }
            }
        }

        progress("Installation complete", 1.0);
        Ok(())
    }

    /// Parity `install_libraries(id, libraries, path)`:
    /// - rules filter (OS hiện tại)
    /// - `downloads.artifact` url≠"" + path → tải đúng artifact (sha1)
    /// - còn lại → Maven `lib.url` (trim `/`) hoặc `libraries.minecraft.net`
    ///   — lỗi NUỐT (parity `try/except: pass`), single attempt
    /// - native classifier → tải (sha1, bắt buộc) + extract vào
    ///   `versions/<id>/natives` với `lib.extract.exclude`
    fn install_libraries(
        &self,
        version_id: &str,
        libraries: &[Library],
        progress: Progress<'_>,
    ) -> AppResult<()> {
        progress("Download Libraries", 0.05);
        let os = OsInfo::current();
        let natives_dest = self.versions_dir(version_id).join("natives");
        let mut jobs: Vec<FileJob> = Vec::new();
        let mut extracts: Vec<(PathBuf, Vec<String>)> = Vec::new();

        for lib in libraries {
            if let Some(rules) = &lib.rules {
                // parity MLL: library rules luôn với `options={}`.
                if !rules_pass(rules, &os, &RuleOptions::default()) {
                    continue;
                }
            }
            let native = natives_classifier(lib, &os);
            let base = lib
                .url
                .as_deref()
                .map(str::to_string)
                .unwrap_or_else(|| self.endpoints.libraries.clone());
            let Some((rel, maven_url)) = maven_rel_and_url(&lib.name, &base) else {
                log::warn!("bỏ lib name không parse được: {}", lib.name);
                continue;
            };
            let maven_target = self.game_dir.join("libraries").join(&rel);

            // Artifact branch — parity: url != "" && có path.
            let artifact = lib
                .downloads
                .as_ref()
                .and_then(|d| d.artifact.as_ref())
                .filter(|a| {
                    a.url.as_deref().is_some_and(|u| !u.is_empty()) && a.path.is_some()
                });
            if let Some(artifact) = artifact {
                let path = artifact.path.as_deref().unwrap_or(rel.as_str());
                jobs.push(FileJob {
                    url: artifact.url.clone().unwrap_or_default(),
                    target: self.game_dir.join("libraries").join(path),
                    sha1: artifact.sha1.clone(),
                    optional: false,
                });
            } else {
                // Maven branch — optional (MLL swallow).
                jobs.push(FileJob {
                    url: maven_url,
                    target: maven_target.clone(),
                    sha1: None,
                    optional: true,
                });
            }

            // Natives — parity `if native != ""`.
            // Target = thư mục maven của lib + `-{native}` trước đuôi file —
            // ĐÚNG chỗ classpath mong đợi (parity `natives_jar_path` của MLL:
            // get_library_path(name) splitext + `-native`).
            if let Some(native) = &native {
                let Some(parsed) = parse_maven_name(&lib.name) else {
                    continue;
                };
                let native_rel = format!(
                    "{}/{}/{}/{}-{version}-{native}.jar",
                    parsed.group.replace('.', "/"),
                    parsed.artifact,
                    parsed.version,
                    parsed.artifact,
                    version = parsed.version,
                    native = native,
                );
                let native_target = self.game_dir.join("libraries").join(&native_rel);
                let classifier = lib
                    .downloads
                    .as_ref()
                    .and_then(|d| d.classifiers.as_ref())
                    .and_then(|c| c.get(native));
                match classifier {
                    // Classifier có url+sha1 → tải bắt buộc (parity MLL không nuốt).
                    Some(cls) => {
                        let url = cls.get("url").and_then(|v| v.as_str()).unwrap_or("");
                        let sha1 = cls.get("sha1").and_then(|v| v.as_str());
                        if !url.is_empty() {
                            jobs.push(FileJob {
                                url: url.to_string(),
                                target: native_target.clone(),
                                sha1: sha1.map(str::to_string),
                                optional: false,
                            });
                        }
                    }
                    // Không có info classifier → Maven base + cùng path (MLL
                    // nhánh này sẽ KeyError/crash — ta optional, không chặn).
                    None => {
                        let base = base.trim_end_matches('/');
                        jobs.push(FileJob {
                            url: format!("{base}/{native_rel}"),
                            target: native_target.clone(),
                            sha1: None,
                            optional: true,
                        });
                    }
                }
                let excludes = lib
                    .extract
                    .as_ref()
                    .map(|e| e.exclude.clone())
                    .unwrap_or_default();
                extracts.push((native_target, excludes));
            }
            // Natives map vắng nhưng có extract (không có tên native để tìm
            // file): MLL NameError — ta bỏ qua im lặng (defensive parity).
        }

        let total = jobs.len();
        run_jobs(
            &dedup_jobs(jobs),
            progress,
            "Download Libraries",
            0.05,
            0.40,
            (self.download_max_attempts, self.download_backoff),
        )?;

        // Extract natives — parity `extract_natives_file` (lỗi chỉ warn:
        // game sẽ lộ rõ ở log launch).
        for (jar, excludes) in extracts {
            if !jar.is_file() {
                log::warn!("natives jar thiếu (skip extract): {}", jar.display());
                continue;
            }
            if let Err(err) = crate::launch::extract_natives_jar(&jar, &natives_dest, &excludes) {
                log::warn!("extract natives {} thất bại: {err}", jar.display());
            }
        }
        if total > 0 {
            progress("Download Libraries", 0.45);
        }
        Ok(())
    }

    /// Parity `install_assets` — index (`assets/indexes/<assets>.json`, sha1)
    /// rồi objects content-addressed `resources.download.minecraft.net/<h0..1>/<h>`.
    fn install_assets(&self, vj: &VersionJson, progress: Progress<'_>) -> AppResult<()> {
        let Some(index) = vj.asset_index.as_ref() else {
            return Ok(()); // version cũ không có assetIndex
        };
        let Some(url) = index.url.as_deref().filter(|u| !u.is_empty()) else {
            log::warn!("assetIndex thiếu url — bỏ qua assets");
            return Ok(());
        };
        progress("Download Assets", 0.46);
        let index_name = vj
            .assets
            .clone()
            .or_else(|| index.id.clone())
            .unwrap_or_else(|| vj.id.clone());
        let index_path = self
            .game_dir
            .join("assets")
            .join("indexes")
            .join(format!("{index_name}.json"));
        self.fetch(url, &index_path, index.sha1.as_deref(), false)?;

        let bytes = std::fs::read(&index_path).map_err(|err| {
            AppError::new(codes::STORAGE_WRITE_FAILED, format!("asset index: {err}"))
        })?;
        let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|err| {
            AppError::new(codes::CONFIG_INVALID, format!("asset index json: {err}"))
        })?;
        let Some(objects) = value.get("objects").and_then(|o| o.as_object()) else {
            return Ok(());
        };

        // HashSet parity `set(val["hash"])` — dedupe + thứ tự deterministic.
        let mut hashes: Vec<String> = objects
            .values()
            .filter_map(|obj| obj.get("hash").and_then(|h| h.as_str()).map(str::to_string))
            .collect::<std::collections::BTreeSet<String>>()
            .into_iter()
            .collect();
        hashes.sort();

        let jobs: Vec<FileJob> = hashes
            .iter()
            .filter(|hash| hash.len() >= 4) // defensive: index hỏng → tránh slice panic
            .map(|hash| FileJob {
                url: format!("{}/{}/{}", self.endpoints.resources, &hash[..2], hash),
                target: self
                    .game_dir
                    .join("assets")
                    .join("objects")
                    .join(&hash[..2])
                    .join(hash),
                sha1: Some(hash.clone()),
                optional: false,
            })
            .collect();
        run_jobs(
            &jobs,
            progress,
            "Download Assets",
            0.46,
            0.49,
            (self.download_max_attempts, self.download_backoff),
        )
    }

    /// Parity `logging` — tải `assets/log_configs/<file.id>` (MLL luôn tải;
    /// legacy không bật logging config lúc launch → lỗi chỉ warn, không chặn).
    fn install_logging(&self, vj: &VersionJson) {
        let Some(file) = vj
            .logging
            .as_ref()
            .and_then(|l| l.client.as_ref())
            .and_then(|c| c.file.as_ref())
        else {
            return;
        };
        let (Some(id), Some(url)) = (file.id.as_deref(), file.url.as_deref()) else {
            return;
        };
        let target = self.game_dir.join("assets").join("log_configs").join(id);
        if let Err(err) = self.fetch(url, &target, file.sha1.as_deref(), true) {
            log::warn!("logging config {id} bỏ qua: {err}");
        }
    }

    // -----------------------------------------------------------------------
    // Fabric — parity fabric.install_fabric (khác: không chạy installer jar)
    // -----------------------------------------------------------------------

    /// Parity `install_fabric`: cài vanilla trước rồi ghi profile
    /// `versions/<resolved>/<resolved>.json` (installer jar của fabric ghi
    /// ĐÚNG file này từ meta — ta fetch trực tiếp, không java -jar) rồi
    /// `install_minecraft_version(resolved)` (merge parent + libs/assets).
    ///
    /// JSON profile đã có → bỏ qua fetch (repair offline); loader version
    /// khác nhau → id khác nhau → vẫn fetch (đúng như resolve yêu cầu).
    fn install_fabric(
        &self,
        mc_version: &str,
        resolved: &str,
        progress: Progress<'_>,
    ) -> AppResult<()> {
        progress("Install vanilla base", 0.01);
        self.install_version(mc_version, progress)?;

        let loader_version = fabric_loader_version(resolved, mc_version).ok_or_else(|| {
            AppError::new(
                codes::VALIDATION_FAILED,
                format!("resolved không phải fabric-loader id: {resolved}"),
            )
        })?;

        if !self.version_json(resolved).is_file() {
            progress("Download fabric profile", 0.1);
            let url = format!(
                "{}/v2/versions/loader/{mc_version}/{loader_version}/profile/json",
                self.endpoints.fabric_meta
            );
            let body = http_get(&url)?;
            std::fs::create_dir_all(self.versions_dir(resolved)).map_err(|err| {
                AppError::new(codes::STORAGE_WRITE_FAILED, format!("mkdir fabric: {err}"))
            })?;
            std::fs::write(self.version_json(resolved), body).map_err(|err| {
                AppError::new(codes::STORAGE_WRITE_FAILED, format!("write fabric profile: {err}"))
            })?;
        }

        self.install_version(resolved, progress)
    }

    // -----------------------------------------------------------------------
    // Forge — parity forge.install_forge_version + forge_processors
    // -----------------------------------------------------------------------

    /// Parity `install_forge_version(versionid, path)`:
    /// installer jar → `install_profile.json` → cài vanilla base → profile
    /// libraries → extract `version.json` + universal jar + `data/client.lzma`
    /// → `install_minecraft_version(forge_id)` → chạy processors client.
    fn install_forge(
        &self,
        mc_version: &str,
        resolved: &str,
        progress: Progress<'_>,
    ) -> AppResult<()> {
        if !supports_automatic_install(mc_version) {
            return Err(AppError::new(
                codes::LOADER_INSTALL_FAILED,
                format!(
                    "Forge {mc_version} không hỗ trợ cài tự động (cần Minecraft ≥ 1.13) — legacy chạy installer GUI"
                ),
            ));
        }
        let versionid = mc_version; // parity tham số install_forge_version
        let forge_version_id = Self::forge_to_installed_version(versionid);
        if forge_version_id != resolved {
            log::warn!("resolved {resolved} ≠ profile id {forge_version_id} — theo profile (parity MLL)");
        }

        progress("Download forge installer", 0.02);
        let temp = temp_dir_for("antares-forge-install");
        std::fs::create_dir_all(&temp).map_err(|err| {
            AppError::new(codes::STORAGE_WRITE_FAILED, format!("temp dir: {err}"))
        })?;
        let result = self.install_forge_inner(versionid, &forge_version_id, &temp, progress);
        // cleanup best-effort — processor {ROOT} đã dùng temp xong.
        let _ = std::fs::remove_dir_all(&temp);
        result
    }

    fn install_forge_inner(
        &self,
        versionid: &str,
        forge_version_id: &str,
        temp: &Path,
        progress: Progress<'_>,
    ) -> AppResult<()> {
        let installer_path = temp.join("installer.jar");
        let installer_url = format!(
            "{}/net/minecraftforge/forge/{v}/forge-{v}-installer.jar",
            self.endpoints.forge_maven,
            v = versionid
        );
        // parity MLL: installer không tải được → VersionNotFound → legacy
        // ForgeProvider wrap LOADER_INSTALL_FAILED (retry ×3).
        self.fetch(&installer_url, &installer_path, None, false).map_err(|err| {
            AppError::new(
                codes::LOADER_INSTALL_FAILED,
                format!("Forge installer không tải được: {err}"),
            )
        })?;
        let installer_bytes = std::fs::read(&installer_path).map_err(|err| {
            AppError::new(codes::STORAGE_WRITE_FAILED, format!("installer jar: {err}"))
        })?;

        // install_profile.json — parity `zf.open("install_profile.json")`.
        let profile_bytes = zip_read_entry(&installer_bytes, "install_profile.json").map_err(|err| {
            AppError::new(
                codes::LOADER_INSTALL_FAILED,
                format!("install_profile.json trong installer: {err}"),
            )
        })?;
        let profile: ForgeProfile = serde_json::from_slice(&profile_bytes).map_err(|err| {
            AppError::new(
                codes::LOADER_INSTALL_FAILED,
                format!("install_profile.json hỏng: {err}"),
            )
        })?;
        let minecraft = profile.minecraft_version().ok_or_else(|| {
            AppError::new(codes::LOADER_INSTALL_FAILED, "install_profile thiếu minecraft")
        })?;
        let minecraft = minecraft.as_str();

        // 1. Base version — parity install_minecraft_version(minecraft).
        progress("Install vanilla base", 0.05);
        self.install_version(minecraft, progress)?;

        // 2. Profile libraries — parity `install_libraries(minecraft, ...)`.
        if !profile.libraries.is_empty() {
            self.install_libraries(minecraft, &profile.libraries, progress)?;
        }

        // 3. Extract version.json → versions/<forge_id>/<forge_id>.json.
        //    (thiếu entry → fallback `versionInfo` của spec cũ — parity MLL).
        progress("Extract forge version json", 0.5);
        let version_json_path = self.version_json(forge_version_id);
        if let Ok(bytes) = zip_read_entry(&installer_bytes, "version.json") {
            std::fs::create_dir_all(self.versions_dir(forge_version_id)).map_err(|err| {
                AppError::new(codes::STORAGE_WRITE_FAILED, format!("mkdir forge: {err}"))
            })?;
            std::fs::write(&version_json_path, bytes).map_err(|err| {
                AppError::new(codes::STORAGE_WRITE_FAILED, format!("write version.json: {err}"))
            })?;
        } else if let Some(info) = &profile.version_info {
            std::fs::create_dir_all(self.versions_dir(forge_version_id)).map_err(|err| {
                AppError::new(codes::STORAGE_WRITE_FAILED, format!("mkdir forge: {err}"))
            })?;
            let bytes = serde_json::to_vec_pretty(info).map_err(|err| {
                AppError::new(codes::CONFIG_INVALID, format!("versionInfo serialize: {err}"))
            })?;
            std::fs::write(&version_json_path, bytes).map_err(|err| {
                AppError::new(codes::STORAGE_WRITE_FAILED, format!("write versionInfo: {err}"))
            })?;
        } else {
            return Err(AppError::new(
                codes::LOADER_INSTALL_FAILED,
                "installer thiếu version.json/versionInfo",
            ));
        }

        // 4. Universal jar — parity 3 nhánh fallback tên entry.
        let forge_lib_dir = self
            .game_dir
            .join("libraries/net/minecraftforge/forge")
            .join(versionid);
        extract_first(&installer_bytes, &[
            (
                format!("maven/net/minecraftforge/forge/{v}/forge-{v}-universal.jar", v = versionid),
                forge_lib_dir.join(format!("forge-{versionid}-universal.jar")),
            ),
            (
                format!("forge-{versionid}-universal.jar"),
                forge_lib_dir.join(format!("forge-{versionid}.jar")),
            ),
            (
                format!("maven/net/minecraftforge/forge/{v}/forge-{v}.jar", v = versionid),
                forge_lib_dir.join(format!("forge-{versionid}.jar")),
            ),
        ])?;

        // 5. data/client.lzma → temp (BINPATCH của processors).
        let lzma_path = temp.join("client.lzma");
        let have_lzma = zip_read_entry(&installer_bytes, "data/client.lzma")
            .map(|bytes| std::fs::write(&lzma_path, bytes).is_ok())
            .unwrap_or(false);
        if !have_lzma {
            log::warn!("installer không có data/client.lzma — processors có thể fail");
        }

        // 6. Cài nốt version json forge đã merge (libs/assets/client jar).
        self.install_version(forge_version_id, progress)?;

        // 7. Processors — parity forge_processors (client sides).
        self.run_processors(
            &profile,
            &installer_path,
            &lzma_path,
            temp,
            progress,
        )
    }

    /// Parity `forge_processors(data, path, lzma_path, installer_path, ...)`:
    /// chạy mọi processor có side `client` (mặc định cả 2 side = chạy):
    /// `[java, -cp, classpath, Main-Class(jar)] + args` với var substitution
    /// `{VAR}` + unwrap `[maven]`. Java = JRE resolve được (MLL hardcode
    /// `"java"` — ta dùng java ≥ required, an toàn hơn).
    fn run_processors(
        &self,
        profile: &ForgeProfile,
        installer_path: &Path,
        lzma_path: &Path,
        root: &Path,
        progress: Progress<'_>,
    ) -> AppResult<()> {
        if profile.processors.is_empty() {
            return Ok(());
        }
        let java = crate::launch::resolve_java(&profile.minecraft_version_or_empty())?;

        // argument_vars — parity thứ tự dict: data trước, {INSTALLER}/
        // {BINPATCH}/{ROOT}/{SIDE} override (key trùng → giá trị sau thắng).
        let mut vars: Vec<(String, String)> = Vec::new();
        let mut push_var = |key: String, value: String| {
            if let Some(slot) = vars.iter_mut().find(|(k, _)| *k == key) {
                slot.1 = value; // parity dict overwrite
            } else {
                vars.push((key, value));
            }
        };
        push_var(
            "{MINECRAFT_JAR}".into(),
            self.version_jar(&profile.minecraft_version_or_empty())
                .to_string_lossy()
                .into_owned(),
        );
        for (key, entry) in &profile.data {
            let Some(client) = entry.client.as_deref() else {
                continue;
            };
            let value = if client.starts_with('[') && client.ends_with(']') {
                library_path(&self.game_dir, &client[1..client.len() - 1])
                    .to_string_lossy()
                    .into_owned()
            } else {
                client.to_string()
            };
            push_var(format!("{{{key}}}"), value);
        }
        push_var("{INSTALLER}".into(), installer_path.to_string_lossy().into_owned());
        push_var("{BINPATCH}".into(), lzma_path.to_string_lossy().into_owned());
        push_var("{ROOT}".into(), root.to_string_lossy().into_owned());
        push_var("{SIDE}".into(), "client".into());

        let total = profile.processors.len();
        let mut ran = 0usize;
        for processor in &profile.processors {
            // parity: `if "client" not in i.get("sides", ["client"])` → skip.
            let sides = processor.sides.as_deref();
            if let Some(sides) = sides {
                if !sides.iter().any(|s| s == "client") {
                    continue;
                }
            }
            ran += 1;
            progress(&format!("Running processor {}", processor.jar), 0.95 + 0.05 * (ran as f64 / total as f64));

            // classpath: từng entry + sep, kết thúc bằng processor jar.
            let sep = if cfg!(windows) { ';' } else { ':' };
            let mut cp: Vec<String> = processor
                .classpath
                .iter()
                .filter_map(|name| {
                    library_path(&self.game_dir, name)
                        .to_string_lossy()
                        .into_owned()
                        .into()
                })
                .collect();
            let jar_path = library_path(&self.game_dir, &processor.jar);
            cp.push(jar_path.to_string_lossy().into_owned());
            let classpath = cp.join(&sep.to_string());

            // Main-Class từ MANIFEST.MF của processor jar (parity
            // get_jar_mainclass).
            let jar_bytes = std::fs::read(&jar_path).map_err(|err| {
                AppError::new(
                    codes::LOADER_INSTALL_FAILED,
                    format!("processor jar {}: {err}", jar_path.display()),
                )
            })?;
            let mainclass = jar_mainclass(&jar_bytes).ok_or_else(|| {
                AppError::new(
                    codes::LOADER_INSTALL_FAILED,
                    format!("processor {} thiếu Main-Class", processor.jar),
                )
            })?;

            // command — parity đúng thứ tự build + replace.
            let mut command: Vec<String> = vec![
                java.path.clone(),
                "-cp".into(),
                classpath,
                mainclass,
            ];
            for arg in &processor.args {
                let value = vars
                    .iter()
                    .find(|(k, _)| k == arg)
                    .map(|(_, v)| v.clone())
                    .unwrap_or_else(|| arg.clone());
                if value.starts_with('[') && value.ends_with(']') {
                    command.push(
                        library_path(&self.game_dir, &value[1..value.len() - 1])
                            .to_string_lossy()
                            .into_owned(),
                    );
                } else {
                    command.push(value);
                }
            }
            for (key, value) in &vars {
                for element in &mut command {
                    *element = element.replace(key.as_str(), value.as_str());
                }
            }

            run_processor(&command).map_err(|err| {
                AppError::new(
                    codes::LOADER_INSTALL_FAILED,
                    format!("processor {} thất bại: {err}", processor.jar),
                )
            })?;
        }
        progress("Installation complete", 1.0);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Free functions — metadata list + entry points
// ---------------------------------------------------------------------------

/// Parity `forge.forge_to_installed_version` (free fn tiện gọi ngoài).
pub fn forge_to_installed_version(forge_version: &str) -> String {
    Installer::forge_to_installed_version(forge_version)
}

/// Parity `forge.supports_automatic_install`: `split("-")` đúng 2 phần
/// (nhiều dash → ValueError → false) + minor MC ≥ 13 (parse fail → false).
pub fn supports_automatic_install(forge_version: &str) -> bool {
    if forge_version.matches('-').count() != 1 {
        return false;
    }
    let Some((vanilla, _forge)) = forge_version.split_once('-') else {
        return false;
    };
    let minor = vanilla.split('.').nth(1).and_then(|p| p.parse::<u32>().ok());
    matches!(minor, Some(n) if n >= 13)
}

/// `fabric-loader-{lv}-{mc}` → Some(`lv`); không khớp format → None.
fn fabric_loader_version(resolved: &str, mc_version: &str) -> Option<String> {
    let rest = resolved.strip_prefix("fabric-loader-")?;
    let suffix = format!("-{mc_version}");
    rest.strip_suffix(&suffix).map(str::to_string)
}

/// Parity `get_jar_mainclass` — đọc `META-INF/MANIFEST.MF` (hỗ trợ dòng
/// continuation bắt đầu bằng space) → `Main-Class`.
fn jar_mainclass(jar_bytes: &[u8]) -> Option<String> {
    let manifest = zip_read_entry(jar_bytes, "META-INF/MANIFEST.MF").ok()?;
    let text = String::from_utf8_lossy(&manifest);
    // Gộp continuation: dòng kế tiếp bắt đầu bằng ' ' nối vào dòng trước.
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        if let Some(prev) = lines.last_mut() {
            if let Some(rest) = line.strip_prefix(' ') {
                prev.push_str(rest);
                continue;
            }
        }
        lines.push(line.to_string());
    }
    lines.iter().find_map(|line| {
        line.strip_prefix("Main-Class:")
            .map(|value| value.trim().to_string())
    })
}

/// Chạy một processor: capture output để báo lỗi; exit != 0 → Err (parity
/// an toàn hơn MLL — subprocess.run không check exit code).
fn run_processor(command: &[String]) -> Result<(), String> {
    let Some((program, args)) = command.split_first() else {
        return Err("command rỗng".into());
    };
    let mut cmd = std::process::Command::new(program);
    cmd.args(args);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW — parity SUBPROCESS_STARTUPINFO (không mở console).
        cmd.creation_flags(0x0800_0000);
    }
    let output = cmd.output().map_err(|err| format!("spawn {program}: {err}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let tail = if stderr.trim().is_empty() { stdout } else { stderr };
        return Err(format!(
            "exit {:?}: {}",
            output.status.code(),
            tail.lines().rev().take(5).collect::<Vec<_>>().join(" | ")
        ));
    }
    Ok(())
}

/// Extract một trong các entry (thứ tự fallback) — parity 3 try/KeyError của
/// MLL cho universal jar. Không entry nào có → Ok(()) (parity `except: pass`).
fn extract_first(zip: &[u8], candidates: &[(String, PathBuf)]) -> AppResult<()> {
    for (entry, dest) in candidates {
        let Ok(bytes) = zip_read_entry(zip, entry) else {
            continue;
        };
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|err| {
                AppError::new(codes::STORAGE_WRITE_FAILED, format!("mkdir: {err}"))
            })?;
        }
        std::fs::write(dest, bytes).map_err(|err| {
            AppError::new(codes::STORAGE_WRITE_FAILED, format!("write {}: {err}", dest.display()))
        })?;
        return Ok(());
    }
    Ok(())
}

/// Temp dir unique cho forge install (installer + {ROOT} processors).
fn temp_dir_for(prefix: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("{prefix}-{}-{nanos}", std::process::id()))
}

// ---------------------------------------------------------------------------
// Forge profile types — parity _internal_types.forge_types.ForgeInstallProfile
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ForgeProfile {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub minecraft: Option<String>,
    /// Spec cũ (install v0): `{version, minecraft}` dưới key `install`.
    #[serde(default)]
    pub install: Option<ForgeProfileLegacy>,
    #[serde(default)]
    pub libraries: Vec<Library>,
    #[serde(default)]
    pub processors: Vec<ForgeProcessor>,
    #[serde(default)]
    pub data: std::collections::BTreeMap<String, ForgeDataEntry>,
    /// Spec cũ — version JSON nằm ngay trong profile.
    #[serde(default, rename = "versionInfo")]
    pub version_info: Option<serde_json::Value>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ForgeProfileLegacy {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub minecraft: Option<String>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ForgeProcessor {
    pub jar: String,
    #[serde(default)]
    pub classpath: Vec<String>,
    #[serde(default)]
    pub args: Vec<String>,
    /// vắng mặt = cả 2 side (parity `i.get("sides", ["client"])`).
    #[serde(default)]
    pub sides: Option<Vec<String>>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ForgeDataEntry {
    #[serde(default)]
    pub client: Option<String>,
    #[serde(default)]
    pub server: Option<String>,
}

impl ForgeProfile {
    pub fn minecraft_version(&self) -> Option<String> {
        self.minecraft
            .clone()
            .or_else(|| self.install.as_ref().and_then(|i| i.minecraft.clone()))
    }

    pub fn minecraft_version_or_empty(&self) -> String {
        self.minecraft_version().unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// list versions — parity FabricProvider/ForgeProvider.list_versions
// ---------------------------------------------------------------------------

/// Parity `versions.list` cho loader (khối vanilla vẫn đi Mojang manifest ở
/// command layer):
/// - `fabric` → stable MC versions, sort desc string (parity
///   `sorted(..., reverse=True)`)
/// - `forge` → maven `<versions>`, sort desc string (parity
///   `sorted(list_forge_versions(), reverse=True)`)
/// - khác → `VALIDATION_FAILED Unknown loader` (parity `get_loader`)
pub fn list_loader_versions(loader: &str, endpoints: &Endpoints) -> AppResult<Vec<String>> {
    match loader {
        "fabric" => {
            let url = format!("{}/v2/versions/game", endpoints.fabric_meta);
            let body = http_get(&url)?;
            let value: serde_json::Value =
                serde_json::from_slice(&body).map_err(|err| {
                    AppError::new(codes::NETWORK_UNAVAILABLE, format!("fabric game list: {err}"))
                })?;
            let mut versions: Vec<String> = value
                .as_array()
                .map(|entries| {
                    entries
                        .iter()
                        .filter(|e| e.get("stable").and_then(|s| s.as_bool()) == Some(true))
                        .filter_map(|e| e.get("version").and_then(|v| v.as_str()))
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default();
            versions.sort();
            versions.reverse();
            Ok(versions)
        }
        "forge" => {
            let url = format!(
                "{}/net/minecraftforge/forge/maven-metadata.xml",
                endpoints.forge_maven
            );
            let body = http_get(&url)?;
            let xml = String::from_utf8_lossy(&body);
            let mut versions = parse_maven_versions(&xml);
            versions.sort();
            versions.reverse();
            Ok(versions)
        }
        other => Err(AppError::new(
            codes::VALIDATION_FAILED,
            format!("Unknown loader: {other}"),
        )),
    }
}

/// Parity `_helper.parse_maven_metadata` regex `<version>(.*?)</version>`.
fn parse_maven_versions(xml: &str) -> Vec<String> {
    let mut versions = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<version>") {
        let after = &rest[start + "<version>".len()..];
        let Some(end) = after.find("</version>") else {
            break;
        };
        versions.push(after[..end].to_string());
        rest = &after[end + "</version>".len()..];
    }
    versions
}

// ---------------------------------------------------------------------------
// Entry points — play_install (async task) + install_instance (sync, chung
// với auto-install trong play_launch)
// ---------------------------------------------------------------------------

/// Kết quả `play_install` — contract `{taskId}` (UI poll task như launch).
#[derive(Debug, Clone)]
pub struct InstallOutcome {
    pub task_id: String,
}

/// Resolve + ensure đồng bộ — dùng bởi `play_install` (trong thread) và
/// `play_launch` (auto-install khi Play, parity orchestrator step 3).
/// Trả về `resolved` id. `task_id` = None → không báo tiến độ.
pub fn install_instance(
    tasks: &TaskRegistry,
    services: &AppServices,
    instance: &Instance,
    task_id: Option<&str>,
) -> AppResult<String> {
    let game_dir = PathBuf::from(&instance.directory).join("game");
    let cache_dir = services.storage().root().join("cache");
    let installer = Installer::new(&game_dir, &cache_dir, Endpoints::default());
    let progress = |status: &str, fraction: f64| {
        if let Some(task_id) = task_id {
            let _ = tasks.set_progress(task_id, fraction, Some(status.to_string()));
        }
    };
    let resolved = installer.resolve(instance)?;
    installer.ensure_installed(
        &instance.loader,
        &instance.minecraft_version,
        &resolved,
        &progress,
    )?;
    Ok(resolved)
}

/// parity `play.install` (native-only — legacy không có lệnh này): validate
/// instance → lock → tạo task `INSTALL` → **trả `{taskId}` ngay**, chạy nền
/// (tải hàng trăm MB không được block invoke). Lỗi trong thread → task failed
/// (UI thấy message); lỗi validation trả typed §117 ngay.
pub fn play_install(
    tasks: &Arc<TaskRegistry>,
    services: &Arc<AppServices>,
    instance_id: &str,
) -> AppResult<InstallOutcome> {
    let instance = services.instances().get(instance_id).ok_or_else(|| {
        AppError::new(
            codes::INSTANCE_NOT_FOUND,
            format!("Instance '{instance_id}' không tồn tại"),
        )
    })?;

    // Lock đồng bộ trước khi spawn — INSTANCE_LOCKED typed như launch.
    let instance_root = PathBuf::from(&instance.directory);
    let mut lock = ResourceLock::new(instance_root.join(".lock"));
    if !lock.acquire() {
        return Err(AppError::new(
            codes::INSTANCE_LOCKED,
            "Instance is already running or installing.",
        ));
    }

    let task = tasks.spawn(
        "INSTALL",
        TaskPriority::P1UserAction,
        Some(format!("install:{instance_id}")),
    )?;
    let task_id = task.id.clone();
    if let Err(err) = tasks.start(&task_id) {
        return Err(AppError::internal(err.to_string()));
    }

    let tasks_bg = Arc::clone(tasks);
    let services_bg = Arc::clone(services);
    let task_id_bg = task_id.clone();
    std::thread::spawn(move || {
        let _lock = lock; // giữ suốt install — release khi xong/panic (Drop)
        let result = install_instance(&tasks_bg, &services_bg, &instance, Some(&task_id_bg));
        match result {
            Ok(resolved) => {
                let _ = tasks_bg.set_progress(&task_id_bg, 1.0, Some(format!("Installed {resolved}")));
                let _ = tasks_bg.complete(&task_id_bg);
            }
            Err(err) => {
                let _ = tasks_bg.fail(&task_id_bg, err.to_string());
            }
        }
    });

    Ok(InstallOutcome { task_id })
}

#[cfg(test)]
mod tests {
    use super::*;
    use antares_launch::resolve_version_json;
    use antares_mods::zip_read_entry_with_limit;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "antares-install-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn dead_endpoints() -> Endpoints {
        Endpoints {
            fabric_meta: "http://127.0.0.1:1".into(),
            forge_maven: "http://127.0.0.1:1".into(),
            libraries: "http://127.0.0.1:1".into(),
            resources: "http://127.0.0.1:1".into(),
            java_runtime: "http://127.0.0.1:1".into(),
        }
    }

    /// Installer cho test: retry 1 + không backoff (endpoint chết fail ngay,
    /// không chờ ~44s backoff của engine).
    fn installer(game_dir: &Path, cache_dir: &Path, endpoints: Endpoints) -> Installer {
        Installer::new(game_dir, cache_dir, endpoints).with_download_tuning(1, false)
    }

    fn instance(loader: &str, mc: &str, dir: &Path) -> Instance {
        Instance {
            id: "i1".into(),
            name: "n".into(),
            minecraft_version: mc.into(),
            loader: loader.into(),
            directory: dir.display().to_string(),
            memory: crate::instances::InstanceMemory { min_mb: 512, max_mb: 2048 },
            jvm_args: vec![],
            jvm_preset: "auto".into(),
            created_at: None,
            last_played_at: None,
            launch_count: 0,
        }
    }

    /// Server HTTP tối giản cho test: map path → body (parity test downloads).
    fn serve(routes: Vec<(String, Vec<u8>)>) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let mut stream = stream;
                let mut reader = BufReader::new(stream.try_clone().unwrap());
                let mut line = String::new();
                let _ = reader.read_line(&mut line);
                loop {
                    let mut header = String::new();
                    if reader.read_line(&mut header).unwrap_or(0) == 0 || header.trim().is_empty() {
                        break;
                    }
                }
                let path = line.split_whitespace().nth(1).unwrap_or("/").to_string();
                let body = routes
                    .iter()
                    .find(|(p, _)| *p == path)
                    .map(|(_, b)| b.clone())
                    .unwrap_or_default();
                let head = if body.is_empty() {
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_string()
                } else {
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                };
                let _ = stream.write_all(head.as_bytes());
                if !body.is_empty() {
                    let _ = stream.write_all(&body);
                }
                let _ = stream.flush();
            }
        });
        (format!("http://{addr}"), handle)
    }

    // ---- resolve parity ----

    #[test]
    fn resolve_vanilla_is_identity_and_forge_derives_id() {
        let root = temp_root("resolve");
        let installer = installer(&root.join("game"), &root.join("cache"), dead_endpoints());
        assert_eq!(
            installer.resolve(&instance("vanilla", "1.21.11", &root)).unwrap(),
            "1.21.11"
        );
        // parity ForgeProvider.resolve_launch_version — không cần network.
        assert_eq!(
            installer.resolve(&instance("forge", "1.20.1-47.2.0", &root)).unwrap(),
            "1.20.1-forge-47.2.0"
        );
        assert_eq!(forge_to_installed_version("1.20.1-47.2.0"), "1.20.1-forge-47.2.0");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_unknown_loader_is_validation_failed() {
        let root = temp_root("resolve-unknown");
        let installer = installer(&root.join("game"), &root.join("cache"), dead_endpoints());
        let err = installer
            .resolve(&instance("quilt", "1.21.11", &root))
            .expect_err("quilt ngoài scope 07c");
        assert_eq!(err.code, codes::VALIDATION_FAILED);
        assert!(err.message.contains("Unknown loader: quilt"), "{}", err.message);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_fabric_offline_grace_uses_installed_version() {
        let root = temp_root("resolve-fabric-offline");
        let game_dir = root.join("game");
        std::fs::create_dir_all(game_dir.join("versions/fabric-loader-0.16.14-1.21.11")).unwrap();
        std::fs::create_dir_all(game_dir.join("versions/fabric-loader-0.15.0-1.21.11")).unwrap();
        let installer = installer(&game_dir, &root.join("cache"), dead_endpoints());
        // meta chết → quét versions/ đã cài, lấy id_sort lớn nhất (parity format).
        assert_eq!(
            installer.resolve(&instance("fabric", "1.21.11", &root)).unwrap(),
            "fabric-loader-0.16.14-1.21.11"
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn resolve_fabric_without_meta_and_installed_is_network_unavailable() {
        let root = temp_root("resolve-fabric-net");
        let installer = installer(&root.join("game"), &root.join("cache"), dead_endpoints());
        let err = installer
            .resolve(&instance("fabric", "1.21.11", &root))
            .expect_err("offline + chưa cài");
        assert_eq!(err.code, codes::NETWORK_UNAVAILABLE);
        let _ = std::fs::remove_dir_all(&root);
    }

    // ---- install_version parity ----

    #[test]
    fn install_version_missing_from_manifest_is_version_not_found() {
        let root = temp_root("manifest-miss");
        // Seed manifest cache FRESH (không network) nhưng không có id cần tìm.
        let cache = ManifestCache::new(&root.join("cache"));
        cache.put(
            "mojang_manifest",
            &vec![antares_net::manifest::VersionEntry {
                id: "1.20.4".into(),
                r#type: "release".into(),
                release_time: "2023".into(),
                url: "".into(),
                sha1: "".into(),
            }],
        );
        let installer = installer(&root.join("game"), &root.join("cache"), dead_endpoints());
        let err = installer
            .install_version("9.9.9", &no_progress)
            .expect_err("không có trong manifest");
        assert_eq!(err.code, codes::MINECRAFT_VERSION_NOT_FOUND);
        assert!(err.message.contains("9.9.9"), "{}", err.message);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn install_version_repairs_from_disk_without_manifest() {
        let root = temp_root("repair");
        let game_dir = root.join("game");
        let dir = game_dir.join("versions/1.8.9");
        std::fs::create_dir_all(&dir).unwrap();
        // JSON kiểu cũ (minecraftArguments) — không assetIndex/downloads →
        // install = libraries rỗng, không download (offline-deterministic).
        std::fs::write(
            dir.join("1.8.9.json"),
            r#"{"id":"1.8.9","type":"release","mainClass":"net.minecraft.client.main.Main",
                "minecraftArguments":"--username ${auth_player_name}","libraries":[]}"#,
        )
        .unwrap();
        let installer = installer(&game_dir, &root.join("cache"), dead_endpoints());
        installer.install_version("1.8.9", &no_progress).expect("repair từ đĩa");
        let _ = std::fs::remove_dir_all(&root);
    }

    // ---- fabric end-to-end (local server) ----

    #[test]
    fn fabric_install_writes_profile_and_merges_via_local_meta() {
        let root = temp_root("fabric-e2e");
        let game_dir = root.join("game");
        // Vanilla base đã có JSON tối thiểu (không assetIndex/downloads →
        // không chạm mạng ngoài — server test chỉ serve profile).
        let vanilla_dir = game_dir.join("versions/1.21.11");
        std::fs::create_dir_all(&vanilla_dir).unwrap();
        std::fs::write(
            vanilla_dir.join("1.21.11.json"),
            r#"{"id":"1.21.11","type":"release","mainClass":"net.minecraft.client.main.Main",
                "arguments":{"game":["--username","${auth_player_name}"],
                             "jvm":["-Djava.library.path=${natives_directory}","-cp","${classpath}"]},
                "libraries":[{"name":"com.mojang:brigadier:1.3.13"}]}"#,
        )
        .unwrap();

        let profile = serde_json::json!({
            "id": "fabric-loader-0.16.14-1.21.11",
            "inheritsFrom": "1.21.11",
            "type": "release",
            "jar": serde_json::Value::Null,
            "mainClass": "net.fabricmc.loader.impl.launch.knot.KnotClient",
            "arguments": { "game": [], "jvm": [] },
            "libraries": [
                { "name": "net.fabricmc:fabric-loader:0.16.14",
                  "url": "http://127.0.0.1:1/" }
            ]
        });
        let (base, server) = serve(vec![(
            "/v2/versions/loader/1.21.11/0.16.14/profile/json".into(),
            serde_json::to_vec(&profile).unwrap(),
        )]);
        let endpoints = Endpoints {
            fabric_meta: base,
            ..dead_endpoints()
        };
        let installer = installer(&game_dir, &root.join("cache"), endpoints);
        let resolved = "fabric-loader-0.16.14-1.21.11";
        installer
            .ensure_installed("fabric", "1.21.11", resolved, &no_progress)
            .expect("fabric install");

        // Profile ghi trên đĩa (JSON thô giữ inheritsFrom — parity installer).
        let raw = std::fs::read_to_string(game_dir.join(format!("versions/{resolved}/{resolved}.json"))).unwrap();
        assert!(raw.contains("inheritsFrom"));
        // Merge thành công = resolve đọc được (mainClass con + libs parent).
        let vj = resolve_version_json(&game_dir, resolved).expect("merged");
        assert_eq!(vj.main_class, "net.fabricmc.loader.impl.launch.knot.KnotClient");
        assert!(vj.libraries.len() > 1, "fabric lib + vanilla libs");
        // Fabric lib tải từ maven (optional, server chết → vẫn Ok parity swallow).
        drop(server);
        let _ = std::fs::remove_dir_all(&root);
    }

    // ---- artifact download qua local server ----

    #[test]
    fn install_downloads_artifact_with_sha1() {
        use antares_downloads::sha1_hex;

        let payload = b"library-bytes-v1".to_vec();
        let sha1 = sha1_hex(&payload);
        let (base, server) = serve(vec![(
            "/libs/com/example/demo/1.0/demo-1.0.jar".into(),
            payload.clone(),
        )]);
        let root = temp_root("artifact");
        let game_dir = root.join("game");
        let version_json = serde_json::json!({
            "id": "1.0.0",
            "type": "release",
            "mainClass": "x.Y",
            "minecraftArguments": "a b",
            "libraries": [
                { "name": "com.example:demo:1.0",
                  "downloads": { "artifact": {
                      "path": "com/example/demo/1.0/demo-1.0.jar",
                      "url": format!("{base}/libs/com/example/demo/1.0/demo-1.0.jar"),
                      "sha1": sha1 } } }
            ]
        });
        let dir = game_dir.join("versions/1.0.0");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("1.0.0.json"), serde_json::to_vec(&version_json).unwrap()).unwrap();

        let installer = installer(&game_dir, &root.join("cache"), dead_endpoints());
        installer.install_version("1.0.0", &no_progress).expect("install");
        let target = game_dir.join("libraries/com/example/demo/1.0/demo-1.0.jar");
        assert_eq!(std::fs::read(&target).unwrap(), payload);
        // Idempotent — lần 2 không lỗi (cache check trong download pipeline).
        installer.install_version("1.0.0", &no_progress).expect("repair lần 2");
        drop(server);
        let _ = std::fs::remove_dir_all(&root);
    }

    // ---- forge profile parity ----

    #[test]
    fn forge_profile_fixture_parses_with_processors_and_data() {
        let bytes = include_bytes!("../fixtures/forge/install_profile.json");
        let profile: ForgeProfile = serde_json::from_slice(bytes).expect("parse install_profile");
        assert_eq!(profile.version.as_deref(), Some("1.20.1-forge-47.2.0"));
        assert_eq!(profile.minecraft_version().as_deref(), Some("1.20.1"));
        // 10 processors — 6 client-side (3 không sides + jarsplitter client + FAT + binarypatcher).
        assert_eq!(profile.processors.len(), 10);
        let client = profile
            .processors
            .iter()
            .filter(|p| p.sides.as_deref().is_none_or(|s| s.iter().any(|x| x == "client")))
            .count();
        assert_eq!(client, 6);
        // data vars — BINPATCH là path dẫn xuất (client.lzma), bracket → maven.
        let binpatch = profile.data.get("BINPATCH").and_then(|d| d.client.as_deref());
        assert_eq!(binpatch, Some("/data/client.lzma"));
        let mcp = profile.data.get("MAPPINGS").and_then(|d| d.client.as_deref()).unwrap();
        assert!(mcp.starts_with('[') && mcp.ends_with(']'));
        assert!(!profile.libraries.is_empty());
    }

    #[test]
    fn forge_version_json_fixture_has_inheritance_and_bootstrap() {
        let bytes = include_bytes!("../fixtures/forge/version.json");
        let vj: VersionJson = serde_json::from_slice(bytes).expect("parse version.json");
        assert_eq!(vj.id, "1.20.1-forge-47.2.0");
        assert_eq!(vj.inherits_from.as_deref(), Some("1.20.1"));
        assert_eq!(vj.main_class, "cpw.mods.bootstraplauncher.BootstrapLauncher");
        assert_eq!(vj.libraries.len(), 29);
        // args jvm có placeholder ${library_directory}/${classpath_separator}
        // (command.rs đã thay — test render command nằm ở antares-launch).
        let jvm = vj.arguments.as_ref().unwrap().jvm.as_ref().unwrap();
        assert!(jvm.iter().any(|arg| match arg {
            antares_launch::mcjson::ArgEntry::Plain(s) => s.contains("${library_directory}"),
            antares_launch::mcjson::ArgEntry::Ruled(r) => match &r.value {
                antares_launch::mcjson::ArgValue::One(s) => s.contains("${library_directory}"),
                antares_launch::mcjson::ArgValue::Many(v) =>
                    v.iter().any(|x| x.contains("${library_directory}")),
            },
        }));
    }

    #[test]
    fn supports_automatic_install_matches_mll() {
        assert!(supports_automatic_install("1.20.1-47.2.0"));
        assert!(supports_automatic_install("1.13.2-25.0.223"));
        assert!(!supports_automatic_install("1.12.2-14.23.5.2860"), "minor 12 < 13");
        assert!(!supports_automatic_install("no-dash-here"));
        assert!(!supports_automatic_install("1.20.1-47.2.0-extra"), "nhiều dash → false");
    }

    #[test]
    fn forge_old_version_fails_with_loader_install_failed() {
        let root = temp_root("forge-old");
        let installer = installer(&root.join("game"), &root.join("cache"), dead_endpoints());
        let err = installer
            .ensure_installed("forge", "1.12.2-14.23.5.2860", "1.12.2-forge-14.23.5.2860", &no_progress)
            .expect_err("không tự động được");
        assert_eq!(err.code, codes::LOADER_INSTALL_FAILED);
        assert!(err.retryable, "parity legacy retry ×3 quanh LOADER_INSTALL_FAILED");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn forge_installer_download_failure_is_loader_install_failed() {
        let root = temp_root("forge-dl");
        let installer = installer(&root.join("game"), &root.join("cache"), dead_endpoints());
        let err = installer
            .ensure_installed("forge", "1.20.1-47.2.0", "1.20.1-forge-47.2.0", &no_progress)
            .expect_err("installer không tải được");
        // parity: MLL VersionNotFound → legacy ForgeProvider wrap LOADER_INSTALL_FAILED.
        assert_eq!(err.code, codes::LOADER_INSTALL_FAILED);
        assert!(err.message.contains("installer"), "{}", err.message);
        let _ = std::fs::remove_dir_all(&root);
    }

    // ---- list versions ----

    #[test]
    fn fabric_list_is_stable_mc_sorted_desc() {
        let body = serde_json::json!([
            { "version": "1.21.11", "stable": true },
            { "version": "25w31a", "stable": false },
            { "version": "1.20.1", "stable": true }
        ]);
        let (base, server) = serve(vec![("/v2/versions/game".into(), body.to_string().into_bytes())]);
        let endpoints = Endpoints { fabric_meta: base, ..dead_endpoints() };
        let versions = list_loader_versions("fabric", &endpoints).expect("list");
        assert_eq!(versions, vec!["1.21.11".to_string(), "1.20.1".to_string()]);
        drop(server);
    }

    #[test]
    fn forge_list_parses_maven_metadata_sorted_desc() {
        let xml = r#"<?xml version="1.0"?>
<metadata>
  <groupId>net.minecraftforge</groupId>
  <artifactId>forge</artifactId>
  <versioning>
    <latest>1.20.1-47.2.0</latest>
    <release>1.20.1-47.2.0</release>
    <versions>
      <version>1.20.1-47.2.0</version>
      <version>1.19.2-43.2.0</version>
      <version>1.12.2-14.23.5.2860</version>
    </versions>
  </versioning>
</metadata>"#;
        let (base, server) = serve(vec![(
            "/net/minecraftforge/forge/maven-metadata.xml".into(),
            xml.as_bytes().to_vec(),
        )]);
        let endpoints = Endpoints { forge_maven: base, ..dead_endpoints() };
        let versions = list_loader_versions("forge", &endpoints).expect("list");
        // parity sorted(..., reverse=True) — sort STRING (1.19 > 1.20 theo string).
        assert_eq!(versions[0], "1.20.1-47.2.0");
        assert_eq!(versions.len(), 3);
        assert!(versions.contains(&"1.12.2-14.23.5.2860".to_string()));
        drop(server);
    }

    #[test]
    fn list_loader_versions_unknown_is_validation_failed() {
        let err = list_loader_versions("neoforge", &dead_endpoints()).expect_err("ngoài 07c");
        assert_eq!(err.code, codes::VALIDATION_FAILED);
        assert!(err.message.contains("Unknown loader: neoforge"));
    }

    // ---- dedup job trùng target (lib trùng trong version JSON) ----

    #[test]
    fn dedup_jobs_merges_duplicate_targets_preferring_required_sha1() {
        let target = PathBuf::from("libraries/a/b.jar");
        let jobs = vec![
            FileJob {
                url: "http://x/a".into(),
                target: target.clone(),
                sha1: None,
                optional: true,
            },
            FileJob {
                url: "http://x/b".into(),
                target: target.clone(),
                sha1: Some("abc".into()),
                optional: false,
            },
            FileJob {
                url: "http://x/other".into(),
                target: PathBuf::from("libraries/c.jar"),
                sha1: None,
                optional: true,
            },
        ];
        let out = dedup_jobs(jobs);
        assert_eq!(out.len(), 2, "trùng target → gộp");
        // bản bắt buộc + sha1 thắng bản optional.
        assert_eq!(out[0].url, "http://x/b");
        assert!(!out[0].optional);
        assert_eq!(out[1].target, PathBuf::from("libraries/c.jar"));
    }

    // ---- jar_mainclass / helpers ----

    #[test]
    fn jar_mainclass_reads_manifest_with_continuation() {
        // Zip stored thủ công: 1 entry META-INF/MANIFEST.MF (continuation line).
        let manifest = b"Manifest-Version: 1.0\r\nMain-Class: net.minecraftforge.installer.Main\r\n\r\n";
        let jar = build_stored_zip(&[("META-INF/MANIFEST.MF", manifest)]);
        assert_eq!(
            jar_mainclass(&jar).as_deref(),
            Some("net.minecraftforge.installer.Main")
        );
    }

    #[test]
    fn jar_mainclass_wrapped_line_is_joined() {
        // Dòng Main-Class dài bị wrap (continuation spec = đúng 1 space đầu dòng).
        let manifest = b"Main-Class: com.example.VeryLongClassNameThatWraps\r\n .Suffix\r\n\r\n";
        let jar = build_stored_zip(&[("META-INF/MANIFEST.MF", manifest)]);
        assert_eq!(
            jar_mainclass(&jar).as_deref(),
            Some("com.example.VeryLongClassNameThatWraps.Suffix")
        );
    }

    #[test]
    fn maven_url_build_skips_classifier_part_parity() {
        // parity MLL: parts[3..] BỊ BỎ khi build URL download (quirk get_libraries khác).
        let (rel, url) =
            maven_rel_and_url("org.lwjgl.lwjgl:lwjgl-platform:2.9.4:natives-windows", "https://x").unwrap();
        assert_eq!(rel, "org/lwjgl/lwjgl/lwjgl-platform/2.9.4/lwjgl-platform-2.9.4.jar");
        assert_eq!(url, "https://x/org/lwjgl/lwjgl/lwjgl-platform/2.9.4/lwjgl-platform-2.9.4.jar");
        // @suffix trong version component.
        let (rel, _) = maven_rel_and_url("de.oceanlabs.mcp:mcp_config:1.20.1:mappings@txt", LIBRARIES_URL).unwrap();
        assert!(rel.ends_with("mcp_config-1.20.1.txt"), "{rel}");
    }

    /// Zip writer tối giản (stored, không nén) cho test — 1+ entries.
    fn build_stored_zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out: Vec<u8> = Vec::new();
        let mut central: Vec<u8> = Vec::new();
        let mut count: u16 = 0;
        for (name, data) in entries {
            let name_bytes = name.as_bytes();
            let offset = out.len() as u32;
            out.extend_from_slice(&[0x50, 0x4b, 0x03, 0x04]); // local header
            out.extend_from_slice(&20u16.to_le_bytes()); // version
            out.extend_from_slice(&0u16.to_le_bytes()); // flags
            out.extend_from_slice(&0u16.to_le_bytes()); // method stored
            out.extend_from_slice(&0u16.to_le_bytes()); // time
            out.extend_from_slice(&0u16.to_le_bytes()); // date
            out.extend_from_slice(&0u32.to_le_bytes()); // crc (0 = không check ở reader?)
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(data.len() as u32).to_le_bytes());
            out.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            out.extend_from_slice(&0u16.to_le_bytes()); // extra len
            out.extend_from_slice(name_bytes);
            out.extend_from_slice(data);

            central.extend_from_slice(&[0x50, 0x4b, 0x01, 0x02]); // central header
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(data.len() as u32).to_le_bytes());
            central.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes());
            central.extend_from_slice(&0u32.to_le_bytes());
            central.extend_from_slice(&(offset).to_le_bytes());
            central.extend_from_slice(name_bytes);
            count += 1;
        }
        let central_offset = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&[0x50, 0x4b, 0x05, 0x06]); // end of central dir
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&central_offset.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out
    }

    #[test]
    fn stored_zip_builder_roundtrips_with_zipread() {
        let jar = build_stored_zip(&[("a/b.txt", b"hello")]);
        assert_eq!(zip_read_entry(&jar, "a/b.txt").unwrap(), b"hello");
        let _ = zip_read_entry_with_limit(&jar, "a/b.txt", 1024).unwrap();
    }

    // ---- processor runner (java thật nếu máy có) ----

    #[test]
    fn run_processor_checks_exit_code_with_real_java() {
        // Máy không có JRE → skip (soft parity precedent — không flaky CI).
        // Dùng `exe` (binary) — KHÔNG phải `path` (java home = thư mục).
        let Some(java) = antares_java::scan_java_infos()
            .into_iter()
            .map(|info| info.exe)
            .next()
        else {
            log::warn!("skip run_processor test — không tìm thấy java");
            return;
        };
        assert!(std::path::Path::new(&java).is_file(), "exe phải là file: {java}");
        // exit 0 → Ok (parity subprocess.run không raise).
        run_processor(&[java.clone(), "-version".into()]).expect("java -version exit 0");
        // exit != 0 → Err — parity AN TOÀN HƠN MLL (bỏ qua exit code).
        let err = run_processor(&[
            java,
            "-cp".into(),
            ".".into(),
            "NoSuchMainClassXxx".into(),
        ])
        .expect_err("main class không tồn tại phải fail");
        assert!(err.contains("exit"), "{err}");
    }

    // ---- smoke THẬT với mạng (tay, không chạy trong CI) ----

    /// Cài vanilla 1.0 thật (13 libs + assetIndex `pre-1.6` ~50MB + client
    /// jar 2.3MB — version nhỏ nhất còn đủ shape hiện đại) rồi verify cấu
    /// trúc. Chạy tay:
    /// `cargo test -p antares-app real_network -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_network_install_smoke_vanilla_1_0() {
        let root = temp_root("smoke-real");
        let game_dir = root.join("game");
        let installer = Installer::new(&game_dir, &root.join("cache"), Endpoints::default());
        let progress = |status: &str, fraction: f64| {
            println!("[install 1.0] {fraction:.2} {status}");
        };
        installer
            .install_version("1.0", &progress)
            .expect("cài 1.0 với mạng thật");

        assert!(installer.is_installed("1.0"), "versions/1.0/1.0.json");
        assert!(game_dir.join("versions/1.0/1.0.jar").is_file(), "client jar");
        assert!(
            game_dir.join("assets/indexes/pre-1.6.json").is_file(),
            "asset index"
        );
        // libraries đã tải — 13 entry nhưng 3 entry osx-only bị rules loại +
        // 1 entry trùng (jinput-platform) → 9 entry linux + 2 maven-native = 11.
        let jar_count = count_files_with_ext(&game_dir.join("libraries"), "jar");
        assert!(jar_count >= 10, "ít nhất 10 jar, thấy {jar_count}");
        // natives extract parity — `.so` (linux) / `.dll` (windows) trong natives/.
        let natives_dir = game_dir.join("versions/1.0/natives");
        let native_files = std::fs::read_dir(&natives_dir)
            .map(|entries| entries.flatten().count())
            .unwrap_or(0);
        assert!(native_files >= 3, "natives đã extract, thấy {native_files}");
        let asset_count = count_files_with_ext(&game_dir.join("assets/objects"), "*");
        assert!(asset_count > 100, "assets objects > 100, thấy {asset_count}");

        // Repair idempotent — lần 2 không lỗi (đã có: skip qua cache check).
        installer.install_version("1.0", &no_progress).expect("repair");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Smoke THẬT fabric: profile JSON từ meta.fabricmc.net thật + lib tải
    /// từ maven.fabricmc.net thật (base vanilla minimal sẵn trên đĩa → không
    /// tải assets/client jar hàng trăm MB). Chạy tay:
    /// `cargo test -p antares-app real_network_fabric -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_network_fabric_profile_smoke() {
        let root = temp_root("smoke-fabric");
        let game_dir = root.join("game");
        let vanilla_dir = game_dir.join("versions/1.21.11");
        std::fs::create_dir_all(&vanilla_dir).unwrap();
        std::fs::write(
            vanilla_dir.join("1.21.11.json"),
            r#"{"id":"1.21.11","type":"release","mainClass":"net.minecraft.client.main.Main",
                "arguments":{"game":["--username","${auth_player_name}"],
                             "jvm":["-Djava.library.path=${natives_directory}","-cp","${classpath}"]},
                "libraries":[]}"#,
        )
        .unwrap();

        let installer = Installer::new(&game_dir, &root.join("cache"), Endpoints::default());
        // resolve THẬT từ meta (latest loader) — parity FabricProvider.
        let instance = instance("fabric", "1.21.11", &root);
        let resolved = installer.resolve(&instance).expect("resolve fabric từ meta thật");
        assert!(resolved.starts_with("fabric-loader-"), "{resolved}");

        installer
            .ensure_installed("fabric", "1.21.11", &resolved, &|status, fraction| {
                println!("[fabric {fraction:.2}] {status}");
            })
            .expect("cài fabric với meta/maven thật");

        // Profile thật đã ghi + merge được (jar: null của meta → fallback id).
        let vj = resolve_version_json(&game_dir, &resolved).expect("merge profile thật");
        assert_eq!(vj.main_class, "net.fabricmc.loader.impl.launch.knot.KnotClient");
        assert!(
            vj.libraries.iter().any(|l| l.name.starts_with("net.fabricmc:fabric-loader:")),
            "fabric-loader có trong merged libraries"
        );
        // Lib fabric đã tải thật từ maven.fabricmc.net.
        let loader_jar = count_files_with_ext(&game_dir.join("libraries/net/fabricmc"), "jar");
        assert!(loader_jar >= 1, "ít nhất 1 jar fabric, thấy {loader_jar}");
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Smoke THẬT forge — nặng (~600MB vanilla 1.20.1 + assets + 6 processor
    /// java): cài `1.20.1-47.2.0` đầy đủ parity `install_forge_version` rồi
    /// verify universal jar + version json merge (processors exit 0 = install
    /// Ok vì runner fail khi exit != 0). Chạy tay:
    /// `cargo test -p antares-app real_network_forge -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_network_forge_install_smoke() {
        let root = temp_root("smoke-forge");
        let game_dir = root.join("game");
        let installer = Installer::new(&game_dir, &root.join("cache"), Endpoints::default());
        let instance = instance("forge", "1.20.1-47.2.0", &root);
        let resolved = installer.resolve(&instance).expect("resolve forge");
        assert_eq!(resolved, "1.20.1-forge-47.2.0");

        installer
            .ensure_installed("forge", "1.20.1-47.2.0", &resolved, &|status, fraction| {
                println!("[forge {fraction:.2}] {status}");
            })
            .expect("cài forge thật (bao gồm processors)");

        assert!(installer.is_installed(&resolved), "version json forge");
        assert!(
            game_dir.join("versions/1.20.1/1.20.1.jar").is_file(),
            "vanilla client jar"
        );
        assert!(
            game_dir
                .join("libraries/net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-universal.jar")
                .is_file(),
            "universal jar"
        );
        let vj = resolve_version_json(&game_dir, &resolved).expect("merge forge version json");
        assert_eq!(vj.main_class, "cpw.mods.bootstraplauncher.BootstrapLauncher");
        assert_eq!(vj.inherits_from.as_deref(), Some("1.20.1"));
        let _ = std::fs::remove_dir_all(&root);
    }

    fn count_files_with_ext(dir: &Path, ext: &str) -> usize {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return 0;
        };
        entries
            .flatten()
            .map(|entry| {
                let path = entry.path();
                if path.is_dir() {
                    count_files_with_ext(&path, ext)
                } else if ext == "*"
                    || path.extension().is_some_and(|e| e == ext)
                {
                    1
                } else {
                    0
                }
            })
            .sum()
    }
}
