
use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde::Serialize;

pub mod logmux;
pub mod spawn;

pub use logmux::{LogMux, LogRecord, LogStream};
pub use spawn::{spawn, spawn_with_secrets, stop, wait, wait_detached, SpawnError, SpawnedProcess};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRecord {
    pub id: String,
    pub pid: Option<u32>,
    pub owner: String,
    pub instance_id: Option<String>,
    pub started_at_unix_ms: u64,
    pub command_fingerprint: String,
    pub cleanup: CleanupPolicy,
    #[serde(skip)]
    pub started_at: Option<Instant>,
    pub exit_state: ExitState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ExitState {
    Running,
    Exited,
    Killed,
    Failed,
}

/// §113 — Launcher close → child cleanup policy. Không kill process user-owned ngoài scope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CleanupPolicy {
    /// Chờ child tự thoát.
    Wait,
    /// Kill khi launcher close.
    Kill,
    /// Giữ nguyên — game tiếp tục chạy độc lập (default cho Minecraft).
    Keep,
}

impl Default for CleanupPolicy {
    fn default() -> Self {
        CleanupPolicy::Keep
    }
}

/// §113 — không kill process ngoài scope: chỉ những record do supervisor này spawn.
pub struct ProcessRegistry {
    processes: HashMap<String, ProcessRecord>,
    seq: u64,
    /// Sweep các entry terminal cũ hơn ttl này.
    retention: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProcessError {
    #[error("unknown process: {0}")]
    Unknown(String),
    #[error("process {0} is still running")]
    StillRunning(String),
}

impl ProcessError {
    pub fn code(&self) -> &'static str {
        match self {
            ProcessError::Unknown(_) => "PROCESS_NOT_FOUND",
            ProcessError::StillRunning(_) => "PROCESS_STILL_RUNNING",
        }
    }
}

impl Default for ProcessRegistry {
    fn default() -> Self {
        Self::new(Duration::from_secs(60))
    }
}

impl ProcessRegistry {
    pub fn new(retention: Duration) -> Self {
        Self {
            processes: HashMap::new(),
            seq: 0,
            retention,
        }
    }

    /// Đăng ký một process do supervisor spawn.
    pub fn register(
        &mut self,
        owner: impl Into<String>,
        instance_id: Option<String>,
        command_fingerprint: impl Into<String>,
        cleanup: CleanupPolicy,
        started_at_unix_ms: u64,
    ) -> String {
        self.seq += 1;
        let id = format!("proc_{}", self.seq);
        self.processes.insert(
            id.clone(),
            ProcessRecord {
                id: id.clone(),
                pid: None,
                owner: owner.into(),
                instance_id,
                started_at_unix_ms,
                command_fingerprint: command_fingerprint.into(),
                cleanup,
                started_at: Some(Instant::now()),
                exit_state: ExitState::Running,
            },
        );
        id
    }

    pub fn get(&self, id: &str) -> Option<&ProcessRecord> {
        self.processes.get(id)
    }

    pub fn set_pid(&mut self, id: &str, pid: u32) -> Result<(), ProcessError> {
        let record = self
            .processes
            .get_mut(id)
            .ok_or_else(|| ProcessError::Unknown(id.to_string()))?;
        record.pid = Some(pid);
        Ok(())
    }

    /// Ghi nhận exit. `Ok(false)` = record không còn Running (đã mark trước đó).
    pub fn mark_exit(&mut self, id: &str, state: ExitState) -> Result<bool, ProcessError> {
        let record = self
            .processes
            .get_mut(id)
            .ok_or_else(|| ProcessError::Unknown(id.to_string()))?;
        if record.exit_state != ExitState::Running {
            return Ok(false);
        }
        record.exit_state = state;
        Ok(true)
    }

    /// Các process còn Running — dùng khi launcher close để áp cleanup policy (§113).
    pub fn running(&self) -> Vec<&ProcessRecord> {
        let mut running: Vec<&ProcessRecord> = self
            .processes
            .values()
            .filter(|p| p.exit_state == ExitState::Running)
            .collect();
        running.sort_by(|a, b| a.id.cmp(&b.id));
        running
    }

    /// Plan cleanup khi launcher close: chỉ đụng process supervisor-owned (§113).
    /// Trả về danh sách id cần kill theo policy.
    pub fn shutdown_plan(&self) -> Vec<String> {
        self.running()
            .iter()
            .filter(|p| p.cleanup == CleanupPolicy::Kill)
            .map(|p| p.id.clone())
            .collect()
    }

    /// Sweep entry terminal cũ hơn retention — không giữ ref vô hạn (§88 no memory leak).
    pub fn sweep(&mut self) -> usize {
        let retention = self.retention;
        let now = Instant::now();
        let before = self.processes.len();
        self.processes.retain(|_, record| {
            if record.exit_state == ExitState::Running {
                return true;
            }
            match record.started_at {
                Some(started) => now.duration_since(started) < retention,
                None => true, // không biết thời điểm → giữ
            }
        });
        before - self.processes.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_exit_flow() {
        let mut registry = ProcessRegistry::default();
        let id = registry.register(
            "minecraft",
            Some("inst-1".into()),
            "java -jar game.jar",
            CleanupPolicy::Keep,
            1_000,
        );
        assert_eq!(id, "proc_1");

        registry.set_pid(&id, 4242).unwrap();
        let record = registry.get(&id).unwrap();
        assert_eq!(record.pid, Some(4242));
        assert_eq!(record.instance_id.as_deref(), Some("inst-1"));
        assert_eq!(record.exit_state, ExitState::Running);

        assert!(registry.mark_exit(&id, ExitState::Exited).unwrap());
        // double exit → false (idempotent)
        assert!(!registry.mark_exit(&id, ExitState::Exited).unwrap());
        assert!(registry.mark_exit("proc_nope", ExitState::Exited).is_err());
    }

    #[test]
    fn shutdown_plan_only_kills_policy_kill() {
        let mut registry = ProcessRegistry::default();
        let _kill_me = registry.register("tool", None, "cmd-a", CleanupPolicy::Kill, 1_000);
        let _keep_me =
            registry.register("minecraft", None, "cmd-b", CleanupPolicy::Keep, 1_001);
        let _wait_me = registry.register("daemon", None, "cmd-c", CleanupPolicy::Wait, 1_002);

        let plan = registry.shutdown_plan();
        assert_eq!(plan, vec!["proc_1"]);

        // Exit rồi thì không còn trong plan.
        registry.mark_exit("proc_1", ExitState::Killed).unwrap();
        assert!(registry.shutdown_plan().is_empty());
    }

    #[test]
    fn sweep_removes_only_old_terminal_entries() {
        let mut registry = ProcessRegistry::new(Duration::from_millis(0));
        let done = registry.register("tool", None, "cmd", CleanupPolicy::Wait, 1_000);
        registry.mark_exit(&done, ExitState::Exited).unwrap();
        // running không bao giờ bị sweep
        let still = registry.register("game", None, "cmd", CleanupPolicy::Keep, 1_001);

        assert_eq!(registry.sweep(), 1);
        assert!(registry.get(&done).is_none());
        assert!(registry.get(&still).is_some());
    }
}
