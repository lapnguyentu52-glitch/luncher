//! B07b — system probe native §14 (parity `psutil`/`os.cpu_count` của legacy
//! `_low_end()` trong jvm.py). Không reintroduce Python/psutil.

/// Parity `os.cpu_count()`.
pub fn cpu_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(2)
}

/// Tổng RAM (MB) — parity `psutil.virtual_memory().total`.
/// Không probe được → `u64::MAX` (coi như KHÔNG low-end thay vì fail-closed
/// vô lý; chỉ ảnh hưởng chọn GC preset).
pub fn total_memory_mb() -> u64 {
    total_memory_bytes()
        .map(|bytes| bytes / (1024 * 1024))
        .unwrap_or(u64::MAX)
}

/// Parity `_low_end()`: total RAM ≤ 4GiB hoặc cpu ≤ 2 (psutil có trong
/// requirements → nhánh try đầy đủ, không fallback import-fail).
pub fn is_low_end() -> bool {
    total_memory_mb() <= 4 * 1024 || cpu_count() <= 2
}

#[cfg(target_os = "linux")]
fn total_memory_bytes() -> Option<u64> {
    // sysconf(_SC_PHYS_PAGES) * page size — không cần crate ngoài.
    let pages = unsafe { libc::sysconf(libc::_SC_PHYS_PAGES) };
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if pages <= 0 || page_size <= 0 {
        return None;
    }
    Some(pages as u64 * page_size as u64)
}

#[cfg(target_os = "macos")]
fn total_memory_bytes() -> Option<u64> {
    // macOS: sysctlbyname("hw.memsize") — _SC_PHYS_PAGES không có trên Darwin.
    let mut total: u64 = 0;
    let mut len = std::mem::size_of::<u64>();
    let name = std::ffi::CString::new("hw.memsize").expect("static");
    let rc = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            &mut total as *mut u64 as *mut libc::c_void,
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    };
    (rc == 0).then_some(total)
}

#[cfg(windows)]
fn total_memory_bytes() -> Option<u64> {
    // GlobalMemoryStatusEx (kernel32) — 1 API, tự khai extern như disk.rs.
    #[repr(C)]
    struct MemoryStatusEx {
        length: u32,
        memory_load: u32,
        total_phys: u64,
        avail_phys: u64,
        total_page_file: u64,
        avail_page_file: u64,
        total_virtual: u64,
        avail_virtual: u64,
        avail_extended_virtual: u64,
    }
    #[link(name = "kernel32")]
    extern "system" {
        fn GlobalMemoryStatusEx(buffer: *mut MemoryStatusEx) -> i32;
    }
    let mut status = MemoryStatusEx {
        length: std::mem::size_of::<MemoryStatusEx>() as u32,
        memory_load: 0,
        total_phys: 0,
        avail_phys: 0,
        total_page_file: 0,
        avail_page_file: 0,
        total_virtual: 0,
        avail_virtual: 0,
        avail_extended_virtual: 0,
    };
    let ok = unsafe { GlobalMemoryStatusEx(&mut status) };
    (ok != 0).then_some(status.total_phys)
}

#[cfg(not(any(unix, windows)))]
fn total_memory_bytes() -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probes_return_sane_values() {
        assert!(cpu_count() >= 1);
        // CI/dev máy ảo cũng phải ≥ 512MB thật — MAX chỉ khi probe fail.
        let mem = total_memory_mb();
        assert!(mem == u64::MAX || mem >= 512, "mem bất thường: {mem}");
        // is_low_end chỉ là boolean hợp lệ (không panic/overflow)
        let _ = is_low_end();
    }
}
