//! Batch 07a — free disk space native (parity `shutil.disk_usage` cho
//! preflight check `disk`).
//!
//! §14: platform API isolate tại đây — **không** reintroduce Python/psutil
//! qua "helper tạm". Unix: `statvfs` (libc); Windows: `GetDiskFreeSpaceExW`
//! (kernel32, tự khai extern — 1 API không đáng kéo windows-sys).

use std::path::Path;

/// Trả số MB còn trống (không tính superuser reserve) tại `path` —
/// parity `usage.free // (1024 * 1024)` với `free = f_bavail * f_frsize`.
///
/// Path không tồn tại → `Err` (parity: `shutil.disk_usage` raise → caller
/// preflight downgrade thành warning).
pub fn free_mb(path: &Path) -> std::io::Result<u64> {
    free_bytes(path).map(|bytes| bytes / (1024 * 1024))
}

#[cfg(unix)]
fn free_bytes(path: &Path) -> std::io::Result<u64> {
    use std::os::unix::ffi::OsStrExt;

    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "path contains NUL byte")
    })?;
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: c_path là C string hợp lệ; statvfs chỉ ghi vào `stat`.
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
    if rc != 0 {
        return Err(std::io::Error::last_os_error());
    }
    // f_frsize = fundamental block size (fallback f_bsize — một số FS để 0).
    let block = {
        let frsize = stat.f_frsize as u64;
        if frsize != 0 {
            frsize
        } else {
            stat.f_bsize as u64
        }
    };
    Ok(stat.f_bavail as u64 * block)
}

#[cfg(windows)]
fn free_bytes(path: &Path) -> std::io::Result<u64> {
    use std::os::windows::ffi::OsStrExt;

    // SAFETY: kernel32 luôn được link (std đã link); out-params cấp stack.
    #[link(name = "kernel32")]
    extern "system" {
        fn GetDiskFreeSpaceExW(
            lp_directory_name: *const u16,
            lp_free_bytes_available_to_caller: *mut u64,
            lp_total_number_of_bytes: *mut u64,
            lp_total_number_of_free_bytes: *mut u64,
        ) -> i32;
    }

    let wide: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut free_to_caller: u64 = 0;
    let mut total: u64 = 0;
    let mut total_free: u64 = 0;
    // SAFETY: wide có null terminator; 3 out-ptr đều hợp lệ.
    let ok = unsafe {
        GetDiskFreeSpaceExW(wide.as_ptr(), &mut free_to_caller, &mut total, &mut total_free)
    };
    if ok == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(free_to_caller)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_mb_on_existing_dir_is_positive() {
        // Temp dir luôn tồn tại → parity: trả số MB thật (> 512MB trên CI).
        let dir = std::env::temp_dir();
        let mb = free_mb(&dir).expect("temp dir phải có free space");
        assert!(mb >= 512, "CI/đĩa dev phải còn ≥512MB: {mb}");
    }

    #[test]
    fn free_mb_on_missing_dir_is_err() {
        let missing = std::env::temp_dir().join("antares-disk-missing-nope");
        let err = free_mb(&missing).expect_err("path không tồn tại → Err (parity warning)");
        assert_eq!(err.kind(), std::io::ErrorKind::NotFound);
    }
}
