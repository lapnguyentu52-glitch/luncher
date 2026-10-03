//! F-12 — LogMux: pipeline merge timestamped stdout/stderr + scoped log storage.
//!
//! ```text
//! child stdout ─┐
//!               ├→ LogMux → bounded ring (drop-oldest) → <logs root>/<scope>.log
//! child stderr ─┘
//! ```
//!
//! - Mỗi record có `timestamp_ms` (UTC epoch ms) + `stream` — gộp 2 stream vẫn
//!   phân biệt được nguồn (legacy `stderr=STDOUT` gộp mù, ở đây có tag).
//! - Ring bounded + `dropped` counter (§88 không giữ vô hạn) — 10–20 instance
//!   song song không phình memory.
//! - Scope file: `create_scoped()` chỉ nhận scope sạch (`[A-Za-z0-9._-]`,
//!   không `.`/`..`) → không escape khỏi logs root (§50).
//! - Writer thread-safe (`&self` + Mutex) — consumer UI/diagnostics đọc `recent()`
//!   song song với pump.

use std::collections::VecDeque;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::{Mutex, PoisonError};
use std::time::{SystemTime, UNIX_EPOCH};

/// Nguồn dòng log.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogStream {
    Stdout,
    Stderr,
}

impl LogStream {
    /// Tag trong file/log line (`[stdout]` / `[stderr]`).
    pub fn tag(self) -> &'static str {
        match self {
            LogStream::Stdout => "stdout",
            LogStream::Stderr => "stderr",
        }
    }
}

/// Một dòng đã merge: timestamp + stream + nội dung.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRecord {
    pub timestamp_ms: u64,
    pub stream: LogStream,
    pub line: String,
}

impl LogRecord {
    /// Record với timestamp hiện tại — reader thread chốt thời điểm **tại lúc
    /// đọc được dòng**, nên merge 2 stream vẫn giữ đúng thứ tự thời gian thật.
    pub fn now(stream: LogStream, line: impl Into<String>) -> Self {
        Self {
            timestamp_ms: now_ms(),
            stream,
            line: line.into(),
        }
    }

    /// `[2026-10-02T15:33:18.123Z] [stdout] line`
    pub fn format_line(&self) -> String {
        format!(
            "{} [{}] {}",
            format_utc_ms(self.timestamp_ms),
            self.stream.tag(),
            self.line
        )
    }
}

/// Epoch ms hiện tại (giá trị 0 nếu đồng hồ trước 1970 — không panic).
pub fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// `YYYY-MM-DDTHH:MM:SS.mmmZ` — civil date UTC (thuật toán Hinnant, không zone).
pub fn format_utc_ms(ms: u64) -> String {
    let secs = ms / 1_000;
    let millis = ms % 1_000;
    let days = (secs / 86_400) as i64;
    let tod = secs % 86_400;
    let (h, m, s) = ((tod / 3_600) as u32, ((tod % 3_600) / 60) as u32, (tod % 60) as u32);
    // civil_from_days (Howard Hinnant) — UTC date từ days since epoch.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if month <= 2 { year + 1 } else { year };
    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}.{millis:03}Z")
}

struct MuxState {
    ring: VecDeque<LogRecord>,
    dropped: usize,
    file: Option<File>,
}

/// Bounded log pipeline cho một process/scope.
pub struct LogMux {
    scope: String,
    capacity: usize,
    state: Mutex<MuxState>,
}

impl LogMux {
    /// Mux chỉ giữ trong memory (ring bounded, không ghi file).
    pub fn new(scope: impl Into<String>, capacity: usize) -> Self {
        Self {
            scope: scope.into(),
            capacity: capacity.max(1),
            state: Mutex::new(MuxState {
                ring: VecDeque::new(),
                dropped: 0,
                file: None,
            }),
        }
    }

    /// Scoped storage: ring bounded + append vào `<logs_root>/<scope>.log`.
    ///
    /// Scope phải sạch (§50) — chỉ `[A-Za-z0-9._-]`, không rỗng, không `.`/`..`,
    /// ≤64 ký tự → không escape khỏi `logs_root`.
    pub fn create_scoped(
        logs_root: impl AsRef<Path>,
        scope: &str,
        capacity: usize,
    ) -> std::io::Result<Self> {
        validate_scope(scope)?;
        let root = logs_root.as_ref();
        std::fs::create_dir_all(root)?;
        let path = root.join(format!("{scope}.log"));
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            scope: scope.to_string(),
            capacity: capacity.max(1),
            state: Mutex::new(MuxState {
                ring: VecDeque::new(),
                dropped: 0,
                file: Some(file),
            }),
        })
    }

    /// Push dòng mới — timestamp ngay tại đây (không cần biết ai pump).
    pub fn push(&self, stream: LogStream, line: impl Into<String>) {
        self.push_record(LogRecord::now(stream, line));
    }

    /// Push record đã có timestamp (dùng khi reader thread đã chốt lúc đọc).
    pub fn push_record(&self, record: LogRecord) {
        // Poison → dữ liệu vẫn nhất quán từng thao tác (ring push/file write
        // không panic giữa chừng) → lấy lock tiếp thay vì panic lan.
        let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        if state.ring.len() >= self.capacity {
            state.ring.pop_front();
            state.dropped += 1;
        }
        state.ring.push_back(record.clone());
        if let Some(file) = state.file.as_mut() {
            // Lỗi đĩa không được kill pipeline log — giữ ring hoạt động.
            let _ = writeln!(file, "{}", record.format_line());
        }
    }

    /// Snapshot ring theo thứ tự push (bounded — tối đa `capacity` dòng gần nhất).
    pub fn recent(&self) -> Vec<LogRecord> {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.ring.iter().cloned().collect()
    }

    /// Số dòng hiện giữ.
    pub fn len(&self) -> usize {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.ring.len()
    }

    /// Số dòng đã bị evict vì vượt capacity (bounded policy — drop-oldest).
    pub fn dropped(&self) -> usize {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.dropped
    }

    /// Scope của mux (tên file scoped khi tạo qua `create_scoped`).
    pub fn scope(&self) -> &str {
        &self.scope
    }

    /// Capacity ring.
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

/// §50 — scope phải là single filename an toàn, không traversal.
fn validate_scope(scope: &str) -> std::io::Result<()> {
    let ok = !scope.is_empty()
        && scope.len() <= 64
        && scope != "."
        && scope != ".."
        && scope
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.');
    if ok {
        Ok(())
    } else {
        Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("invalid log scope: {scope:?}"),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "antares-logmux-{name}-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn format_utc_ms_known_values() {
        assert_eq!(format_utc_ms(0), "1970-01-01T00:00:00.000Z");
        // 1_700_000_000s = 2023-11-14T22:13:20Z
        assert_eq!(format_utc_ms(1_700_000_000_000), "2023-11-14T22:13:20.000Z");
        assert_eq!(
            format_utc_ms(1_759_438_045_123),
            "2025-10-02T20:47:25.123Z"
        );
    }

    #[test]
    fn record_format_line_has_timestamp_and_stream_tag() {
        let record = LogRecord {
            timestamp_ms: 0,
            stream: LogStream::Stderr,
            line: "boom".into(),
        };
        assert_eq!(
            record.format_line(),
            "1970-01-01T00:00:00.000Z [stderr] boom"
        );
        assert_eq!(LogStream::Stdout.tag(), "stdout");
    }

    #[test]
    fn now_ms_is_plausible() {
        // Sau 2020-01-01, trước 2100.
        let now = now_ms();
        assert!((1_577_836_800_000..4_102_444_800_000).contains(&now));
        let record = LogRecord::now(LogStream::Stdout, "x");
        assert_eq!(record.timestamp_ms, record.timestamp_ms & now); // sanity: same magnitude
        assert!((now - 5_000..=now).contains(&record.timestamp_ms));
    }

    #[test]
    fn ring_is_bounded_drop_oldest_counts_dropped() {
        let mux = LogMux::new("test", 3);
        assert_eq!(mux.capacity(), 3);
        assert_eq!(mux.scope(), "test");
        for i in 0..5 {
            mux.push(LogStream::Stdout, format!("line-{i}"));
        }
        let recent = mux.recent();
        assert_eq!(recent.len(), 3);
        assert_eq!(mux.len(), 3);
        assert_eq!(mux.dropped(), 2);
        // Drop-oldest: giữ 3 dòng CUỐI, theo thứ tự push.
        let lines: Vec<&str> = recent.iter().map(|r| r.line.as_str()).collect();
        assert_eq!(lines, vec!["line-2", "line-3", "line-4"]);
        assert!(recent.iter().all(|r| r.stream == LogStream::Stdout));
    }

    #[test]
    fn capacity_zero_clamps_to_one() {
        let mux = LogMux::new("t", 0);
        assert_eq!(mux.capacity(), 1);
        mux.push(LogStream::Stdout, "a");
        mux.push(LogStream::Stdout, "b");
        assert_eq!(mux.len(), 1);
        assert_eq!(mux.dropped(), 1);
    }

    #[test]
    fn create_scoped_writes_timestamped_lines() {
        let root = temp_root("scoped");
        let mux = LogMux::create_scoped(&root, "inst-1", 16).expect("scoped");
        assert_eq!(mux.scope(), "inst-1");
        mux.push(LogStream::Stdout, "hello");
        mux.push(LogStream::Stderr, "warn");

        let path = root.join("inst-1.log");
        assert!(path.is_file(), "file scoped phải nằm trong root");
        let content = std::fs::read_to_string(&path).expect("read");
        let lines: Vec<&str> = content.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].ends_with(" [stdout] hello"), "{}", lines[0]);
        assert!(lines[1].ends_with(" [stderr] warn"), "{}", lines[1]);
        // Shape ISO: `YYYY-MM-DDTHH:MM:SS.mmmZ` (24 ký tự) + space + `[stream]`
        for line in &lines {
            let prefix = line.get(..24).unwrap_or_else(|| panic!("ts prefix: {line}"));
            assert!(prefix.starts_with('2') && prefix.contains('T') && prefix.ends_with('Z'));
            assert_eq!(line.as_bytes().get(24), Some(&b' '), "space after ts: {line}");
        }

        // Append mode: instance mở lại cùng scope → thêm dòng, không truncate.
        let mux2 = LogMux::create_scoped(&root, "inst-1", 16).expect("reopen");
        mux2.push(LogStream::Stdout, "again");
        let content = std::fs::read_to_string(&path).expect("read2");
        assert_eq!(content.lines().count(), 3);
        assert!(content.lines().next().unwrap().ends_with(" [stdout] hello"));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn create_scoped_rejects_traversal_scope() {
        let root = temp_root("sandbox");
        for bad in ["", ".", "..", "a/b", "a\\b", "../evil", "/abs", "x y", "x".repeat(65).as_str()]
        {
            let err = LogMux::create_scoped(&root, bad, 8)
                .err()
                .unwrap_or_else(|| panic!("scope {bad:?} phải bị reject"));
            assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);
        }
        // Scope hợp lệ đi qua.
        assert!(LogMux::create_scoped(&root, "Inst_1.2026-10-02", 8).is_ok());
        // Không có file escape ra ngoài root.
        assert!(!root.join("..").join("evil.log").exists());
        let _ = std::fs::remove_dir_all(&root);
    }
}
