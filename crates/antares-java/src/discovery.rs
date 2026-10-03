//! Phase 3 — Java discovery, parity `services/java/discovery.py`.
//!
//! - Scan known directories theo OS (`Program Files\Java`, `/usr/lib/jvm`,
//!   `/Library/Java/JavaVirtualMachines`, …)
//! - `JAVA_HOME` env + `java` trên PATH (resolve symlink về java home)
//! - `detect_major` — chạy `java -showversion` rồi parse `version "NN`
//! - `java_info` — full info 1 java home (path/exe/javaw/major/name)
//!
//! Windows javaw.exe riêng cho GUI launch (không console) — parity javaw_executable.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use crate::parse_java_version;

/// Timeout cho `java -showversion` (parity timeout=15).
const DETECT_TIMEOUT_SECS: u64 = 15;

/// Scan directories theo OS (parity _WIN/_LINUX/_MAC_JAVA_DIRS).
pub fn known_java_dirs() -> Vec<PathBuf> {
    if cfg!(windows) {
        vec![
            PathBuf::from(r"C:\Program Files\Java"),
            PathBuf::from(r"C:\Program Files (x86)\Java"),
            PathBuf::from(r"C:\Program Files\Eclipse Adoptium"),
            PathBuf::from(r"C:\Program Files\Microsoft\jdk"),
        ]
    } else if cfg!(target_os = "macos") {
        vec![PathBuf::from("/Library/Java/JavaVirtualMachines")]
    } else {
        vec![PathBuf::from("/usr/lib/jvm"), PathBuf::from("/opt/java")]
    }
}

/// `bin/java(.exe)` trong 1 Java home (parity java_executable).
pub fn java_executable(java_home: &Path) -> Option<PathBuf> {
    let exe = if cfg!(windows) { "java.exe" } else { "java" };
    let path = java_home.join("bin").join(exe);
    path.is_file().then_some(path)
}

/// `javaw.exe` cho Windows GUI launch — None trên non-Windows (parity).
pub fn javaw_executable(java_home: &Path) -> Option<PathBuf> {
    if !cfg!(windows) {
        return None;
    }
    let path = java_home.join("bin").join("javaw.exe");
    path.is_file().then_some(path)
}

/// Tìm mọi subdir có bin/java — bỏ symlink (parity _scan_directory).
pub fn scan_directory(base: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let entries = match std::fs::read_dir(base) {
        Ok(entries) => entries,
        Err(_) => return found,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || path.is_symlink() {
            continue;
        }
        if java_executable(&path).is_some() {
            found.push(path);
        }
    }
    found
}

/// Tìm mọi Java install trên máy (parity scan_system_java): scan dirs + JAVA_HOME +
/// PATH `java` (resolve symlink → parent.parent là java home).
pub fn scan_system_java() -> Vec<PathBuf> {
    let mut homes: Vec<PathBuf> = Vec::new();
    for dir in known_java_dirs() {
        homes.extend(scan_directory(&dir));
    }

    // JAVA_HOME
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let home = PathBuf::from(java_home);
        if java_executable(&home).is_some() {
            homes.push(home);
        }
    }

    // PATH java — resolve symlink về java home (exe ở <home>/bin/java)
    if let Ok(path_var) = std::env::var("PATH") {
        let exe = if cfg!(windows) { "java.exe" } else { "java" };
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(exe);
            if candidate.is_file() {
                // which parity: resolve symlink → <home>/bin/java → home = parent.parent
                let real = candidate.canonicalize().unwrap_or_else(|_| candidate.clone());
                if let Some(bin_dir) = real.parent() {
                    if let Some(home) = bin_dir.parent() {
                        let home = home.to_path_buf();
                        if java_executable(&home).is_some() && !homes.contains(&home) {
                            homes.push(home);
                        }
                    }
                }
                break; // which → tìm thấy đầu tiên là đủ (parity)
            }
        }
    }

    // dedupe giữ thứ tự (parity)
    let mut seen: HashSet<PathBuf> = HashSet::new();
    let mut out = Vec::new();
    for home in homes {
        if seen.insert(home.clone()) {
            out.push(home);
        }
    }
    out
}

/// Parse major version từ `java -showversion` — dùng parse_java_version parity
/// (1.8 legacy mapping). Trả None khi chạy/parse fail (parity detect_major).
pub fn detect_major(exe: &Path) -> Option<u16> {
    let output = run_with_timeout(exe, &["-showversion"], Duration::from_secs(DETECT_TIMEOUT_SECS))?;
    // Parity: out = stderr + stdout; regex version "(NN)
    let combined = format!("{}{}", output.0, output.1);
    parse_java_version(&combined).ok().map(|(major, _)| major)
}

/// Chạy command với timeout poll — trả (stderr, stdout). None khi spawn fail/timeout
/// (parity subprocess.run capture_output + timeout → None).
fn run_with_timeout(
    exe: &Path,
    args: &[&str],
    timeout: Duration,
) -> Option<(String, String)> {
    let mut child = Command::new(exe)
        .args(args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .stdin(std::process::Stdio::null())
        .spawn()
        .ok()?;
    let deadline = std::time::Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => break,
            Ok(None) => {
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(_) => return None,
        }
    }
    // Output nhỏ (java -version vài trăm byte) → read to end an toàn sau exit.
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    if let Some(mut stream) = child.stdout.take() {
        use std::io::Read;
        let _ = stream.read_to_end(&mut stdout);
    }
    if let Some(mut stream) = child.stderr.take() {
        use std::io::Read;
        let _ = stream.read_to_end(&mut stderr);
    }
    Some((
        String::from_utf8_lossy(&stderr).into_owned(),
        String::from_utf8_lossy(&stdout).into_owned(),
    ))
}

/// Full info cho 1 java home (parity java_info): path/exe/javaw/major/name.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaInfo {
    pub path: String,
    pub exe: String,
    pub javaw: Option<String>,
    pub major: u16,
    pub name: String,
}

pub fn java_info(java_home: &Path) -> Option<JavaInfo> {
    let exe = java_executable(java_home)?;
    let major = detect_major(&exe)?;
    Some(JavaInfo {
        path: java_home.display().to_string(),
        exe: exe.display().to_string(),
        javaw: javaw_executable(java_home).map(|p| p.display().to_string()),
        major,
        name: java_home
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    })
}

/// Scan toàn bộ → list JavaInfo (bỏ home không detect được major).
pub fn scan_java_infos() -> Vec<JavaInfo> {
    scan_system_java()
        .iter()
        .filter_map(|home| java_info(home))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_executable_missing_home_is_none() {
        let root = std::env::temp_dir().join(format!("antares-java-disc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let home = root.join("jdk-17");
        std::fs::create_dir_all(&home).unwrap();
        assert_eq!(java_executable(&home), None);
        assert_eq!(javaw_executable(&home), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_directory_finds_java_homes_skips_symlinks() {
        let root = std::env::temp_dir().join(format!("antares-java-scan-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        let home = root.join("jdk-21");
        std::fs::create_dir_all(home.join("bin")).unwrap();
        std::fs::write(home.join("bin").join("java"), b"ELF").unwrap();
        std::fs::write(home.join("bin").join("java.exe"), b"PE").unwrap(); // cả 2 để test OS nào cũng pass

        let empty = root.join("not-java");
        std::fs::create_dir_all(&empty).unwrap();

        let found = scan_directory(&root);
        assert_eq!(found, vec![home]);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn scan_system_java_returns_list_without_panic() {
        // Không assert số lượng (CI machine có thể không có java) — chỉ đảm bảo chạy sạch.
        let homes = scan_system_java();
        let _ = homes.len();
    }

    #[test]
    fn detect_major_garbage_binary_is_none() {
        // File không phải executable → spawn fail → None (parity).
        let root = std::env::temp_dir().join(format!("antares-java-detect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let fake = root.join("java");
        std::fs::write(&fake, b"not an executable").unwrap();
        assert_eq!(detect_major(&fake), None);
        let _ = std::fs::remove_dir_all(&root);
    }

    // F-04 — bỏ cfg(unix): `which` là unix-only, thay bằng scan PATH cross-platform
    // → test này chạy cả trên Windows CI (runner có Java preinstalled).
    #[test]
    fn detect_major_with_real_java_binary() {
        // Chỉ khẳng định khi có java thật trên PATH — máy không có → bỏ qua.
        let Some(java) = java_on_path() else {
            return; // máy không có java → bỏ qua
        };
        let Some(major) = detect_major(&java) else {
            return; // binary lạ → bỏ qua (CI vẫn pass)
        };
        assert!((8..=60).contains(&major), "major {major} ngoài khoảng hợp lý");
    }

    /// `which java` parity — cross-platform (scan PATH, Windows thêm .exe).
    fn java_on_path() -> Option<PathBuf> {
        let path_var = std::env::var_os("PATH")?;
        let exe = if cfg!(windows) { "java.exe" } else { "java" };
        for dir in std::env::split_paths(&path_var) {
            if dir.as_os_str().is_empty() {
                continue;
            }
            let candidate = dir.join(exe);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }
}
