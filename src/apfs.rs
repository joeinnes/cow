use std::ffi::CString;
use std::path::Path;

/// Returns true if the given path resides on an APFS filesystem.
/// Uses the `statfs(2)` syscall directly to avoid any shell dependency.
pub fn is_apfs(path: &Path) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "macos")]
    fn home_dir_is_apfs() {
        let home = dirs::home_dir().unwrap();
        assert!(is_apfs(&home), "Home directory should be on APFS on a modern Mac");
    }
}
