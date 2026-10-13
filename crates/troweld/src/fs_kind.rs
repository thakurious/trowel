//! F3/F4: network and FUSE roots poll instead of inotify.

use std::path::Path;

const NFS_SUPER_MAGIC: i64 = 0x6969;
const SMB_SUPER_MAGIC: i64 = 0x517B;
const CIFS_MAGIC: i64 = 0xFF53_4D42;
const FUSE_SUPER_MAGIC: i64 = 0x6573_5546;
const FUSE_CTL: i64 = 0x6573_5543;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RootKind {
    Local,
    Network,
    Fuse,
}

pub fn classify(path: &Path) -> RootKind {
    let mut buf = unsafe { std::mem::zeroed::<libc::statfs>() };
    let c_path = match std::ffi::CString::new(path.to_string_lossy().as_bytes()) {
        Ok(s) => s,
        Err(_) => return RootKind::Local,
    };
    let rc = unsafe { libc::statfs(c_path.as_ptr(), &mut buf) };
    if rc != 0 {
        return RootKind::Local;
    }
    match buf.f_type {
        NFS_SUPER_MAGIC | SMB_SUPER_MAGIC | CIFS_MAGIC => RootKind::Network,
        FUSE_SUPER_MAGIC | FUSE_CTL => RootKind::Fuse,
        _ => RootKind::Local,
    }
}

pub fn poll_interval(kind: RootKind) -> Option<std::time::Duration> {
    match kind {
        RootKind::Local => None,
        RootKind::Network | RootKind::Fuse => Some(std::time::Duration::from_secs(30 * 60)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmp_is_local() {
        assert_eq!(classify(Path::new("/tmp")), RootKind::Local);
    }
}
