//! F9 disk-full gate before a helper commit batch.

use std::path::Path;

/// Pause indexing when free space is below 500 MB or 5% of the volume.
pub fn too_full(path: &Path) -> Option<String> {
    let mut stat = unsafe { std::mem::zeroed::<libc::statvfs>() };
    let c_path = std::ffi::CString::new(path.to_string_lossy().as_bytes()).ok()?;
    let rc = unsafe { libc::statvfs(c_path.as_ptr(), &mut stat) };
    if rc != 0 {
        return None;
    }
    let frsize = stat.f_frsize.max(1);
    let free = stat.f_bavail.saturating_mul(frsize);
    let total = stat.f_blocks.saturating_mul(frsize);
    if free < 500 * 1024 * 1024 {
        return Some("disk_full".into());
    }
    if total > 0 && free * 20 < total {
        return Some("disk_full".into());
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmp_is_not_full() {
        assert!(too_full(Path::new("/tmp")).is_none());
    }
}
