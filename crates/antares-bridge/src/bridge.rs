use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use parking_lot::{Mutex, RwLock};

use crate::error::{BridgeError, BridgeResult};
use crate::protocol::{
    BridgeNotification, BridgeRequest, BridgeResponse, VersionInfo, PROTOCOL_VERSION,
};

const DEFAULT_TIMEOUT_MS: u64 = 10_000;
const MAX_PAYLOAD_BYTES: usize = 8 * 1024 * 1024; // §31: oversized payload → reject

struct PendingCall {
    respond_to: Sender<BridgeResult<BridgeResponse>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BridgeStatus {
    Stopped,
    Starting,
    Ready,
    Failed(String),
}

/// §43/§234 — LegacyBridge: quản lý sidecar process + JSON Lines stdio protocol.
pub struct LegacyBridge {
    program: Mutex<Option<String>>,
    status: RwLock<BridgeStatus>,
    version: RwLock<Option<VersionInfo>>,
    child: Mutex<Option<Child>>,
    stdin: Mutex<Option<std::process::ChildStdin>>,
    pending: Arc<Mutex<HashMap<String, PendingCall>>>,
    seq: AtomicU64,
    notifications_tx: Sender<BridgeNotification>,
    notifications_rx: Mutex<Option<Receiver<BridgeNotification>>>,
    crash_count: AtomicU64,
}

impl LegacyBridge {
    /// Tạo bridge nhưng chưa spawn. `program` = command chạy sidecar
    /// (vd: `legacy/python/run_sidecar.py`, hoặc `antares-legacy.exe` sau bundle).
    /// Notifications đọc qua [`take_notifications`].
    pub fn new() -> Arc<Self> {
        let (tx, rx) = channel();
        Arc::new(Self {
            program: Mutex::new(None),
            status: RwLock::new(BridgeStatus::Stopped),
            version: RwLock::new(None),
            child: Mutex::new(None),
            stdin: Mutex::new(None),
            pending: Arc::new(Mutex::new(HashMap::new())),
            seq: AtomicU64::new(0),
            notifications_tx: tx,
            notifications_rx: Mutex::new(Some(rx)),
            crash_count: AtomicU64::new(0),
        })
    }

    pub fn set_program(&self, program: impl Into<String>) {
        *self.program.lock() = Some(program.into());
    }

    pub fn program(&self) -> Option<String> {
        self.program.lock().clone()
    }

    pub fn status(&self) -> BridgeStatus {
        self.status.read().clone()
    }

    pub fn version(&self) -> Option<VersionInfo> {
        self.version.read().clone()
    }

    pub fn crash_count(&self) -> u64 {
        self.crash_count.load(Ordering::Relaxed)
    }

    fn next_id(&self) -> String {
        format!("req-{}", self.seq.fetch_add(1, Ordering::Relaxed))
    }

    /// §234 — Spawn sidecar + version handshake. Trả VersionInfo nếu handshake OK.
    pub fn start(&self) -> BridgeResult<VersionInfo> {
        let program = {
            let guard = self.program.lock();
            match guard.as_ref() {
                Some(p) => p.clone(),
                None => return Err(BridgeError::NotFound("program not set".into())),
            }
        };

        {
            let mut status = self.status.write();
            if matches!(&*status, BridgeStatus::Ready | BridgeStatus::Starting) {
                return match self.version.read().clone() {
                    Some(v) => Ok(v),
                    None => Err(BridgeError::NotRunning),
                };
            }
            *status = BridgeStatus::Starting;
        }

        log::info!("spawning legacy sidecar: {program}");
        // Program có thể là single path (production: sidecar exe) hoặc
        // "python3 <script>" (dev/test override) → tách whitespace khi không
        // phải file tồn tại nguyên khối.
        let mut cmd = if std::path::Path::new(&program).exists() {
            Command::new(&program)
        } else {
            let mut parts = program.split_whitespace();
            let exe = parts.next().unwrap_or(&program);
            let mut c = Command::new(exe);
            c.args(parts);
            c
        };
        let mut child = cmd
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|source| {
                *self.status.write() = BridgeStatus::Failed(source.to_string());
                BridgeError::Spawn { source }
            })?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| BridgeError::Protocol("no stdin".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| BridgeError::Protocol("no stdout".into()))?;

        {
            *self.child.lock() = Some(child);
            *self.stdin.lock() = Some(stdin);
        }

        self.spawn_reader(stdout);

        // §234 — version handshake với timeout
        let info: VersionInfo = serde_json::from_value(
            self.call_internal(
                crate::protocol::methods::VERSION,
                serde_json::json!({}),
                Duration::from_millis(DEFAULT_TIMEOUT_MS),
            )?,
        )
        .map_err(|err| BridgeError::Protocol(err.to_string()))?;

        if info.protocol != PROTOCOL_VERSION {
            self.kill_internal();
            *self.status.write() = BridgeStatus::Failed(format!(
                "protocol mismatch: sidecar {}, launcher {PROTOCOL_VERSION}",
                info.protocol
            ));
            return Err(BridgeError::Protocol(format!(
                "version mismatch: sidecar {} != {PROTOCOL_VERSION}",
                info.protocol
            )));
        }

        *self.version.write() = Some(info.clone());
        *self.status.write() = BridgeStatus::Ready;
        log::info!("legacy sidecar ready: {} v{}", info.service, info.service_version);
        Ok(info)
    }

    fn spawn_reader(&self, stdout: std::process::ChildStdout) {
        let pending = Arc::clone(&self.pending);
        let tx = self.notifications_tx.clone();
        std::thread::Builder::new()
            .name("antares-bridge-reader".into())
            .spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    if line.len() > MAX_PAYLOAD_BYTES {
                        // §31: oversized → reject line này, vẫn tiếp tục đọc
                        log::warn!("bridge line oversized ({} bytes), rejected", line.len());
                        continue;
                    }
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    // Response hay notification?
                    let parsed: Result<serde_json::Value, _> = serde_json::from_str(trimmed);
                    match parsed {
                        Ok(value) => {
                            if value.get("id").is_some() && value.get("ok").is_some() {
                                if let Ok(response) =
                                    serde_json::from_value::<BridgeResponse>(value)
                                {
                                    let pending_call =
                                        pending.lock().remove(&response.id);
                                    if let Some(call) = pending_call {
                                        let result = if response.ok {
                                            Ok(response)
                                        } else {
                                            let (code, message) = response
                                                .error
                                                .map(|e| (e.code, e.message))
                                                .unwrap_or_else(|| {
                                                    ("IPC_REMOTE_ERROR".into(), "unknown".into())
                                                });
                                            Err(BridgeError::Remote { code, message })
                                        };
                                        let _ = call.respond_to.send(result);
                                    }
                                }
                            } else if let Ok(notification) =
                                serde_json::from_value::<BridgeNotification>(value)
                            {
                                let _ = tx.send(notification);
                            }
                        }
                        Err(err) => {
                            log::warn!("bridge line not JSON: {err}");
                        }
                    }
                }
                // stdout đóng = sidecar thoát — crash detection ở wait/restart layer
            })
            .expect("spawn bridge reader");
    }

    fn call_internal(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout: Duration,
    ) -> BridgeResult<serde_json::Value> {
        let id = self.next_id();
        let request = BridgeRequest {
            id: id.clone(),
            method: method.to_string(),
            params,
        };
        let line = serde_json::to_string(&request)
            .map_err(|err| BridgeError::Protocol(err.to_string()))?;

        let (respond_tx, respond_rx) = channel();
        {
            let mut pending = self.pending.lock();
            pending.insert(id.clone(), PendingCall { respond_to: respond_tx });
        }

        {
            let mut stdin_guard = self.stdin.lock();
            let Some(stdin) = stdin_guard.as_mut() else {
                self.pending.lock().remove(&id);
                return Err(BridgeError::NotRunning);
            };
            if stdin.write_all(line.as_bytes()).is_err()
                || stdin.write_all(b"\n").is_err()
                || stdin.flush().is_err()
            {
                drop(stdin_guard);
                self.pending.lock().remove(&id);
                *self.status.write() = BridgeStatus::Failed("stdin write failed".into());
                return Err(BridgeError::NotRunning);
            }
        }

        match respond_rx.recv_timeout(timeout) {
            Ok(result) => result.map(|response| response.data),
            Err(_) => {
                self.pending.lock().remove(&id);
                Err(BridgeError::Timeout {
                    method: method.to_string(),
                    timeout_ms: timeout.as_millis() as u64,
                })
            }
        }
    }

    /// Public call với default timeout. Không tự restart — caller quyết định.
    pub fn call(&self, method: &str, params: serde_json::Value) -> BridgeResult<serde_json::Value> {
        if !matches!(self.status(), BridgeStatus::Ready) {
            return Err(BridgeError::NotRunning);
        }
        self.call_internal(method, params, Duration::from_millis(DEFAULT_TIMEOUT_MS))
    }

    /// Call với custom timeout (cho op dài như build pack).
    pub fn call_with_timeout(
        &self,
        method: &str,
        params: serde_json::Value,
        timeout_ms: u64,
    ) -> BridgeResult<serde_json::Value> {
        if !matches!(self.status(), BridgeStatus::Ready) {
            return Err(BridgeError::NotRunning);
        }
        self.call_internal(method, params, Duration::from_millis(timeout_ms))
    }

    /// §234 — Crash detection: nếu process đã thoát, mark failed + tăng crash_count.
    pub fn check_health(&self) -> BridgeStatus {
        let mut child_guard = self.child.lock();
        if let Some(child) = child_guard.as_mut() {
            match child.try_wait() {
                Ok(Some(exit)) => {
                    // `signal()` chỉ có trên unix (ExitStatusExt) — Windows chỉ log code.
                    #[cfg(unix)]
                    let signal = {
                        use std::os::unix::process::ExitStatusExt;
                        exit.signal()
                    };
                    #[cfg(not(unix))]
                    let signal: Option<i32> = None;
                    let detail = format!(
                        "sidecar exited: code={:?}, signal={signal:?}",
                        exit.code()
                    );
                    log::warn!("bridge crash detection: {detail}");
                    *self.status.write() = BridgeStatus::Failed(detail.clone());
                    self.crash_count.fetch_add(1, Ordering::Relaxed);
                    // dọn pending calls đang treo
                    let mut pending = self.pending.lock();
                    for (_, call) in pending.drain() {
                        let _ = call.respond_to.send(Err(BridgeError::NotRunning));
                    }
                    BridgeStatus::Failed(detail)
                }
                Ok(None) => self.status(),
                Err(err) => BridgeStatus::Failed(err.to_string()),
            }
        } else {
            self.status()
        }
    }

    /// §234 — Restart: kill hiện tại (nếu có) rồi start lại với handshake mới.
    pub fn restart(&self) -> BridgeResult<VersionInfo> {
        self.kill_internal();
        *self.child.lock() = None;
        *self.stdin.lock() = None;
        self.start()
    }

    /// §234 — Graceful shutdown: gửi health.shutdown, đợi exit, kill nếu cần.
    pub fn shutdown(&self, grace: Duration) -> BridgeResult<()> {
        if matches!(self.status(), BridgeStatus::Ready) {
            // best-effort graceful — không xử lý kết quả
            let _ = self.call_internal(
                crate::protocol::methods::SHUTDOWN,
                serde_json::json!({}),
                Duration::from_millis(1_000),
            );
            std::thread::sleep(Duration::from_millis(100));
        }
        self.kill_internal_with_wait(grace);
        *self.status.write() = BridgeStatus::Stopped;
        *self.version.write() = None;
        Ok(())
    }

    fn kill_internal(&self) {
        self.kill_internal_with_wait(Duration::from_millis(500));
    }

    fn kill_internal_with_wait(&self, grace: Duration) {
        let mut child_guard = self.child.lock();
        if let Some(child) = child_guard.as_mut() {
            let started = std::time::Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(_)) => break,
                    Ok(None) => {
                        if started.elapsed() > grace {
                            let _ = child.kill();
                            let _ = child.wait();
                            break;
                        }
                        std::thread::sleep(Duration::from_millis(25));
                    }
                    Err(_) => break,
                }
            }
        }
        *child_guard = None;
        *self.stdin.lock() = None;
    }
}

/// Lấy notification (non-blocking) — src-tauri bridge forwarder dùng.
pub fn take_notifications(bridge: &LegacyBridge) -> Vec<BridgeNotification> {
    let rx_guard = bridge.notifications_rx.lock();
    if let Some(rx) = rx_guard.as_ref() {
        let mut out = Vec::new();
        while let Ok(notification) = rx.try_recv() {
            out.push(notification);
        }
        out
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Python mock sidecar cho integration test — trả version handshake + echo.
    const MOCK_SIDECAR: &str = r#"
import json, sys, platform
for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    req = json.loads(line)
    if req.get("method") == "health.version":
        out = {"id": req["id"], "ok": True, "data": {
            "protocol": 1, "service": "antares-legacy",
            "serviceVersion": "4.0.0", "python": platform.python_version()}}
    elif req.get("method") == "health.ping":
        out = {"id": req["id"], "ok": True, "data": {"pong": True}}
    elif req.get("method") == "echo":
        out = {"id": req["id"], "ok": True, "data": req.get("params", {})}
    elif req.get("method") == "health.shutdown":
        out = {"id": req["id"], "ok": True, "data": {"stopping": True}}
        sys.stdout.write(json.dumps(out) + "\n"); sys.stdout.flush()
        sys.exit(0)
    else:
        out = {"id": req["id"], "ok": False, "error": {"code": "METHOD_NOT_FOUND", "message": req.get("method", "")}}
    sys.stdout.write(json.dumps(out) + "\n")
    sys.stdout.flush()
"#;

    #[test]
    fn handshake_call_and_shutdown_roundtrip() {
        let dir = std::env::temp_dir().join(format!("antares-bridge-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let script = dir.join("mock_sidecar.py");
        std::fs::write(&script, MOCK_SIDECAR).expect("write mock");

        let bridge = LegacyBridge::new();
        bridge.set_program(format!("python3 {}", script.display()));

        let info = bridge.start().expect("handshake");
        assert_eq!(info.protocol, PROTOCOL_VERSION);
        assert_eq!(info.service, "antares-legacy");
        assert_eq!(bridge.status(), BridgeStatus::Ready);

        let pong = bridge.call(crate::protocol::methods::PING, serde_json::json!({})).expect("ping");
        assert_eq!(pong["pong"], true);

        let echoed = bridge.call("echo", serde_json::json!({"x": 42})).expect("echo");
        assert_eq!(echoed["x"], 42);

        let err = bridge.call("no.such.method", serde_json::json!({})).expect_err("should err");
        match err {
            BridgeError::Remote { code, .. } => assert_eq!(code, "METHOD_NOT_FOUND"),
            other => panic!("unexpected error: {other:?}"),
        }

        bridge.shutdown(Duration::from_secs(3)).expect("shutdown");
        assert_eq!(bridge.status(), BridgeStatus::Stopped);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn not_running_call_errors() {
        let bridge = LegacyBridge::new();
        assert!(matches!(
            bridge.call("health.ping", serde_json::json!({})),
            Err(BridgeError::NotRunning)
        ));
    }

    #[test]
    fn program_not_set_errors() {
        let bridge = LegacyBridge::new();
        assert!(matches!(bridge.start(), Err(BridgeError::NotFound(_))));
    }
}
