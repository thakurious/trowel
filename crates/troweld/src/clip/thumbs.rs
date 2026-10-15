//! `~/.cache/trowel/clip/<id>@{64,132,264}.png`, written off-thread with libvips like file
//! previews (`previews.rs`). The row's first paint shows the kind icon (UI-DESIGN §5).

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use trowel_proto::PathValue;

pub const SIZES: [u32; 3] = [64, 132, 264];
/// What `ClipRow.thumb` points at.
pub const ROW_SIZE: u32 = 132;

pub fn dir() -> PathBuf {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/tmp".into())).join(".cache")
        })
        .join("trowel/clip")
}

pub fn path(id: i64, size: u32) -> PathBuf {
    dir().join(format!("{id}@{size}.png"))
}

pub fn row_thumb(id: i64) -> Option<PathValue> {
    let p = path(id, ROW_SIZE);
    p.is_file().then(|| PathValue::from_path(&p))
}

/// Renders every size; `false` when libvips is missing or fails. Each size goes through a temp
/// name so a reader never sees half a PNG.
pub fn render(id: i64, src: &Path) -> bool {
    let dir = dir();
    if fs::create_dir_all(&dir).is_err() {
        return false;
    }
    for size in SIZES {
        let dest = path(id, size);
        if dest.is_file() {
            continue;
        }
        let tmp = dir.join(format!(".{id}@{size}.{}.png", std::process::id()));
        let ok = Command::new("vips")
            .arg("thumbnail")
            .arg(src)
            .arg(&tmp)
            .arg(size.to_string())
            .args(["--height", &size.to_string(), "--size", "down"])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok || fs::rename(&tmp, &dest).is_err() {
            let _ = fs::remove_file(&tmp);
            return false;
        }
    }
    true
}

pub fn remove(id: i64) {
    for size in SIZES {
        let _ = fs::remove_file(path(id, size));
    }
}

/// Drops thumbnails of entries that no longer exist (and stale temp files).
pub fn sweep(live: &HashSet<i64>) -> u64 {
    let Ok(read) = fs::read_dir(dir()) else {
        return 0;
    };
    let mut removed = 0;
    for item in read.filter_map(Result::ok) {
        let name = item.file_name().to_string_lossy().into_owned();
        let id = name.split('@').next().and_then(|i| i.parse::<i64>().ok());
        let keep = !name.starts_with('.') && id.is_some_and(|id| live.contains(&id));
        if !keep && fs::remove_file(item.path()).is_ok() {
            removed += 1;
        }
    }
    removed
}
