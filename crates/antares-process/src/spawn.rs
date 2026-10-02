
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

use crate::{ExitState, ProcessRegistry};

pub type OnLine<'a> = &'a mut (dyn FnMut(&str) + 'a);

pub struct SpawnedProcess {
    child: Child,
    stdin: Option<std::process::ChildStdin>,
}

impl SpawnedProcess {
    pub fn pid(&self) -> u32 {
        self.child.id()
    }

    /// Ghi một dòng vào stdin (graceful stop prompt, parity `graceful_stdin`).
    pub fn write_line(&mut self, line: &str) -> std::io::Result<()> {
        match self.stdin.as_mut() {
            Some(stdin) => {
                stdin.write_all(line.as_bytes())?;
                stdin.write_all(b"\n")?;
                stdin.flush()
            }
            None => Err(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "stdin not captured",
            )),
        }
    }

    fn take_stdin(&mut self) -> Option<std::process::ChildStdin> {
        self.stdin.take()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SpawnError {
    #[error("executable not found: {0}")]
    ExecutableNotFound(String),
    #[error("spawn failed: {0}")]
    Io(String),
}

impl SpawnError {
    pub fn code(&self) -> &'static str {
        match self {
            SpawnError::ExecutableNotFound(_) => "PROCESS_EXECUTABLE_NOT_FOUND",
            SpawnError::Io(_) => "PROCESS_SPAWN_FAILED",
        }
    }
}

/// Spawn một process: merge stderr→stdout, capture stdin, không shell.
/// Trả về (handle, pid, reader-thread-closed sentinel không cần — đọc trong `wait`).
///
/// Parity legacy: `CREATE_NO_WINDOW` là Windows-only attribute của subprocess —
/// `std::process::Command` trên Windows 2nd console không mở khi dùng pipes;
/// tạo `CREATE_NO_WINDOW` thực sự cần `windows` crate — để phase 3 khi bundle.
pub fn spawn(
    registry: &mut ProcessRegistry,
    owner: &str,
    instance_id: Option<String>,
    program: &str,
    args: &[String],
    cwd: Option<&str>,
    started_at_unix_ms: u64,
) -> Result<(String, SpawnedProcess, u32), SpawnError> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped()); // legacy: stderr=subprocess.STDOUT — merge khi pump
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }

    let mut child = command.spawn().map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => {
            SpawnError::ExecutableNotFound(program.to_string())
        }
        _ => SpawnError::Io(err.to_string()),
    })?;
    let pid = child.id();
    let stdin = child.stdin.take();
    let id = registry.register(owner, instance_id, format!("{program} {args:?}"), Default::default(), started_at_unix_ms);
    registry.set_pid(&id, pid).expect("just registered");
    Ok((id, SpawnedProcess { child, stdin }, pid))
}

/// Pump stdout cho tới EOF, gộp stderr (legacy `stderr=STDOUT`), mark exit vào registry.
/// Trả exit code. Mỗi dòng được trim `\r\n` rồi đưa vào `on_line`.
pub fn wait(
    id: &str,
    mut handle: SpawnedProcess,
    registry: &mut ProcessRegistry,
    on_line: Option<OnLine<'_>>,
) -> std::io::Result<i32> {
    // Drop stdin trước để child thấy EOF nếu nó đọc stdin.
    handle.take_stdin();

    let mut child = handle.child;
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout piped"));
    let stderr = child.stderr.take().expect("stderr piped");

    // stderr phải được drain song song (nếu không có thể deadlock khi pipe đầy).
    // Legacy merge stderr vào stdout; ở đây drain nền rồi pump stdout chính.
    let stderr_thread = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            let _ = line; // legacy merge → output stream gộp; phase 3: gộp theo timestamp
        }
    });

    if let Some(callback) = on_line {
        let callback = callback;
        let mut buf = String::new();
        loop {
            buf.clear();
            let n = stdout.read_line(&mut buf)?;
            if n == 0 {
                break;
            }
            let line = buf.trim_end_matches(['\r', '\n']);
            callback(line);
        }
    } else {
        // Vẫn phải drain để tránh deadlock pipe.
        for _ in stdout.lines() {}
    }
    stderr_thread.join().expect("stderr drain thread");

    let code = child.wait()?.code().unwrap_or(-1);
    let state = if code == 0 { ExitState::Exited } else { ExitState::Failed };
    let _ = registry.mark_exit(id, state);
    Ok(code)
}

/// Graceful stop: stdin prompt → wait timeout → kill (parity `ProcessManager.stop`).
pub fn stop(
    id: &str,
    handle: &mut SpawnedProcess,
    registry: &mut ProcessRegistry,
    graceful_stdin: Option<&str>,
    timeout: Duration,
) -> std::io::Result<()> {
    if let Some(prompt) = graceful_stdin {
        let _ = handle.write_line(prompt);
    }
    // std không có wait_timeout ổn định cross-platform trong std — chiến lược:
    // vòng poll ngắn `try_wait`, hết hạn thì kill (đúng ngữ nghĩa legacy
    // wait(timeout) → kill()).
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match handle.child.try_wait()? {
            Some(status) => {
                let code = status.code().unwrap_or(-1);
                let state = if code == 0 { ExitState::Exited } else { ExitState::Failed };
                let _ = registry.mark_exit(id, state);
                return Ok(());
            }
            None => {
                if std::time::Instant::now() >= deadline {
                    let _ = handle.child.kill();
                    let _ = handle.child.wait();
                    let _ = registry.mark_exit(id, ExitState::Killed);
                    return Ok(());
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ProcessRegistry;

    fn registry() -> ProcessRegistry {
        ProcessRegistry::default()
    }

    #[cfg(unix)] // `echo`/`sh` là shell builtin/trình riêng unix — Windows không có exe tương ứng
    #[test]
    fn spawn_echo_and_wait_marks_exit() {
        let mut reg = registry();
        let (id, handle, _pid) = spawn(
            &mut reg,
            "test",
            None,
            "echo",
            &["hello-antares".to_string()],
            None,
            1_000,
        )
        .expect("spawn echo");

        let mut lines: Vec<String> = Vec::new();
        let code = wait(&id, handle, &mut reg, Some(&mut |line: &str| {
            lines.push(line.to_string())
        }))
        .expect("wait");
        assert_eq!(code, 0);
        assert_eq!(lines.join("\n"), "hello-antares");
        assert!(matches!(
            reg.get(&id).expect("record").exit_state,
            ExitState::Exited
        ));
    }

    #[test]
    fn spawn_missing_executable_maps_error() {
        let mut reg = registry();
        let err = match spawn(&mut reg, "test", None, "antares-no-such-binary-xyz", &[], None, 1_000) {
            Ok(_) => panic!("spawn must fail for missing executable"),
            Err(err) => err,
        };
        assert_eq!(err.code(), "PROCESS_EXECUTABLE_NOT_FOUND");
    }

    #[cfg(unix)]
    #[test]
    fn nonzero_exit_marks_failed() {
        let mut reg = registry();
        let (id, handle, _) = spawn(
            &mut reg,
            "test",
            None,
            "sh",
            &["-c".to_string(), "exit 3".to_string()],
            None,
            1_000,
        )
        .expect("spawn sh");
        let code = wait(&id, handle, &mut reg, None).expect("wait");
        assert_eq!(code, 3);
        assert!(matches!(
            reg.get(&id).expect("record").exit_state,
            ExitState::Failed
        ));
    }

    #[cfg(unix)]
    #[test]
    fn stop_graceful_via_stdin_prompt() {
        // sh đọc stdin: echo "ready" rồi đọc prompt; nhận "stop" → exit 0.
        let mut reg = registry();
        let (id, mut handle, _) = spawn(
            &mut reg,
            "test",
            None,
            "sh",
            &["-c".to_string(), "echo ready; read line; echo got:$line".to_string()],
            None,
            1_000,
        )
        .expect("spawn sh");

        let mut lines: Vec<String> = Vec::new();
        {
            // Đọc "ready" trước khi stop để chắc child đã lên.
            let child = &mut handle.child;
            let stdout = child.stdout.as_mut().expect("stdout");
            let mut reader = BufReader::new(stdout);
            let mut first = String::new();
            reader.read_line(&mut first).expect("read ready");
            lines.push(first.trim().to_string());
        }
        assert_eq!(lines[0], "ready");

        stop(&id, &mut handle, &mut reg, Some("stop"), Duration::from_secs(5)).expect("stop");
        assert!(matches!(
            reg.get(&id).expect("record").exit_state,
            ExitState::Exited
        ));
    }

    #[cfg(unix)]
    #[test]
    fn stop_kill_after_timeout() {
        let mut reg = registry();
        let (id, mut handle, _) = spawn(
            &mut reg,
            "test",
            None,
            "sh",
            &["-c".to_string(), "sleep 30".to_string()],
            None,
            1_000,
        )
        .expect("spawn sh");
        stop(&id, &mut handle, &mut reg, None, Duration::from_millis(200)).expect("stop kill");
        assert!(matches!(
            reg.get(&id).expect("record").exit_state,
            ExitState::Killed
        ));
    }
}
