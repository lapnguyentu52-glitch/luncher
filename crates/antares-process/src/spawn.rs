use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

use crate::{CleanupPolicy, ExitState, LogMux, LogStream, ProcessRegistry};

pub type OnLine<'a> = &'a mut (dyn FnMut(&str) + 'a);

pub struct SpawnedProcess {
    child: Child,
    stdin: Option<std::process::ChildStdin>,
    /// F-12 — mux gắn từ `spawn()`: `wait()` pump stdout/stderr vào đây (nếu có).
    mux: Option<Arc<LogMux>>,
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

/// Spawn một process: capture stdin/stdout/stderr, không shell.
///
/// - `cleanup` — §113 policy ghi thẳng vào record (game default `Keep`);
///   không hardcode `Default` nữa để caller (supervisor) quyết định.
/// - `mux` (F-12) — nếu có, `wait()` push từng dòng (timestamp + tag nguồn)
///   vào ring bounded + file scoped của mux.
/// - Windows — `CREATE_NO_WINDOW` (0x0800_0000): parity legacy subprocess
///   cùng tên, không mở console nhấp nháy (pattern `antares-bridge`).
pub fn spawn(
    registry: &mut ProcessRegistry,
    owner: &str,
    instance_id: Option<String>,
    program: &str,
    args: &[String],
    cwd: Option<&str>,
    cleanup: CleanupPolicy,
    mux: Option<Arc<LogMux>>,
    started_at_unix_ms: u64,
) -> Result<(String, SpawnedProcess, u32), SpawnError> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = cwd {
        command.current_dir(dir);
    }
    // F-04 — legacy `CREATE_NO_WINDOW`: std hỗ trợ qua CommandExt (không cần
    // windows crate), y hệt pattern trong crates/antares-bridge/src/bridge.rs.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }

    let mut child = command.spawn().map_err(|err| match err.kind() {
        std::io::ErrorKind::NotFound => SpawnError::ExecutableNotFound(program.to_string()),
        _ => SpawnError::Io(err.to_string()),
    })?;
    let pid = child.id();
    let stdin = child.stdin.take();
    let id = registry.register(
        owner,
        instance_id,
        format!("{program} {args:?}"),
        cleanup,
        started_at_unix_ms,
    );
    registry.set_pid(&id, pid).expect("just registered");
    Ok((id, SpawnedProcess { child, stdin, mux }, pid))
}

/// Pump stdout cho tới EOF, drain stderr song song, mark exit vào registry.
/// Trả exit code. Mỗi dòng được trim `\r\n` rồi xử lý:
///
/// - `on_line` — callback cho dòng **stdout** (parity output stream).
/// - `handle.mux` (F-12) — stdout lẫn stderr được push với `timestamp_ms` +
///   tag `Stdout`/`Stderr` → merge 2 stream vẫn phân biệt được nguồn
///   (legacy `stderr=STDOUT` gộp mù); ring bounded drop-oldest + file scoped.
pub fn wait(
    id: &str,
    mut handle: SpawnedProcess,
    registry: &mut ProcessRegistry,
    mut on_line: Option<OnLine<'_>>,
) -> std::io::Result<i32> {
    // Drop stdin trước để child thấy EOF nếu nó đọc stdin.
    handle.take_stdin();

    let SpawnedProcess {
        child,
        stdin: _,
        mux,
    } = handle;
    let mut child = child;
    let mut stdout = BufReader::new(child.stdout.take().expect("stdout piped"));
    let stderr = child.stderr.take().expect("stderr piped");

    // stderr phải được drain song song (nếu không có thể deadlock khi pipe đầy).
    // Thread riêng giữ Arc clone ('static) — không kéo borrow callback của caller.
    // (Nếu stdout đọc lỗi giữa chừng → return sớm, thread detach như cũ: drain
    //  sẽ kết thúc khi child đóng pipe, không bao giờ join treo.)
    let stderr_mux = mux.clone();
    let stderr_thread = std::thread::spawn(move || {
        let reader = BufReader::new(stderr);
        for line in reader.lines() {
            match line {
                Ok(line) => {
                    if let Some(mux) = &stderr_mux {
                        mux.push(LogStream::Stderr, line);
                    }
                }
                // invalid UTF-8 → dừng drain thread này (không panic lan).
                Err(_) => break,
            }
        }
    });

    let mut buf = String::new();
    loop {
        buf.clear();
        let n = stdout.read_line(&mut buf)?;
        if n == 0 {
            break;
        }
        let line = buf.trim_end_matches(['\r', '\n']);
        if let Some(mux) = &mux {
            mux.push(LogStream::Stdout, line);
        }
        if let Some(callback) = on_line.as_mut() {
            callback(line);
        }
    }
    stderr_thread.join().expect("stderr drain thread");

    let code = child.wait()?.code().unwrap_or(-1);
    let state = if code == 0 {
        ExitState::Exited
    } else {
        ExitState::Failed
    };
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
                let state = if code == 0 {
                    ExitState::Exited
                } else {
                    ExitState::Failed
                };
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
    use std::path::PathBuf;

    fn registry() -> ProcessRegistry {
        ProcessRegistry::default()
    }

    /// F-04 — script cross-platform: unix `sh -c`, windows `cmd /C`.
    /// Mỗi tham số chỉ dùng ở nền tảng của nó (underscore prefix → không warn).
    fn shell_script(_unix: &str, _windows: &str) -> (&'static str, Vec<String>) {
        #[cfg(unix)]
        {
            ("sh", vec!["-c".to_string(), _unix.to_string()])
        }
        #[cfg(windows)]
        {
            ("cmd", vec!["/C".to_string(), _windows.to_string()])
        }
    }

    /// F-04 — spawn executable + stdout capture + exit state — chạy trên cả
    /// Windows CI (bỏ cfg(unix)) + §113 cleanup policy được ghi vào record.
    #[test]
    fn spawn_echo_and_wait_marks_exit() {
        let mut reg = registry();
        let (program, args) = shell_script("echo hello-antares", "echo hello-antares");
        let (id, handle, _pid) = spawn(
            &mut reg,
            "test",
            None,
            &program,
            &args,
            None,
            CleanupPolicy::Wait,
            None,
            1_000,
        )
        .expect("spawn echo");

        // Cleanup policy (§113) do caller quyết định — không còn hardcode default.
        assert_eq!(reg.get(&id).expect("record").cleanup, CleanupPolicy::Wait);

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
        let err = match spawn(
            &mut reg,
            "test",
            None,
            "antares-no-such-binary-xyz",
            &[],
            None,
            CleanupPolicy::Keep,
            None,
            1_000,
        ) {
            Ok(_) => panic!("spawn must fail for missing executable"),
            Err(err) => err,
        };
        assert_eq!(err.code(), "PROCESS_EXECUTABLE_NOT_FOUND");
    }

    /// F-04 stderr capture + F-12 merge: stdout vào callback, cả 2 stream vào
    /// LogMux với tag phân biệt nguồn (legacy merge mù → ở đây có timestamp).
    #[test]
    fn stdout_and_stderr_merge_into_mux_with_tags() {
        let mut reg = registry();
        let (program, args) = shell_script(
            "echo out-line; echo err-line 1>&2",
            "echo out-line & echo err-line 1>&2",
        );
        let mux = Arc::new(LogMux::new("merge-cap", 16));
        let (id, handle, _) = spawn(
            &mut reg,
            "test",
            None,
            &program,
            &args,
            None,
            CleanupPolicy::Keep,
            Some(mux.clone()),
            1_000,
        )
        .expect("spawn");

        let mut lines: Vec<String> = Vec::new();
        let code = wait(&id, handle, &mut reg, Some(&mut |line: &str| {
            lines.push(line.to_string())
        }))
        .expect("wait");
        assert_eq!(code, 0);

        // Callback chỉ nhận stdout (parity on_line) — stderr đi vào mux.
        assert_eq!(lines, vec!["out-line".to_string()]);

        // 2 thread pump song song → thứ tự trong ring không xác định → kiểm `any`.
        let recent = mux.recent();
        assert_eq!(recent.len(), 2, "ring: {recent:?}");
        assert!(recent
            .iter()
            .any(|r| r.stream == LogStream::Stdout && r.line == "out-line"));
        assert!(recent
            .iter()
            .any(|r| r.stream == LogStream::Stderr && r.line == "err-line"));
        assert_eq!(mux.dropped(), 0);
    }

    /// F-04 — non-zero exit → ExitState::Failed (cả 2 shell đều `exit 3`).
    #[test]
    fn nonzero_exit_marks_failed() {
        let mut reg = registry();
        let (program, args) = shell_script("exit 3", "exit 3");
        let (id, handle, _) = spawn(
            &mut reg,
            "test",
            None,
            &program,
            &args,
            None,
            CleanupPolicy::Keep,
            None,
            1_000,
        )
        .expect("spawn shell");
        let code = wait(&id, handle, &mut reg, None).expect("wait");
        assert_eq!(code, 3);
        assert!(matches!(
            reg.get(&id).expect("record").exit_state,
            ExitState::Failed
        ));
    }

    /// F-04 — stdin graceful stop: prompt → child tự thoát trước timeout → Exited.
    ///
    /// - unix: `sh` đọc 1 dòng từ stdin pipe.
    /// - windows: `powershell` đọc 1 dòng qua `[Console]::ReadLine` (cmd `set /p`
    ///   không nhận pipe; `pause` không chắc khi pipe chưa EOF) — nếu prompt
    ///   không tới tay child, ReadLine treo → bị kill → test fail.
    #[test]
    fn stop_graceful_via_stdin_prompt() {
        #[cfg(unix)]
        let (program, args) = ("sh", vec!["-c".to_string(), "read line".to_string()]);
        #[cfg(windows)]
        let (program, args) = (
            "powershell",
            vec![
                "-NoProfile".to_string(),
                "-Command".to_string(),
                "$null = [Console]::ReadLine(); exit 0".to_string(),
            ],
        );

        let mut reg = registry();
        let (id, mut handle, _) = spawn(
            &mut reg,
            "test",
            None,
            &program,
            &args,
            None,
            CleanupPolicy::Wait,
            None,
            1_000,
        )
        .expect("spawn reader");

        // Startup powershell trên CI ~1-2s → timeout rộng, nhưng stop poll
        // ngay khi child thoát nên test chạy nhanh khi pass.
        stop(
            &id,
            &mut handle,
            &mut reg,
            Some("stop"),
            Duration::from_secs(15),
        )
        .expect("stop graceful");
        assert!(matches!(
            reg.get(&id).expect("record").exit_state,
            ExitState::Exited
        ));
    }

    /// F-04 — forced kill: quá timeout → kill → ExitState::Killed.
    /// Dùng binary thật (không qua shell) để kill đúng process vừa spawn —
    /// Windows không kill process tree → tránh orphan còn sót sau shell.
    #[test]
    fn stop_kill_after_timeout() {
        #[cfg(unix)]
        let (program, args) = ("sleep", vec!["30".to_string()]);
        #[cfg(windows)]
        let (program, args) = (
            "ping",
            vec![
                "-n".to_string(),
                "31".to_string(),
                "127.0.0.1".to_string(),
            ],
        );

        let mut reg = registry();
        let (id, mut handle, _) = spawn(
            &mut reg,
            "test",
            None,
            &program,
            &args,
            None,
            CleanupPolicy::Kill,
            None,
            1_000,
        )
        .expect("spawn long-running");
        stop(&id, &mut handle, &mut reg, None, Duration::from_millis(200))
            .expect("stop kill");
        assert!(matches!(
            reg.get(&id).expect("record").exit_state,
            ExitState::Killed
        ));
    }

    /// F-04 — cwd chứa space được áp thật: shell in cwd → so sánh sau
    /// canonicalize (pwd/logical path có thể khác dạng string).
    #[test]
    fn cwd_with_spaces_is_applied() {
        let root =
            std::env::temp_dir().join(format!("antares-cwd-space-{}", std::process::id()));
        let dir = root.join("dir with spaces");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&dir).expect("mkdir");

        let mut reg = registry();
        let (program, args) = shell_script("pwd", "cd");
        let (id, handle, _) = spawn(
            &mut reg,
            "test",
            None,
            &program,
            &args,
            dir.to_str(),
            CleanupPolicy::Keep,
            None,
            1_000,
        )
        .expect("spawn with cwd");

        let mut lines: Vec<String> = Vec::new();
        let code = wait(&id, handle, &mut reg, Some(&mut |line: &str| {
            lines.push(line.to_string())
        }))
        .expect("wait");
        assert_eq!(code, 0);

        let out = lines.join("\n").trim().to_string();
        let expected = dir.canonicalize().expect("canonical dir");
        let actual = PathBuf::from(&out).canonicalize().expect("canonical cwd output");
        assert_eq!(actual, expected);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// F-04 — cwd unicode (kèm space) — spawn + child chạy được.
    /// Chỉ assert cwd đầy đủ khi shell trả UTF-8 (unix); cmd (Windows) in ra
    /// theo OEM codepage khác → không đọc lại path unicode qua cmd.
    #[test]
    fn cwd_with_unicode_path_runs() {
        let root =
            std::env::temp_dir().join(format!("antares-cwd-uni-{}", std::process::id()));
        let dir = root.join("thế giới ünï ❤");
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&dir).expect("mkdir");

        let mut reg = registry();
        let (program, args) = shell_script("pwd", "echo cwd-ok");
        let (id, handle, _) = spawn(
            &mut reg,
            "test",
            None,
            &program,
            &args,
            dir.to_str(),
            CleanupPolicy::Keep,
            None,
            1_000,
        )
        .expect("spawn unicode cwd");

        let mut lines: Vec<String> = Vec::new();
        let code = wait(&id, handle, &mut reg, Some(&mut |line: &str| {
            lines.push(line.to_string())
        }))
        .expect("wait");
        assert_eq!(code, 0);

        #[cfg(unix)]
        {
            let out = lines.join("\n").trim().to_string();
            let expected = dir.canonicalize().expect("canonical dir");
            let actual = PathBuf::from(&out).canonicalize().expect("canonical cwd output");
            assert_eq!(actual, expected);
        }
        #[cfg(windows)]
        {
            assert_eq!(lines, vec!["cwd-ok".to_string()]);
        }

        let _ = std::fs::remove_dir_all(&root);
    }
}
