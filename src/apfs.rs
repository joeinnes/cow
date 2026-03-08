use std::path::Path;

/// Returns true if the given path resides on an APFS filesystem (macOS only).
/// On other platforms, always returns true so the check is a no-op.
#[cfg(target_os = "macos")]
pub fn is_apfs(path: &Path) -> bool {
    use std::ffi::CString;

    // Allow test harnesses to simulate a non-APFS environment without
    // needing a real non-APFS volume.
    if std::env::var_os("COW_TEST_NOT_APFS").is_some() {
        return false;
    }

    let path_str = match path.to_str() {
        Some(s) => s,
        // tarpaulin-ignore-start
        None => return false,
        // tarpaulin-ignore-end
    };
    let c_path = match CString::new(path_str) {
        Ok(s) => s,
        // tarpaulin-ignore-start
        Err(_) => return false,
        // tarpaulin-ignore-end
    };

    let mut stat_buf: libc::statfs = unsafe { std::mem::zeroed() };
    let ret = unsafe { libc::statfs(c_path.as_ptr(), &mut stat_buf) };
    // tarpaulin-ignore-start
    if ret != 0 {
        return false;
    }
    // tarpaulin-ignore-end

    // f_fstypename is [c_char; 16] on macOS
    let ftype = unsafe {
        let ptr = stat_buf.f_fstypename.as_ptr();
        std::ffi::CStr::from_ptr(ptr).to_str().unwrap_or("")
    };

    ftype == "apfs"
}

#[cfg(not(target_os = "macos"))]
pub fn is_apfs(_path: &Path) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn home_dir_is_apfs() {
        let home = dirs::home_dir().unwrap();
        assert!(is_apfs(&home), "Home directory should be on APFS on a modern Mac");
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn returns_false_when_env_var_set() {
        // Uses a temp dir to avoid touching the real FS.
        let dir = tempfile::TempDir::new().unwrap();
        // Set for this check, then immediately clear so parallel tests are unaffected.
        std::env::set_var("COW_TEST_NOT_APFS", "1");
        let result = is_apfs(dir.path());
        std::env::remove_var("COW_TEST_NOT_APFS");
        assert!(!result);
    }
}
