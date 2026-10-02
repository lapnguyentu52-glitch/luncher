//! Phase 3 — Download pipeline, parity `services/downloads/manager.py::DownloadManager`
//! (mục 9, mục 174): cache check → HTTP (Range khi resume) → .part → checksum → atomic
//! finalize. KHÔNG replace file đích trước khi verify (mục 9.3).
//!
//! Parity chính:
//! - target tồn tại + hợp lệ + `!overwrite` → `Ok(false)` ("đã tồn tại & hợp lệ")
//! - target corrupt → redownload (ghi đè)
//! - max_attempts 8 (mặc định), backoff `min(2*attempt, 8)` — checksum mismatch KHÔNG
//!   backoff (parity: `continue` không sleep)
//! - resume: `resumable and attempt > 1` — attempt đầu không resume (parity legacy);
//!   server không trả 206 → restart từ đầu
//! - verify sha1/sha256 trước finalize; fail → cleanup part + retry

use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::http::{get_stream, HttpError};
use crate::resume::PartInfo;
use crate::{
    sha1_hex, sha256_hex, verify_file_streaming, ChecksumAlgorithm, ChecksumError,
};

const DEFAULT_MAX_ATTEMPTS: u32 = 8;
const DEFAULT_HTTP_TIMEOUT_SECS: u64 = 30;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DownloadPipelineError {
    #[error("cancelled at attempt {0}")]
    Cancelled(u32),
    #[error("SHA1 mismatch: {0}")]
    Sha1Mismatch(String),
    #[error("SHA256 mismatch: {0}")]
    Sha256Mismatch(String),
    #[error("http: {0}")]
    Http(String),
    #[error("io: {0}")]
    Io(String),
    #[error("exhausted {attempts} attempts: {last}")]
    Exhausted { attempts: u32, last: String },
}

impl DownloadPipelineError {
    /// Code taxonomy §117 — parity codes.DOWNLOAD_*.
    pub fn code(&self) -> &'static str {
        match self {
            DownloadPipelineError::Cancelled(_) => "DOWNLOAD_CANCELLED",
            DownloadPipelineError::Sha1Mismatch(_) | DownloadPipelineError::Sha256Mismatch(_) => {
                "DOWNLOAD_CHECKSUM_MISMATCH"
            }
            DownloadPipelineError::Http(_) => "NET_UNREACHABLE",
            DownloadPipelineError::Io(_) => "DOWNLOAD_IO",
            DownloadPipelineError::Exhausted { .. } => "DOWNLOAD_FAILED",
        }
    }
}

/// Options một lần download (parity tham số `DownloadManager.download`).
pub struct DownloadRequest {
    pub url: String,
    pub target: PathBuf,
    pub sha1: Option<String>,
    pub sha256: Option<String>,
    pub overwrite: bool,
    pub max_attempts: u32,
    pub http_timeout: Duration,
    /// Bật/tắt backoff giữa attempts (test = false → sleep 0).
    pub backoff: bool,
    /// Cancel flag — checked mỗi attempt (parity task.cancelled).
    pub cancelled: Option<std::sync::Arc<AtomicBool>>,
}

impl DownloadRequest {
    pub fn new(url: impl Into<String>, target: impl Into<PathBuf>) -> Self {
        Self {
            url: url.into(),
            target: target.into(),
            sha1: None,
            sha256: None,
            overwrite: false,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            http_timeout: Duration::from_secs(DEFAULT_HTTP_TIMEOUT_SECS),
            backoff: true,
            cancelled: None,
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::Relaxed))
    }
}

fn verify_digests(
    request: &DownloadRequest,
    path: &Path,
) -> Result<(), DownloadPipelineError> {
    // Streaming verify — file lớn không load cả vào RAM (sha.rs incremental).
    if let Some(expected) = &request.sha1 {
        verify_file_streaming(ChecksumAlgorithm::Sha1, path, expected).map_err(|err| match err {
            ChecksumError::Mismatch => DownloadPipelineError::Sha1Mismatch(request.url.clone()),
            other => DownloadPipelineError::Io(other.to_string()),
        })?;
    }
    if let Some(expected) = &request.sha256 {
        verify_file_streaming(ChecksumAlgorithm::Sha256, path, expected)
            .map_err(|err| match err {
                ChecksumError::Mismatch => {
                    DownloadPipelineError::Sha256Mismatch(request.url.clone())
                }
                other => DownloadPipelineError::Io(other.to_string()),
            })?;
    }
    Ok(())
}

/// Pipeline chính — trả `Ok(false)` nếu target đã tồn tại & hợp lệ (parity docstring).
pub fn download(request: &DownloadRequest) -> Result<bool, DownloadPipelineError> {
    // 1. Cache check (parity: target.exists() && !overwrite → verify → False)
    if request.target.exists() && !request.overwrite {
        let has_digest = request.sha1.is_some() || request.sha256.is_some();
        if !has_digest || verify_digests(request, &request.target).is_ok() {
            return Ok(false);
        }
        // corrupt → redownload (tiếp pipeline)
    }

    let mut part = PartInfo::for_target(&request.target);
    part.url = request.url.clone();
    let meta_ok = part.load_meta().unwrap_or(false);
    let resumable = part.bytes_have() > 0 && meta_ok;

    let mut last_err = String::new();
    for attempt in 1..=request.max_attempts {
        if request.is_cancelled() {
            return Err(DownloadPipelineError::Cancelled(attempt));
        }
        match attempt_once(request, &mut part, resumable && attempt > 1) {
            Ok(()) => {
                // Verify trước khi finalize (mục 9.3) — mismatch → cleanup + retry
                // KHÔNG backoff (parity continue không sleep).
                if let Err(err) = verify_digests(request, &part.part_path) {
                    match err {
                        DownloadPipelineError::Sha1Mismatch(_) | DownloadPipelineError::Sha256Mismatch(_) => {
                            part.cleanup();
                            last_err = err.to_string();
                            continue;
                        }
                        other => return Err(other),
                    }
                }
                part.finalize(&request.target)
                    .map_err(|err| DownloadPipelineError::Io(err.to_string()))?;
                return Ok(true);
            }
            Err(DownloadPipelineError::Sha1Mismatch(url)) => {
                part.cleanup();
                last_err = format!("SHA1 mismatch: {url}");
                continue;
            }
            Err(DownloadPipelineError::Sha256Mismatch(url)) => {
                part.cleanup();
                last_err = format!("SHA256 mismatch: {url}");
                continue;
            }
            Err(err) => {
                last_err = err.to_string();
                if request.is_cancelled() {
                    return Err(DownloadPipelineError::Cancelled(attempt));
                }
                // backoff parity min(2*attempt, 8) — chỉ khi còn attempt
                if request.backoff && attempt < request.max_attempts {
                    std::thread::sleep(Duration::from_secs((2 * attempt as u64).min(8)));
                }
            }
        }
    }
    Err(DownloadPipelineError::Exhausted {
        attempts: request.max_attempts,
        last: last_err,
    })
}

/// 1 attempt streaming: HTTP GET (Range nếu resume) → ghi .part từng chunk 64KB
/// (không giữ body nguyên khối trong RAM) → save meta.
fn attempt_once(
    request: &DownloadRequest,
    part: &mut PartInfo,
    resume: bool,
) -> Result<(), DownloadPipelineError> {
    let mut headers: Vec<(String, String)> = Vec::new();
    let mut append = false;
    let mut have = 0u64;
    if resume {
        have = part.bytes_have();
        if have > 0 {
            headers.push(("Range".into(), format!("bytes={have}-")));
            append = true;
        }
    }

    let target_part = part.part_path.clone();
    if let Some(parent) = target_part.parent() {
        std::fs::create_dir_all(parent).map_err(|err| DownloadPipelineError::Io(err.to_string()))?;
    }

    let file = if append {
        std::fs::OpenOptions::new()
            .append(true)
            .open(&target_part)
            .map_err(|err| DownloadPipelineError::Io(err.to_string()))?
    } else {
        std::fs::File::create(&target_part).map_err(|err| DownloadPipelineError::Io(err.to_string()))?
    };
    let mut writer = std::io::BufWriter::new(file);

    let head = get_stream(
        &request.url,
        &headers,
        request.http_timeout,
        |chunk| writer.write_all(chunk).map_err(|err| err.to_string()),
    )
    .map_err(|err| match err {
        crate::http::HttpOrUserError::Http(http) => match &http {
            HttpError::Io(msg) => DownloadPipelineError::Http(msg.clone()),
            other => DownloadPipelineError::Http(other.to_string()),
        },
        crate::http::HttpOrUserError::User(io_msg) => DownloadPipelineError::Io(io_msg),
    })?;
    writer
        .flush()
        .map_err(|err| DownloadPipelineError::Io(err.to_string()))?;

    // Server không hỗ trợ range → RESTART thật (parity mode "wb"): body 200 vừa rồi
    // đã bị append vào part stale → attempt lại với resume=false (File::create truncate
    // ghi đè toàn bộ part). Part cuối cùng = full body, đúng như legacy.
    if append && head.status != 206 {
        return attempt_once(request, part, false);
    }

    // content-length (còn lại) + have = tổng (parity)
    let total = head
        .header("content-length")
        .and_then(|v| v.parse::<u64>().ok())
        .map(|len| len + if head.status == 206 { have } else { 0 });
    if let Some(total) = total {
        part.size = total;
        let _ = part.save_meta();
    }
    Ok(())
}

/// Helper tính sha1/sha256 hex cho bytes (tiện caller test).
pub fn quick_hashes(bytes: &[u8]) -> (String, String) {
    (sha1_hex(bytes), sha256_hex(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{BufRead, BufReader, Write};
    use std::net::TcpListener;

    const PAYLOAD: &[u8] = b"antares-download-payload-0123456789";

    /// Server hỗ trợ Range thật: đọc request headers, trả 206 (từ start) hoặc 200.
    /// `accept` = số connection tối đa — hết thì thread thoát (tránh accept block
    /// vô hạn với test không kết nối).
    fn spawn_range_server(accept: usize, payload: &'static [u8]) -> (u16, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().unwrap().port();
        let handle = std::thread::spawn(move || {
            for _ in 0..accept {
            if let Ok((stream, _)) = listener.accept() {
                let mut stream = stream;
                let mut reader = BufReader::new(stream.try_clone().expect("clone"));
                let mut request_line = String::new();
                let _ = reader.read_line(&mut request_line);
                let mut range_start: Option<usize> = None;
                loop {
                    let mut line = String::new();
                    let _ = reader.read_line(&mut line);
                    if line.trim().is_empty() {
                        break;
                    }
                    if let Some(value) = line.to_lowercase().strip_prefix("range: bytes=") {
                        range_start = value.trim().trim_end_matches('-').parse().ok();
                    }
                }
                let (head, body): (String, &[u8]) = match range_start {
                    Some(start) if start < payload.len() && start > 0 => (
                        format!(
                            "HTTP/1.1 206 Partial Content\r\nContent-Length: {}\r\n\r\n",
                            payload.len() - start
                        ),
                        &payload[start..],
                    ),
                    _ => (
                        format!(
                            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                            payload.len()
                        ),
                        payload,
                    ),
                };
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(body);
                let _ = stream.flush();
            }
            }
        });
        (port, handle)
    }

    fn temp_target(tag: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("antares-dl-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root.join("out/artifact.jar")
    }

    fn cleanup(target: &Path) {
        let mut root = target.to_path_buf();
        root.pop();
        root.pop();
        let _ = std::fs::remove_dir_all(root);
    }

    fn request(url: String, target: &Path) -> DownloadRequest {
        let mut request = DownloadRequest::new(url, target);
        request.backoff = false;
        request
    }

    #[test]
    fn downloads_fresh_and_returns_true() {
        let (port, server) = spawn_range_server(1, PAYLOAD);
        let target = temp_target("fresh");
        let mut req = request(format!("http://127.0.0.1:{port}/a.jar"), &target);
        let (sha1, sha256) = quick_hashes(PAYLOAD);
        req.sha1 = Some(sha1);
        req.sha256 = Some(sha256);

        assert!(download(&req).unwrap());
        assert_eq!(std::fs::read(&target).unwrap(), PAYLOAD);
        // Không só part/meta
        assert!(!target.with_file_name("artifact.jar.antares-part").exists());
        assert!(!target.with_file_name("artifact.jar.antares-part.meta").exists());
        server.join().unwrap();
        cleanup(&target);
    }

    #[test]
    fn existing_valid_file_skips_download() {
        let (port, server) = spawn_range_server(0, PAYLOAD);
        let target = temp_target("skip");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, PAYLOAD).unwrap();
        let (_, sha256) = quick_hashes(PAYLOAD);

        let mut req = request(format!("http://127.0.0.1:{port}/a.jar"), &target);
        req.sha256 = Some(sha256);
        // Server không được đụng tới — trả Ok(false)
        assert!(!download(&req).unwrap());
        assert_eq!(std::fs::read(&target).unwrap(), PAYLOAD);
        server.join().unwrap();
        cleanup(&target);
    }

    #[test]
    fn existing_corrupt_file_redownloads() {
        let (port, server) = spawn_range_server(1, PAYLOAD);
        let target = temp_target("corrupt");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, b"corrupted").unwrap();
        let (sha1, _) = quick_hashes(PAYLOAD);

        let mut req = request(format!("http://127.0.0.1:{port}/a.jar"), &target);
        req.sha1 = Some(sha1);
        assert!(download(&req).unwrap());
        assert_eq!(std::fs::read(&target).unwrap(), PAYLOAD);
        server.join().unwrap();
        cleanup(&target);
    }

    #[test]
    fn checksum_mismatch_retries_then_exhausts() {
        let (port, server) = spawn_range_server(2, PAYLOAD);
        let target = temp_target("mismatch");
        let mut req = request(format!("http://127.0.0.1:{port}/a.jar"), &target);
        req.sha256 = Some("a".repeat(64));
        req.max_attempts = 2;

        let err = download(&req).unwrap_err();
        assert_eq!(err.code(), "DOWNLOAD_FAILED");
        // Part đã được cleanup giữa các attempt
        assert!(!target.with_file_name("artifact.jar.antares-part").exists());
        server.join().unwrap();
        cleanup(&target);
    }

    #[test]
    fn resume_appends_from_partial_part() {
        let (port, server) = spawn_range_server(1, PAYLOAD);
        let target = temp_target("resume");
        // Giả lập part tải dở 10 byte đầu + meta url khớp.
        let mut part = PartInfo::for_target(&target);
        part.url = format!("http://127.0.0.1:{port}/a.jar");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&part.part_path, &PAYLOAD[..10]).unwrap();
        part.size = PAYLOAD.len() as u64;
        part.save_meta().unwrap();

        let req = request(part.url.clone(), &target);
        assert!(download(&req).unwrap());
        // File cuối = payload đầy đủ (part 10 byte + 206 append phần còn lại)
        assert_eq!(std::fs::read(&target).unwrap(), PAYLOAD);
        server.join().unwrap();
        cleanup(&target);
    }

    #[test]
    fn server_without_range_support_restarts_from_scratch() {
        // Server bỏ qua Range → trả 200 full → pipeline restart ghi đè part.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            if let Ok((stream, _)) = listener.accept() {
                let mut stream = stream;
                let _ = stream.write_all(
                    format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n",
                        PAYLOAD.len()
                    )
                    .as_bytes(),
                );
                let _ = stream.write_all(PAYLOAD);
                let _ = stream.flush();
            }
        });

        let target = temp_target("no206");
        let mut part = PartInfo::for_target(&target);
        part.url = format!("http://127.0.0.1:{port}/a.jar");
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&part.part_path, b"stale").unwrap();
        part.save_meta().unwrap();

        let req = request(part.url.clone(), &target);
        assert!(download(&req).unwrap());
        assert_eq!(std::fs::read(&target).unwrap(), PAYLOAD);
        server.join().unwrap();
        cleanup(&target);
    }

    #[test]
    fn cancelled_flag_short_circuits() {
        let (port, server) = spawn_range_server(0, PAYLOAD);
        let target = temp_target("cancel");
        let mut req = request(format!("http://127.0.0.1:{port}/a.jar"), &target);
        req.cancelled = Some(std::sync::Arc::new(AtomicBool::new(true)));

        let err = download(&req).unwrap_err();
        assert_eq!(err.code(), "DOWNLOAD_CANCELLED");
        // Không join: cancelled short-circuit trước khi connect → accept() block
        // vô hạn. Thread chết khi test process exit.
        drop(server);
        cleanup(&target);
    }

    #[test]
    fn connection_refused_exhausts_with_zero_backoff() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);

        let target = temp_target("refused");
        let mut req = request(format!("http://127.0.0.1:{port}/a.jar"), &target);
        req.max_attempts = 2;

        let err = download(&req).unwrap_err();
        assert_eq!(err.code(), "DOWNLOAD_FAILED");
        assert!(err.to_string().contains("2 attempts"));
        cleanup(&target);
    }
}
