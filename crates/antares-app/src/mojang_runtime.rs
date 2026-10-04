//! Batch 07d — **Mojang JRE runtime** parity `minecraft_launcher_lib.runtime`
//! + legacy `services/java/manager.py` (`mojang_runtime_executable` /
//! `install_mojang_runtime`) — tải JRE cùng version khi hệ thống thiếu Java
//! đủ mới (parity orchestrator `_resolve_java` bước 2–3).
//!
//! Parity map:
//!
//! | MLL / legacy | native |
//! |---|---|
//! | `runtime._get_jvm_platform_string` | [`jvm_platform_string`] |
//! | `runtime.get_executable_path` | [`get_executable_path`] |
//! | `runtime.install_jvm_runtime` | [`install_jvm_runtime`] |
//! | legacy `JavaManager.mojang_runtime_executable` | [`get_executable_path`] (component lấy từ `VersionJson.javaVersion`) |
//! | legacy `JavaManager.install_mojang_runtime` | [`install_mojang_runtime`] |
//!
//! Khác có chủ đích (đều ghi rõ parity điểm dừng):
//! - **lzma** — parity MLL: ưu tiên `downloads.lzma` (tải ~37× ít bytes hơn
//!   raw với java-runtime-delta: 2.9MB vs 109MB), sha1 verify **container**
//!   rồi decompress + verify sha1 **bản RAW** (MLL truyền `sha1=raw.sha1` +
//!   `lzma_compressed=True` — decompress trong `download_file`). Manifest
//!   không có lzma → tải thẳng raw (parity nhánh else).
//! - Thứ tự xử lý tuần tự (dirs → files → links → chmod → metadata) thay vì
//!   MLL ThreadPoolExecutor lẫn lộn — kết quả cuối giống hệt, tránh race
//!   tạo thư mục/symlink.
//! - `chmod +x` (unix) parity `subprocess.run(["chmod","+x"])` — lỗi → warn
//!   (MLL không check exit).
//! - `.version` / `{component}.sha1` — MLL chỉ **ghi** (không đọc ở đâu) →
//!   ghi parity đúng format `{rel} /#// {sha1} {ctime_ns}`; `st_ctime_ns` lấy
//!   từ `stat.st_ctime` (unix) — std không expose → Windows dùng created-time
//!   (file không ai đọc → lệch vô hại, có ghi chú).
//! - `check_path_inside_minecraft_directory` của MLL so **string-prefix**
//!   (bypass được `/game` vs `/game_evil`) → ta check theo component (§50,
//!   nhất quán với `extract_natives_jar`).
//! - Lỗi (component không có / platform không hỗ trợ / manifest hỏng) →
//!   legacy nuốt thành `None` → `JAVA_NOT_FOUND` — wrapper
//!   [`install_mojang_runtime`] tái hiện đúng.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use serde_json::Value;

use crate::error::{codes, AppError, AppResult};
use crate::install::{self, Endpoints, FileJob, Progress};

/// Progress slice của Java runtime trong task LAUNCH — download runtime chạy
/// SAU khi install version xong (0..1) → giữ trong [0.9, 0.99] để không báo
/// 100% rồi lùi; complete task vẫn set 1.0.
const JAVA_PROGRESS_BASE: f64 = 0.9;
const JAVA_PROGRESS_SPAN: f64 = 0.09;

// ---------------------------------------------------------------------------
// Platform + executable path — parity MLL runtime.py
// ---------------------------------------------------------------------------

/// Parity MLL `_get_jvm_platform_string`:
/// Windows → `windows-x64`/`windows-x86` (độ rộng process), Linux → `linux`
/// (`linux-i386` 32-bit), macOS → `mac-os-arm64`/`mac-os`, khác → `gamecore`.
pub fn jvm_platform_string() -> &'static str {
    if cfg!(windows) {
        if cfg!(target_pointer_width = "32") {
            "windows-x86"
        } else {
            "windows-x64"
        }
    } else if cfg!(target_os = "linux") {
        if cfg!(target_pointer_width = "32") {
            "linux-i386"
        } else {
            "linux"
        }
    } else if cfg!(target_os = "macos") {
        if cfg!(target_arch = "aarch64") {
            "mac-os-arm64"
        } else {
            "mac-os"
        }
    } else {
        "gamecore"
    }
}

/// Parity MLL `get_executable_path(jvm_version, minecraft_directory)` —
/// `runtime/{component}/{platform}/{component}/bin/java(.exe)` với mac fallback
/// `jre.bundle/Contents/Home/bin/java`. Không có → `None` (legacy → None).
pub fn get_executable_path(component: &str, game_dir: &Path) -> Option<PathBuf> {
    let base = game_dir
        .join("runtime")
        .join(component)
        .join(jvm_platform_string())
        .join(component);
    let java = base.join("bin").join("java");
    if java.is_file() {
        return Some(java);
    }
    let java_exe = base.join("bin").join("java.exe");
    if java_exe.is_file() {
        return Some(java_exe);
    }
    let bundle = base
        .join("jre.bundle")
        .join("Contents")
        .join("Home")
        .join("bin")
        .join("java");
    if bundle.is_file() {
        return Some(bundle);
    }
    None
}

// ---------------------------------------------------------------------------
// install_jvm_runtime — parity MLL runtime.install_jvm_runtime
// ---------------------------------------------------------------------------

/// Spec cho job lzma — tải container vào `part`, decompress → verify raw sha1
/// → ghi `target` (parity `download_file(lzma_compressed=True)`).
struct LzmaSpec {
    target: PathBuf,
    raw_sha1: String,
    /// URL bản raw — chỉ để message lỗi rõ (đã verify container qua job).
    url: String,
}

/// Cài Mojang runtime `component` vào `game_dir/runtime/...` — parity MLL
/// `install_jvm_runtime(jvm_version, minecraft_directory, ...)`.
///
/// Trả `Ok(Some(version_name))` khi cài xong, `Ok(None)` khi component không
/// có trong manifest (MLL `VersionNotFound` — legacy nuốt) hoặc platform không
/// hỗ trợ (MLL list rỗng → return lặng). Lỗi mạng/IO → `Err` (legacy nuốt ở
/// [`install_mojang_runtime`]).
pub fn install_jvm_runtime(
    component: &str,
    game_dir: &Path,
    endpoints: &Endpoints,
    tuning: (u32, bool),
    progress: Progress<'_>,
) -> AppResult<Option<String>> {
    // 1. all.json — parity requests.get(_JVM_MANIFEST_URL).
    let list_manifest = install::http_get_json(&endpoints.java_runtime)?;
    let platform = jvm_platform_string();
    let entries = list_manifest.get(platform).and_then(Value::as_object).ok_or_else(|| {
        AppError::new(
            codes::NETWORK_UNAVAILABLE,
            format!("java runtime all.json thiếu platform {platform}"),
        )
    })?;

    // 2. Component — MLL raise VersionNotFound → legacy warn + None.
    let Some(entry_list) = entries.get(component).and_then(Value::as_array) else {
        log::warn!("Mojang runtime {component} không có trong manifest ({platform})");
        return Ok(None);
    };
    // 3. Platform không hỗ trợ (list rỗng) — parity MLL return im lặng.
    let Some(entry) = entry_list.first().and_then(Value::as_object) else {
        return Ok(None);
    };
    let (manifest_url, version_name) = match (
        entry
            .get("manifest")
            .and_then(|m| m.get("url"))
            .and_then(Value::as_str),
        entry
            .get("version")
            .and_then(|v| v.get("name"))
            .and_then(Value::as_str),
    ) {
        (Some(url), Some(name)) => (url.to_string(), name.to_string()),
        _ => {
            log::warn!("Mojang runtime {component} entry thiếu manifest/version — bỏ qua");
            return Ok(None);
        }
    };

    // 4. Platform manifest — `{files: {relpath: {type, ...}}}`.
    let platform_manifest = install::http_get_json(&manifest_url)?;
    let files = platform_manifest
        .get("files")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            AppError::new(
                codes::NETWORK_UNAVAILABLE,
                format!("platform manifest thiếu files: {manifest_url}"),
            )
        })?;

    let base = game_dir
        .join("runtime")
        .join(component)
        .join(platform)
        .join(component);
    std::fs::create_dir_all(&base).map_err(|err| {
        AppError::new(
            codes::STORAGE_WRITE_FAILED,
            format!("mkdir runtime {}: {err}", base.display()),
        )
    })?;

    // 5. Phân loại entry — parity if/elif trong `install_runtime_file`
    //    (type lạ không có nhánh → bỏ qua im lặng).
    let mut jobs: Vec<FileJob> = Vec::new();
    let mut lzma_specs: HashMap<PathBuf, LzmaSpec> = HashMap::new();
    let mut links: Vec<(PathBuf, String)> = Vec::new();
    let mut executables: Vec<PathBuf> = Vec::new();
    // Thứ tự manifest (BTreeMap sort — MLL đẩy theo completion song song,
    // thứ tự file_list không ai đọc → deterministic hơn là giỏi).
    let mut file_rels: Vec<String> = Vec::new();

    for (rel, entry) in files {
        match entry.get("type").and_then(Value::as_str).unwrap_or("") {
            "directory" => {
                let dir = join_inside(&base, rel, game_dir)?;
                // parity: try/except pass — đã tồn tại / lỗi → bỏ qua.
                let _ = std::fs::create_dir_all(dir);
            }
            "file" => {
                let target = join_inside(&base, rel, game_dir)?;
                let downloads = entry.get("downloads");
                let raw_url = downloads
                    .and_then(|d| d.get("raw"))
                    .and_then(|r| r.get("url"))
                    .and_then(Value::as_str);
                let raw_sha1 = downloads
                    .and_then(|d| d.get("raw"))
                    .and_then(|r| r.get("sha1"))
                    .and_then(Value::as_str);
                let (Some(raw_url), Some(raw_sha1)) = (raw_url, raw_sha1) else {
                    // MLL KeyError → crash → legacy warn + None (trước khi tải).
                    return Err(AppError::new(
                        codes::APP_INTERNAL,
                        format!("java runtime entry thiếu downloads.raw: {rel}"),
                    ));
                };
                if entry.get("executable").and_then(Value::as_bool) == Some(true) {
                    executables.push(target.clone());
                }

                // Ưu tiên lzma (parity: `if "lzma" in downloads`) — sha1 job =
                // sha1 CONTAINER; sha1 RAW verify sau decompress.
                let lzma_url = downloads
                    .and_then(|d| d.get("lzma"))
                    .and_then(|l| l.get("url"))
                    .and_then(Value::as_str);
                let lzma_sha1 = downloads
                    .and_then(|d| d.get("lzma"))
                    .and_then(|l| l.get("sha1"))
                    .and_then(Value::as_str);
                if let (Some(lzma_url), Some(lzma_sha1)) = (lzma_url, lzma_sha1) {
                    let part = part_path(&target);
                    lzma_specs.insert(
                        part.clone(),
                        LzmaSpec {
                            target: target.clone(),
                            raw_sha1: raw_sha1.to_string(),
                            url: raw_url.to_string(),
                        },
                    );
                    jobs.push(FileJob {
                        url: lzma_url.to_string(),
                        target: part,
                        sha1: Some(lzma_sha1.to_string()),
                        optional: false,
                    });
                } else {
                    jobs.push(FileJob {
                        url: raw_url.to_string(),
                        target,
                        sha1: Some(raw_sha1.to_string()),
                        optional: false,
                    });
                }
                file_rels.push(rel.clone());
            }
            "link" => {
                let link = join_inside(&base, rel, game_dir)?;
                let Some(target) = entry.get("target").and_then(Value::as_str) else {
                    return Err(AppError::new(
                        codes::APP_INTERNAL,
                        format!("java runtime link thiếu target: {rel}"),
                    ));
                };
                // parity: check_path_inside(base/target) — validate trước,
                // symlink vẫn giữ CHUỖI target gốc (parity os.symlink).
                join_inside(&base, target, game_dir)?;
                links.push((link, target.to_string()));
            }
            _ => {} // type lạ — parity MLL không nhánh.
        }
    }

    // 6. Download song song — job lzma tải về part rồi decompress ngay trong
    //    fetch closure (đã có target hợp lệ → skip toàn bộ, parity cache check).
    let fetch = |job: &FileJob| -> AppResult<()> {
        if let Some(spec) = lzma_specs.get(&job.target) {
            if sha1_file_matches(&spec.target, &spec.raw_sha1) {
                return Ok(()); // đã cài bản raw đúng → không tải lại
            }
        }
        install::fetch_file(
            &job.url,
            &job.target,
            job.sha1.as_deref(),
            job.optional,
            tuning,
        )?;
        if let Some(spec) = lzma_specs.get(&job.target) {
            decompress_into(&job.target, spec)?;
            let _ = std::fs::remove_file(&job.target); // dọn part (MLL không có part)
        }
        Ok(())
    };
    install::run_jobs_with(
        &jobs,
        fetch,
        progress,
        "Download Java runtime",
        JAVA_PROGRESS_BASE,
        JAVA_PROGRESS_SPAN,
    )?;

    // 7. Links — parity os.makedirs(parent) + os.symlink lỗi → nuốt.
    create_links(&links);

    // 8. chmod +x — parity subprocess.run(["chmod","+x"]) (unix; Windows exe
    //    đã chạy được không cần).
    mark_executables(&executables);

    // 9. `.version` — parity ghi TÊN version (không newline) tại
    //    runtime/{component}/{platform}/.version.
    let parent = base.parent().ok_or_else(|| {
        AppError::internal("runtime base không có parent")
    })?;
    std::fs::write(parent.join(".version"), version_name.as_bytes()).map_err(|err| {
        AppError::new(
            codes::STORAGE_WRITE_FAILED,
            format!("ghi .version: {err}"),
        )
    })?;

    // 10. `{component}.sha1` — parity format `{rel} /#// {sha1} {ctime_ns}`
    //     (MLL chỉ ghi, không đọc).
    let mut sha1_buf = String::new();
    for rel in &file_rels {
        let path = join_inside(&base, rel, game_dir)?;
        let (sha1, ctime) = file_sha1_and_ctime(&path)?;
        sha1_buf.push_str(&format!("{rel} /#// {sha1} {ctime}\n"));
    }
    std::fs::write(parent.join(format!("{component}.sha1")), sha1_buf).map_err(|err| {
        AppError::new(
            codes::STORAGE_WRITE_FAILED,
            format!("ghi {component}.sha1: {err}"),
        )
    })?;

    progress("Java runtime installed", JAVA_PROGRESS_BASE + JAVA_PROGRESS_SPAN);
    Ok(Some(version_name))
}

/// Parity legacy `JavaManager.install_mojang_runtime` — MLL raise / manifest
/// vắng / lỗi mạng / decompress hỏng → **warn + None** (không raise), thành
/// công → probe `get_executable_path` (legacy check `Path(exe).is_file()`).
/// Orchestrator rơi vào `JAVA_NOT_FOUND + OPEN_JAVA_SETTINGS` khi None.
pub(crate) fn install_mojang_runtime(
    component: &str,
    game_dir: &Path,
    endpoints: &Endpoints,
    tuning: (u32, bool),
    progress: Progress<'_>,
) -> Option<PathBuf> {
    match install_jvm_runtime(component, game_dir, endpoints, tuning, progress) {
        Ok(_) => get_executable_path(component, game_dir),
        Err(err) => {
            log::warn!("Mojang runtime install failed: {err}");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Tên file part tải lzma (không lẫn file manifest — hiếm khi trùng; MLL ghi
/// thẳng nên không có part nào, ta dọn ngay sau decompress).
fn part_path(target: &Path) -> PathBuf {
    let name = target
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    target.with_file_name(format!("{name}.antares-lzma-part"))
}

/// Parity `check_path_inside_minecraft_directory(game_dir, base.join(rel))` —
/// lexical, **component-based** (§50: an toàn hơn string-prefix của MLL).
/// `..` hợp lệ trong phạm vi (link `../java.base/x`) — chỉ chặn vượt game_dir.
fn join_inside(base: &Path, rel: &str, game_dir: &Path) -> AppResult<PathBuf> {
    let escape = || {
        AppError::new(
            codes::VALIDATION_FAILED,
            format!("runtime path thoát game dir: {rel}"),
        )
    };
    let rel_path = Path::new(rel);
    if rel_path.is_absolute() {
        return Err(escape());
    }
    // Số bậc base dưới game_dir — popping vượt mức này là thoát ra ngoài.
    let base_depth = base
        .strip_prefix(game_dir)
        .map(|p| p.components().count() as i64)
        .map_err(|_| escape())?;
    let mut resolved = base.to_path_buf();
    let mut depth: i64 = 0;
    for comp in rel_path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                depth -= 1;
                if -depth > base_depth {
                    return Err(escape());
                }
                resolved.pop();
            }
            Component::Normal(part) => {
                depth += 1;
                resolved.push(part);
            }
            // RootDir / Prefix (Windows "C:") — đã bắt ở is_absolute, bắt lại cho chắc.
            _ => return Err(escape()),
        }
    }
    if !resolved.starts_with(game_dir) {
        return Err(escape());
    }
    Ok(resolved)
}

/// Decompress part lzma → verify sha1 RAW → ghi target (parity MLL
/// `lzma.decompress` + `get_sha1_hash == raw sha1` trước khi coi xong).
fn decompress_into(part: &Path, spec: &LzmaSpec) -> AppResult<()> {
    let compressed = std::fs::read(part).map_err(|err| {
        AppError::new(
            codes::STORAGE_WRITE_FAILED,
            format!("đọc part {}: {err}", part.display()),
        )
    })?;
    let mut input = compressed.as_slice();
    let mut out: Vec<u8> = Vec::new();
    lzma_rs::lzma_decompress(&mut input, &mut out).map_err(|err| {
        AppError::internal(format!("lzma decompress {}: {err}", spec.url))
    })?;
    let digest = antares_downloads::sha1_hex(&out);
    if !digest.eq_ignore_ascii_case(&spec.raw_sha1) {
        return Err(AppError::internal(format!(
            "SHA1 mismatch sau decompress {}: thấy {digest}, cần {}",
            spec.url, spec.raw_sha1
        )));
    }
    if let Some(parent) = spec.target.parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            AppError::new(
                codes::STORAGE_WRITE_FAILED,
                format!("mkdir {}: {err}", parent.display()),
            )
        })?;
    }
    std::fs::write(&spec.target, &out).map_err(|err| {
        AppError::new(
            codes::STORAGE_WRITE_FAILED,
            format!("ghi {}: {err}", spec.target.display()),
        )
    })?;
    Ok(())
}

/// File đã đúng sha1 chưa (dùng cho nhánh lzma: target hợp lệ → bỏ qua tải).
fn sha1_file_matches(path: &Path, expected: &str) -> bool {
    let Ok(bytes) = std::fs::read(path) else {
        return false;
    };
    antares_downloads::sha1_hex(&bytes).eq_ignore_ascii_case(expected)
}

/// sha1 (hex) + `st_ctime_ns` của file — parity vòng ghi `{component}.sha1`.
fn file_sha1_and_ctime(path: &Path) -> AppResult<(String, String)> {
    let bytes = std::fs::read(path).map_err(|err| {
        AppError::new(
            codes::STORAGE_WRITE_FAILED,
            format!("đọc {}: {err}", path.display()),
        )
    })?;
    let sha1 = antares_downloads::sha1_hex(&bytes);
    Ok((sha1, ctime_ns(path)))
}

/// `st_ctime_ns` (unix) — std không expose ctime → dùng MetadataExt.
/// Windows: created-time (st_ctime bất biến trên NTFS theo nghĩa thống kê —
/// file `.sha1` không ai đọc → lệch vô hại, có ghi chú parity ở đầu module).
fn ctime_ns(path: &Path) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let Ok(meta) = std::fs::metadata(path) {
            return ((meta.ctime() as i128) * 1_000_000_000 + meta.ctime_nsec() as i128)
                .to_string();
        }
    }
    let time = std::fs::metadata(path)
        .ok()
        .and_then(|meta| meta.created().or_else(|_| meta.modified()).ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i128)
        .unwrap_or(0);
    time.to_string()
}

/// Tạo symlink (unix/windows) — lỗi (đã có / thiếu quyền) → warn, parity
/// `try: os.symlink except: pass`.
fn create_links(links: &[(PathBuf, String)]) {
    for (path, target) in links {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if path.symlink_metadata().is_ok() {
            continue; // đã có — parity EEXIST bị nuốt
        }
        #[cfg(unix)]
        if let Err(err) = std::os::unix::fs::symlink(target, path) {
            log::warn!("symlink {} → {target} thất bại: {err}", path.display());
        }
        #[cfg(windows)]
        if let Err(err) = std::os::windows::fs::symlink_file(target, path) {
            log::warn!("symlink {} → {target} thất bại: {err}", path.display());
        }
    }
}

/// chmod +x (parity MLL; Windows không cần — `.exe` chạy trực tiếp).
fn mark_executables(executables: &[PathBuf]) {
    #[cfg(unix)]
    for path in executables {
        use std::os::unix::fs::PermissionsExt;
        let result = std::fs::metadata(path).and_then(|meta| {
            let mut mode = meta.permissions().mode();
            mode |= 0o111; // parity `chmod +x` (a+x)
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
        });
        if let Err(err) = result {
            log::warn!("chmod +x {} thất bại: {err}", path.display());
        }
    }
    #[cfg(not(unix))]
    let _ = executables;
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_http::serve_dynamic;
    use serde_json::json;

    fn temp_root(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "antares-mojang-rt-{tag}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    fn dead_endpoints() -> Endpoints {
        Endpoints {
            java_runtime: "http://127.0.0.1:1".into(),
            ..Endpoints::default()
        }
    }

    /// Fixture all.json + platform manifest + objects — file lzma, file
    /// raw-only, directory, link, type lạ. URL object nhúng base của server
    /// (serve_dynamic). Trả (endpoints, version_name, server).
    fn serve_runtime() -> (Endpoints, String, std::thread::JoinHandle<()>) {
        let raw_java = b"fake-java-binary-v1".to_vec();
        let helper = b"helper-bytes".to_vec();

        // Object lzma — nén bằng chính lzma-rs (cùng format MLL giải).
        let mut lzma_java: Vec<u8> = Vec::new();
        lzma_rs::lzma_compress(&mut raw_java.as_slice(), &mut lzma_java).expect("lzma nén");

        let mut all_map = serde_json::Map::new();
        all_map.insert(
            jvm_platform_string().to_string(),
            json!({
                "java-runtime-test": [{
                    "availability": { "group": "general", "rule": "allow" },
                    "manifest": { "sha1": "x", "size": 1, "url": "${BASE}/manifest.json" },
                    "version": { "name": "17.0.15-test", "released": "2024-01-01" }
                }],
                "java-runtime-empty": []
            }),
        );

        let manifest = json!({
            "files": {
                "bin": { "type": "directory" },
                "bin/java": {
                    "type": "file",
                    "executable": true,
                    "downloads": {
                        "raw": { "url": "${BASE}/obj/java",
                                  "sha1": antares_downloads::sha1_hex(&raw_java) },
                        "lzma": { "url": "${BASE}/obj/java.lzma",
                                  "sha1": antares_downloads::sha1_hex(&lzma_java) }
                    }
                },
                "lib/helper.txt": {
                    "type": "file",
                    "executable": false,
                    "downloads": {
                        "raw": { "url": "${BASE}/obj/helper",
                                  "sha1": antares_downloads::sha1_hex(&helper) }
                    }
                },
                "bin/java-link": { "type": "link", "target": "../bin/java" },
                "unknown-type": { "type": "future-thing" }
            }
        });

        let (base, server) = serve_dynamic(move |base| {
            let all = Value::Object(all_map).to_string().replace("${BASE}", base);
            let manifest = manifest.to_string().replace("${BASE}", base);
            vec![
                ("/all.json".into(), all.into_bytes()),
                ("/manifest.json".into(), manifest.into_bytes()),
                ("/obj/java".into(), raw_java),
                ("/obj/java.lzma".into(), lzma_java),
                ("/obj/helper".into(), helper),
            ]
        });
        let endpoints = Endpoints {
            java_runtime: format!("{base}/all.json"),
            ..Endpoints::default()
        };
        (endpoints, "17.0.15-test".into(), server)
    }

    #[test]
    fn platform_string_matches_mll_for_this_os() {
        let expected = if cfg!(windows) {
            if cfg!(target_pointer_width = "32") {
                "windows-x86"
            } else {
                "windows-x64"
            }
        } else if cfg!(target_os = "linux") {
            if cfg!(target_pointer_width = "32") {
                "linux-i386"
            } else {
                "linux"
            }
        } else if cfg!(target_os = "macos") {
            if cfg!(target_arch = "aarch64") {
                "mac-os-arm64"
            } else {
                "mac-os"
            }
        } else {
            "gamecore"
        };
        assert_eq!(jvm_platform_string(), expected);
    }

    #[test]
    fn executable_path_probes_bin_java_then_exe_then_bundle() {
        let root = temp_root("exe-probe");
        let game = root.join("game");
        // chưa cài → None
        assert!(get_executable_path("java-runtime-delta", &game).is_none());

        let base = game
            .join("runtime/java-runtime-delta")
            .join(jvm_platform_string())
            .join("java-runtime-delta");
        // mac bundle fallback (tạo tay — probe chỉ is_file, không chạy java).
        let bundle = base.join("jre.bundle/Contents/Home/bin/java");
        std::fs::create_dir_all(bundle.parent().unwrap()).unwrap();
        std::fs::write(&bundle, b"x").unwrap();
        assert_eq!(
            get_executable_path("java-runtime-delta", &game),
            Some(bundle.clone())
        );
        // bin/java.exe thắng bundle (thứ tự MLL).
        let exe = base.join("bin/java.exe");
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"x").unwrap();
        assert_eq!(get_executable_path("java-runtime-delta", &game), Some(exe));
        // bin/java thắng .exe.
        let java = base.join("bin/java");
        std::fs::write(&java, b"x").unwrap();
        assert_eq!(get_executable_path("java-runtime-delta", &game), Some(java));
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn install_full_manifest_downloads_lzma_raw_link_dirs_metadata() {
        let root = temp_root("install");
        let game = root.join("game");
        let (endpoints, expected_version, server) = serve_runtime();

        let version = install_jvm_runtime(
            "java-runtime-test",
            &game,
            &endpoints,
            (1, false),
            &install::no_progress,
        )
        .expect("install")
        .expect("có version");

        assert_eq!(version, expected_version);
        let platform = jvm_platform_string();
        let base = game
            .join("runtime/java-runtime-test")
            .join(platform)
            .join("java-runtime-test");
        // file lzma → decompress → đúng bản raw (sha1 raw verify trong pipeline).
        assert_eq!(
            std::fs::read(base.join("bin/java")).unwrap(),
            b"fake-java-binary-v1"
        );
        // file raw-only tải thẳng.
        assert_eq!(
            std::fs::read(base.join("lib/helper.txt")).unwrap(),
            b"helper-bytes"
        );
        // directory từ manifest.
        assert!(base.join("lib").is_dir());
        // part lzma đã dọn sau decompress.
        assert!(!base.join("bin/java.antares-lzma-part").exists());
        // .version = tên version (không newline).
        let parent = base.parent().unwrap();
        assert_eq!(
            std::fs::read_to_string(parent.join(".version")).unwrap(),
            "17.0.15-test"
        );
        // {component}.sha1 đúng format `{rel} /#// {sha1} {ctime}`.
        let sha1_file = std::fs::read_to_string(parent.join("java-runtime-test.sha1"))
            .expect(".sha1 file");
        let raw_sha1 = antares_downloads::sha1_hex(b"fake-java-binary-v1");
        assert!(
            sha1_file.contains(&format!("bin/java /#// {raw_sha1} ")),
            "{sha1_file}"
        );
        assert!(sha1_file.contains("lib/helper.txt /#// "), "{sha1_file}");
        // link (unix) — parity os.symlink với target string gốc.
        #[cfg(unix)]
        {
            let link = base.join("bin/java-link");
            let meta = std::fs::symlink_metadata(&link).expect("link tồn tại");
            assert!(meta.file_type().is_symlink());
            assert_eq!(
                std::fs::read_link(&link).unwrap(),
                PathBuf::from("../bin/java")
            );
        }
        // executable bit trên unix (parity chmod +x).
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let mode = std::fs::metadata(base.join("bin/java")).unwrap().mode();
            assert_ne!(mode & 0o111, 0, "bin/java phải có bit thực");
            let helper_mode = std::fs::metadata(base.join("lib/helper.txt")).unwrap().mode();
            assert_eq!(helper_mode & 0o111, 0, "helper không executable");
        }
        // Probe — parity legacy mojang_runtime_executable sau khi cài.
        assert_eq!(
            get_executable_path("java-runtime-test", &game),
            Some(base.join("bin/java"))
        );
        // Idempotent — lần 2 mọi job skip qua sha1 (không lỗi, không tải lại).
        let again = install_jvm_runtime(
            "java-runtime-test",
            &game,
            &endpoints,
            (1, false),
            &install::no_progress,
        )
        .expect("repair");
        assert_eq!(again, Some(expected_version));

        drop(server);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn missing_component_is_ok_none_and_empty_platform_too() {
        let root = temp_root("missing");
        let game = root.join("game");
        let (endpoints, _name, server) = serve_runtime();

        // Component không có trong all.json — parity MLL VersionNotFound
        // → legacy nuốt (trả None, KHÔNG raise).
        let out = install_jvm_runtime(
            "java-runtime-nope",
            &game,
            &endpoints,
            (1, false),
            &install::no_progress,
        )
        .expect("không raise");
        assert_eq!(out, None);

        // Platform không hỗ trợ (list rỗng) — parity MLL return im lặng.
        let out = install_jvm_runtime(
            "java-runtime-empty",
            &game,
            &endpoints,
            (1, false),
            &install::no_progress,
        )
        .expect("không raise");
        assert_eq!(out, None);
        assert!(!game.join("runtime").exists(), "không tạo gì khi None");

        drop(server);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn dead_endpoint_is_network_unavailable_and_wrapper_returns_none() {
        let root = temp_root("dead");
        let game = root.join("game");
        let err = install_jvm_runtime(
            "java-runtime-delta",
            &game,
            &dead_endpoints(),
            (1, false), // fail ngay, không backoff
            &install::no_progress,
        )
        .expect_err("endpoint chết");
        assert_eq!(err.code, codes::NETWORK_UNAVAILABLE);
        // Legacy wrapper nuốt mọi lỗi → None (orchestrator → JAVA_NOT_FOUND).
        assert_eq!(
            install_mojang_runtime(
                "java-runtime-delta",
                &game,
                &dead_endpoints(),
                (1, false),
                &install::no_progress,
            ),
            None
        );
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn path_escape_in_manifest_is_validation_failed() {
        let root = temp_root("escape");
        let game = root.join("game");

        let mut all_map = serde_json::Map::new();
        all_map.insert(
            jvm_platform_string().to_string(),
            json!({
                "java-runtime-evil": [{
                    "manifest": { "url": "${BASE}/manifest.json" },
                    "version": { "name": "0" }
                }]
            }),
        );
        // Manifest có entry thoát game dir (base = runtime/<c>/<p>/<c> —
        // 4 bậc dưới game_dir → 5 bậc `..` là vượt ra ngoài) — guard §50
        // chặn TRƯỚC download.
        let manifest = json!({
            "files": {
                "../../../../../evil.txt": {
                    "type": "file",
                    "downloads": {
                        "raw": { "url": "${BASE}/obj/evil", "sha1": "a".repeat(40) }
                    }
                }
            }
        });
        let (base, server) = serve_dynamic(move |base| {
            let all = Value::Object(all_map).to_string().replace("${BASE}", base);
            let manifest = manifest.to_string().replace("${BASE}", base);
            vec![
                ("/all.json".into(), all.into_bytes()),
                ("/manifest.json".into(), manifest.into_bytes()),
            ]
        });
        let endpoints = Endpoints {
            java_runtime: format!("{base}/all.json"),
            ..Endpoints::default()
        };

        let err = install_jvm_runtime(
            "java-runtime-evil",
            &game,
            &endpoints,
            (1, false),
            &install::no_progress,
        )
        .expect_err("path escape phải bị chặn");
        assert_eq!(err.code, codes::VALIDATION_FAILED, "{}", err.message);
        assert!(!root.join("evil.txt").exists(), "không ghi ra ngoài");

        drop(server);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// Smoke THẬT với mạng (tay, không chạy trong CI) — cài
    /// `java-runtime-delta` từ manifest Mojang thật (136 file, ~109MB raw
    /// nhưng chỉ ~3MB lzma tải về — parity MLL) rồi probe exe THẬT chạy
    /// được. Chạy tay:
    /// `cargo test -p antares-app real_network_mojang_runtime -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn real_network_mojang_runtime_smoke() {
        let root = temp_root("smoke-rt");
        let game = root.join("game");
        let endpoints = Endpoints::default();

        let version = install_jvm_runtime(
            "java-runtime-delta",
            &game,
            &endpoints,
            (3, true),
            &|status: &str, fraction: f64| {
                println!("[runtime {fraction:.2}] {status}");
            },
        )
        .expect("cài java-runtime-delta với mạng thật")
        .expect("component có trên platform này")
        .to_string();
        println!("installed version: {version}");

        let exe = get_executable_path("java-runtime-delta", &game).expect("probe exe");
        assert!(exe.is_file(), "{}", exe.display());
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            assert_ne!(
                std::fs::metadata(&exe).unwrap().mode() & 0o111,
                0,
                "chmod +x parity"
            );
        }
        // java THẬT khởi động được — check mạnh nhất (parity detect_major).
        let major = antares_java::detect_major(&exe).expect("java -showversion chạy được");
        println!("detected major: {major}");
        assert!(major >= 16, "java-runtime-delta = Java 21, thấy {major}");

        // metadata parity — .version + {component}.sha1 ở mức platform.
        let platform_dir = exe
            .parent()
            .and_then(|p| p.parent())
            .and_then(|p| p.parent())
            .expect("runtime/{c}/{platform}");
        assert_eq!(
            std::fs::read_to_string(platform_dir.join(".version")).unwrap(),
            version
        );
        let sha1_file = std::fs::read_to_string(
            platform_dir.join("java-runtime-delta.sha1"),
        )
        .expect("{component}.sha1");
        assert!(sha1_file.lines().count() >= 100, "136 file: {} dòng", sha1_file.lines().count());
        assert!(sha1_file.contains("bin/java /#// "), "{sha1_file}");
        // link thật tồn tại (205 link trên linux).
        #[cfg(unix)]
        let link_count = {
            fn count_links(dir: &Path) -> usize {
                std::fs::read_dir(dir)
                    .map(|entries| {
                        entries.flatten().map(|e| e.path()).map(|p| {
                            let is_link = std::fs::symlink_metadata(&p)
                                .map(|m| m.file_type().is_symlink())
                                .unwrap_or(false);
                            usize::from(is_link)
                                + if p.is_dir() && !is_link { count_links(&p) } else { 0 }
                        }).sum()
                    })
                    .unwrap_or(0)
            }
            let base = exe.parent().and_then(|p| p.parent()).unwrap();
            count_links(base)
        };
        #[cfg(unix)]
        assert!(link_count >= 100, "link parity, thấy {link_count}");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn join_inside_allows_inner_dotdot_but_blocks_escape() {
        let game_dir = Path::new("/game/inst/game");
        let base = game_dir.join("runtime/c/p/c");
        // `..` trong phạm vi game_dir hợp lệ (link ../java.base/x).
        assert_eq!(
            join_inside(&base, "../java.base/lib", game_dir).unwrap(),
            game_dir.join("runtime/c/p/java.base/lib")
        );
        // base cách game_dir 4 bậc → 5 bậc `..` là vượt ra ngoài → chặn
        // (component-based — không theo string-prefix).
        assert!(join_inside(&base, "../../../../../evil", game_dir).is_err());
        assert!(join_inside(&base, "/abs/path", game_dir).is_err());
        // Quirk string-prefix của MLL bị loại bỏ: `..` ×5 + `gamefoo` ra
        // `/game/inst/gamefoo` — MLL `startswith("/game/inst/game")` CHẶN SAI
        // (True) → cho qua; ta bắt đúng (5 > 4).
        assert!(join_inside(&base, "../../../../../gamefoo/x", game_dir).is_err());
        // 4 bậc `..` dừng ĐÚNG ở game_dir rồi đi vào nó vẫn hợp lệ.
        assert_eq!(
            join_inside(&base, "../../../../inside.txt", game_dir).unwrap(),
            game_dir.join("inside.txt")
        );
    }
}
