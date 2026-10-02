
use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub mod download;
pub mod http;
pub mod resume;
pub mod sha;

pub use download::{download, quick_hashes, DownloadPipelineError, DownloadRequest};
pub use http::{
    get, get_stream, https_url_to_http, parse_url, HttpError, HttpOrUserError, HttpResponse,
    StreamHead,
};
pub use resume::PartInfo;
pub use sha::{
    commit_artifact, sha1_digest, sha1_hex, sha256_digest, sha256_hex, verify_file,
    verify_file_streaming, Sha1Hasher, Sha256Hasher,
};

// ---------------------------------------------------------------------------
// §103 — state machine
// ---------------------------------------------------------------------------

/// Trạng thái của một download job (§103).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DownloadState {
    Discover,
    Prepare,
    Download,
    Verify,
    Commit,
    /// Terminal thành công.
    Done,
    /// Terminal thất bại (sau khi retry hết).
    Failed,
    /// Terminal do user cancel.
    Cancelled,
}

impl DownloadState {
    /// Transition hợp lệ theo đồ thị §103. `Done/Failed/Cancelled` là terminal.
    pub fn can_transition(self, next: DownloadState) -> bool {
        use DownloadState::*;
        if self.is_terminal() {
            return false;
        }
        matches!(
            (self, next),
            (Discover, Prepare)
                | (Prepare, Download)
                | (Download, Download) // retry / resume
                | (Download, Verify)
                | (Verify, Verify) // retry verify
                | (Verify, Download) // checksum sai → tải lại
                | (Verify, Commit)
                | (Commit, Done)
        )
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            DownloadState::Done | DownloadState::Failed | DownloadState::Cancelled
        )
    }
}

// ---------------------------------------------------------------------------
// Checksum — parity services/downloads/checksum.py
// ---------------------------------------------------------------------------

/// Thuật toán checksum hỗ trợ (mở rộng sau khi nối HTTP engine thật).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChecksumAlgorithm {
    Sha1,
    Sha256,
}

/// So sánh checksum hex expected/actual theo thuật toán.
/// Parity `services/downloads/checksum.py`: mismatch → error giữ code `CHECKSUM_MISMATCH`.
pub fn verify_checksum(
    algorithm: ChecksumAlgorithm,
    expected: &str,
    actual: &str,
) -> Result<(), ChecksumError> {
    let expected = expected.trim().to_lowercase();
    let actual = actual.trim().to_lowercase();
    let expected_len = match algorithm {
        ChecksumAlgorithm::Sha1 => 40,
        ChecksumAlgorithm::Sha256 => 64,
    };
    if expected.len() != expected_len || !expected.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ChecksumError::InvalidDigest {
            field: "expected".into(),
        });
    }
    if actual.len() != expected_len || !actual.chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ChecksumError::InvalidDigest {
            field: "actual".into(),
        });
    }
    if expected != actual {
        return Err(ChecksumError::Mismatch);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ChecksumError {
    #[error("checksum mismatch")]
    Mismatch,
    #[error("invalid {field} digest length/format")]
    InvalidDigest { field: &'static str },
    /// Phase 2 — file không đọc được khi verify (đã bị xoá/di chuyển).
    #[error("file unreadable: {path}")]
    FileUnreadable { path: String },
}

impl ChecksumError {
    /// Code taxonomy §117 (parity `CHECKSUM_MISMATCH` legacy).
    pub fn code(&self) -> &'static str {
        match self {
            ChecksumError::Mismatch => "CHECKSUM_MISMATCH",
            ChecksumError::InvalidDigest { .. } => "CONFIG_INVALID",
            ChecksumError::FileUnreadable { .. } => "FILE_UNREADABLE",
        }
    }
}

// ---------------------------------------------------------------------------
// §103 — Download job + dedup registry
// ---------------------------------------------------------------------------

/// Một download job — model dữ liệu của engine (I/O nối ở phase sau qua HTTP engine §102).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadJob {
    pub id: String,
    pub url: String,
    pub artifact_path: String,
    pub algorithm: Option<ChecksumAlgorithm>,
    pub expected_digest: Option<String>,
    pub state: DownloadState,
    pub bytes_downloaded: u64,
    pub bytes_total: Option<u64>,
    pub attempts: u32,
    pub max_attempts: u32,
}

impl DownloadJob {
    pub fn new(id: impl Into<String>, url: impl Into<String>, artifact_path: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            url: url.into(),
            artifact_path: artifact_path.into(),
            algorithm: None,
            expected_digest: None,
            state: DownloadState::Discover,
            bytes_downloaded: 0,
            bytes_total: None,
            attempts: 0,
            max_attempts: 3,
        }
    }

    pub fn with_checksum(mut self, algorithm: ChecksumAlgorithm, digest: impl Into<String>) -> Self {
        self.algorithm = Some(algorithm);
        self.expected_digest = Some(digest.into());
        self
    }

    /// Chuyển state nếu hợp lệ, không thì lỗi `INVALID_TRANSITION`.
    pub fn transition(&mut self, next: DownloadState) -> Result<(), DownloadError> {
        if !self.state.can_transition(next) {
            return Err(DownloadError::InvalidTransition {
                from: format!("{:?}", self.state),
                to: format!("{next:?}"),
            });
        }
        self.state = next;
        if next == DownloadState::Download {
            self.attempts += 1;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DownloadError {
    #[error("invalid transition {from} -> {to}")]
    InvalidTransition { from: String, to: String },
    #[error("download already registered: {0}")]
    Duplicate(String),
    #[error("unknown download job: {0}")]
    Unknown(String),
}

impl DownloadError {
    pub fn code(&self) -> &'static str {
        match self {
            DownloadError::InvalidTransition { .. } => "DOWNLOAD_INVALID_TRANSITION",
            DownloadError::Duplicate(_) => "CONFIG_INVALID",
            DownloadError::Unknown(_) => "DOWNLOAD_NOT_FOUND",
        }
    }
}

/// §103 — dedup registry: hai instance cần cùng artifact → chung 1 physical download.
/// Key = `(url, artifact_path)`; job đầu tiên đăng ký thắng, request sau join vào.
#[derive(Default)]
pub struct DedupRegistry {
    by_key: HashMap<(String, String), String>,
    jobs: HashMap<String, DownloadJob>,
}

impl DedupRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Đăng ký job; nếu `(url, artifact_path)` đã có trả về `Duplicate` kèm job id hiện hữu
    /// để caller join thay vì tải lại.
    pub fn register(&mut self, job: DownloadJob) -> Result<String, DownloadError> {
        let key = (job.url.clone(), job.artifact_path.clone());
        if let Some(existing) = self.by_key.get(&key) {
            return Err(DownloadError::Duplicate(existing.clone()));
        }
        let id = job.id.clone();
        self.by_key.insert(key, id.clone());
        self.jobs.insert(id.clone(), job);
        Ok(id)
    }

    /// Join vào job hiện hữu cho `(url, artifact_path)` — `None` nếu chưa có ai tải.
    pub fn find_shared(&self, url: &str, artifact_path: &str) -> Option<&DownloadJob> {
        self.by_key
            .get(&(url.to_string(), artifact_path.to_string()))
            .and_then(|id| self.jobs.get(id))
    }

    pub fn get(&self, id: &str) -> Option<&DownloadJob> {
        self.jobs.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut DownloadJob> {
        self.jobs.get_mut(id)
    }

    /// Huỷ đăng ký sau khi job terminal (Done/Failed/Cancelled) để key tái sử dụng được.
    pub fn finish(&mut self, id: &str) -> Result<DownloadJob, DownloadError> {
        let job = self.jobs.remove(id).ok_or_else(|| DownloadError::Unknown(id.to_string()))?;
        if !job.state.is_terminal() {
            self.jobs.insert(id.to_string(), job.clone());
            return Err(DownloadError::InvalidTransition {
                from: format!("{:?}", job.state),
                to: "terminal".into(),
            });
        }
        self.by_key
            .remove(&(job.url.clone(), job.artifact_path.clone()));
        Ok(job)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_machine_happy_path() {
        let mut job = DownloadJob::new("d1", "https://example.com/a.jar", "libraries/a.jar");
        assert_eq!(job.state, DownloadState::Discover);
        for step in [
            DownloadState::Prepare,
            DownloadState::Download,
            DownloadState::Verify,
            DownloadState::Commit,
            DownloadState::Done,
        ] {
            job.transition(step).expect("transition ok");
        }
        assert!(job.state.is_terminal());
        assert_eq!(job.attempts, 1);
    }

    #[test]
    fn state_machine_rejects_backward() {
        let mut job = DownloadJob::new("d2", "u", "p");
        job.transition(DownloadState::Prepare).unwrap();
        assert!(job.transition(DownloadState::Discover).is_err());
        // terminal không đi tiếp
        job.transition(DownloadState::Download).unwrap();
        job.transition(DownloadState::Verify).unwrap();
        job.transition(DownloadState::Commit).unwrap();
        job.transition(DownloadState::Done).unwrap();
        assert!(job.transition(DownloadState::Download).is_err());
    }

    #[test]
    fn verify_retry_and_redownload_paths() {
        // Verify → Verify (retry) và Verify → Download (checksum sai) đều hợp lệ (§103).
        let mut job = DownloadJob::new("d3", "u", "p");
        job.transition(DownloadState::Prepare).unwrap();
        job.transition(DownloadState::Download).unwrap();
        job.transition(DownloadState::Verify).unwrap();
        job.transition(DownloadState::Verify).unwrap();
        job.transition(DownloadState::Download).unwrap();
        assert_eq!(job.attempts, 2);
    }

    #[test]
    fn checksum_verify_parity_codes() {
        let sha1 = "d3b07384d113edec49eaa6238ad5ff00b3cf2bcb"; // 40 hex
        assert!(verify_checksum(ChecksumAlgorithm::Sha1, sha1, sha1).is_ok());
        let err = verify_checksum(ChecksumAlgorithm::Sha1, sha1, "deadbeef").unwrap_err();
        assert_eq!(err.code(), "CONFIG_INVALID");
        let mismatch = verify_checksum(ChecksumAlgorithm::Sha1, sha1, &"a".repeat(40)).unwrap_err();
        assert_eq!(mismatch.code(), "CHECKSUM_MISMATCH");
        let sha256 = "ab".repeat(32);
        assert!(verify_checksum(ChecksumAlgorithm::Sha256, &sha256, &sha256).is_ok());
        // case/whitespace không quan trọng
        assert!(verify_checksum(ChecksumAlgorithm::Sha1, &sha1.to_uppercase(), sha1).is_ok());
    }

    #[test]
    fn dedup_registry_shares_one_download() {
        let mut registry = DedupRegistry::new();
        let job = DownloadJob::new("d4", "https://example.com/lib.jar", "libraries/lib.jar")
            .with_checksum(ChecksumAlgorithm::Sha256, "ab".repeat(32));
        registry.register(job).expect("first registers");

        // Request B cùng artifact → Duplicate kèm id job A.
        let dup = registry
            .register(DownloadJob::new("d5", "https://example.com/lib.jar", "libraries/lib.jar"))
            .unwrap_err();
        assert_eq!(dup.code(), "CONFIG_INVALID");
        assert_eq!(dup.to_string(), "download already registered: d4");

        let shared = registry
            .find_shared("https://example.com/lib.jar", "libraries/lib.jar")
            .expect("shared job");
        assert_eq!(shared.id, "d4");
        assert_eq!(shared.expected_digest.as_deref(), Some("ab".repeat(32).as_str()));

        // Chưa terminal thì không finish được.
        assert!(registry.finish("d4").is_err());
        registry.get_mut("d4").unwrap().transition(DownloadState::Prepare).unwrap();
        registry.get_mut("d4").unwrap().transition(DownloadState::Download).unwrap();
        registry.get_mut("d4").unwrap().transition(DownloadState::Verify).unwrap();
        registry.get_mut("d4").unwrap().transition(DownloadState::Commit).unwrap();
        registry.get_mut("d4").unwrap().transition(DownloadState::Done).unwrap();

        let finished = registry.finish("d4").expect("finish terminal");
        assert_eq!(finished.id, "d4");
        assert!(registry.find_shared("https://example.com/lib.jar", "libraries/lib.jar").is_none());
        assert!(registry.get("d4").is_none());
    }
}
