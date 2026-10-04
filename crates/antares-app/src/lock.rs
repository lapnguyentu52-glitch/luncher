//! B07b — instance `.lock` parity `infrastructure/fs/locks.py::ResourceLock`.
//!
//! Lock file chứa `{pid, timestamp}`; stale = pid không còn sống → steal.
//! Parity `_is_stale`: parse hỏng → stale; pid == mình → không stale;
//! kill(pid, 0) ProcessLookupError → stale; PermissionError → sống.

use std::path::{Path, PathBuf};

pub struct ResourceLock {
    path: PathBuf,
    acquired: bool,
}

impl ResourceLock {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            acquired: false,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Parity `acquire(steal_stale=True)`: lock tồn tại mà stale → steal;
    /// còn sống → `false` (caller map INSTANCE_LOCKED).
    pub fn acquire(&mut self) -> bool {
        if let Some(parent) = self.path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if self.path.exists() {
            if self.is_stale() {
                let _ = std::fs::remove_file(&self.path);
            } else {
                return false;
            }
        }
        let payload = serde_json::json!({
            "pid": std::process::id(),
            "timestamp": now_secs(),
        });
        if std::fs::write(&self.path, payload.to_string()).is_err() {
            return false;
        }
        self.acquired = true;
        true
    }

    /// Parity `release()` — chỉ khi mình là người giữ (idempotent).
    pub fn release(&mut self) {
        if self.acquired {
            let _ = std::fs::remove_file(&self.path);
            self.acquired = false;
        }
    }

    /// Parity `_is_stale`.
    fn is_stale(&self) -> bool {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return true; // đọc lỗi → coi stale (parity except → True)
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
            return true; // hỏng định dạng → stale
        };
        let Some(pid) = value.get("pid").and_then(|p| p.as_u64()) else {
            return true;
        };
        let pid = pid as u32;
        if pid == std::process::id() {
            return false; // mình giữ → không steal
        }
        !pid_alive(pid)
    }
}

impl Drop for ResourceLock {
    fn drop(&mut self) {
        self.release();
    }
}

fn now_secs() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// parity `os.kill(pid, 0)` — sống khi kill thành công hoặc EPERM (có quyền
/// nhưng process của user khác); ESRCH → chết.
#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    let rc = unsafe { libc::kill(pid as libc::pid_t, 0) };
    if rc == 0 {
        return true;
    }
    matches!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::EPERM)
    )
}

/// Windows: OpenProcess — null + ACCESS_DENIED(5) → sống (không có quyền
/// query); null + INVALID_PARAMETER(87) → không tồn tại.
#[cfg(windows)]
fn pid_alive(pid: u32) -> bool {
    const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(desired_access: u32, inherit_handle: i32, pid: u32) -> *mut std::ffi::c_void;
        fn CloseHandle(object: *mut std::ffi::c_void) -> i32;
    }
    // SAFETY: handle hợp lệ hoặc null — luôn CloseHandle khi không null.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            let code = std::io::Error::last_os_error().raw_os_error();
            return code == Some(5); // ERROR_ACCESS_DENIED → process tồn tại
        }
        CloseHandle(handle);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lock_path(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!("antares-lock-{tag}-{}.lock", std::process::id()))
    }

    #[test]
    fn acquire_exclusive_and_release_idempotent() {
        let path = lock_path("basic");
        let _ = std::fs::remove_file(&path);

        let mut first = ResourceLock::new(&path);
        assert!(first.acquire());
        assert!(path.exists());
        let payload: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(payload["pid"], std::process::id());
        assert!(payload["timestamp"].as_f64().unwrap() > 0.0);

        // lock còn sống (pid = mình) → không steal
        let mut second = ResourceLock::new(&path);
        assert!(!second.acquire(), "không steal lock đang sống");

        first.release();
        assert!(!path.exists());
        first.release(); // idempotent
    }

    #[test]
    fn stale_lock_is_stolen() {
        let path = lock_path("stale");
        let _ = std::fs::remove_file(&path);

        // pid của process ĐÃ chết (spawn xong → wait xong).
        let mut child = std::process::Command::new(if cfg!(windows) {
            "cmd"
        } else {
            "true"
        })
        .args(if cfg!(windows) { vec!["/C".to_string(), "exit 0".to_string()] } else { vec![] })
        .spawn()
        .expect("spawn");
        let dead_pid = child.id();
        child.wait().expect("wait");

        std::fs::write(
            &path,
            serde_json::json!({ "pid": dead_pid, "timestamp": 1.0 }).to_string(),
        )
        .unwrap();
        let mut lock = ResourceLock::new(&path);
        assert!(lock.acquire(), "pid chết → steal được");
        let payload: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(payload["pid"], std::process::id(), "đã ghi đè bằng pid mình");
        lock.release();
    }

    #[test]
    fn corrupt_lock_is_stale() {
        let path = lock_path("corrupt");
        let _ = std::fs::remove_file(&path);
        std::fs::write(&path, "not json at all").unwrap();

        let mut lock = ResourceLock::new(&path);
        assert!(lock.acquire(), "json hỏng → stale → steal");
        lock.release();
    }
}
