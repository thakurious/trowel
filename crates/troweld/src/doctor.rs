//! `trowel doctor` / PROTOCOL `doctor`. `ROBUSTNESS.md` §3.

use std::path::Path;
use std::process::Command;

use serde::Serialize;
use serde_json::json;

#[derive(Debug, Clone, Serialize)]
pub struct Check {
    pub name: String,
    pub status: &'static str,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fix: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Report {
    pub ok: bool,
    pub checks: Vec<Check>,
}

impl Report {
    pub fn all_green(&self) -> bool {
        self.ok && self.checks.iter().all(|c| c.status != "FAIL")
    }
}

pub fn run(index_dir: Option<&Path>, socket: Option<&Path>) -> Report {
    let mut checks = vec![
        daemon_check(socket),
        model_check(),
        tools_check(),
        inotify_check(),
        gpu_check(),
        worker_check(),
        disk_check(),
    ];
    if let Some(dir) = index_dir {
        checks.push(index_check(dir));
    }
    let ok = checks.iter().all(|c| c.status != "FAIL");
    Report { ok, checks }
}

/// The clipboard watcher (`LAUNCHER-PLAN.md` L3): running, paused, off, or failing to start.
pub fn clipboard_check(
    enabled: bool,
    paused: bool,
    running: bool,
    last_error: Option<String>,
    stats: Option<(u64, u64)>,
) -> Check {
    let size = stats.map_or_else(
        || "history unreadable".to_string(),
        |(n, bytes)| format!("{n} entries, {:.1} MB of blobs", bytes as f64 / 1e6),
    );
    let (status, detail, fix) = if !enabled {
        ("OK", format!("capture off; {size}"), None)
    } else if paused {
        ("OK", format!("paused; {size}"), None)
    } else if running {
        ("OK", format!("wl-paste --watch running; {size}"), None)
    } else {
        (
            "WARN",
            format!(
                "watcher not running ({}); {size}",
                last_error.unwrap_or_else(|| "starting".into())
            ),
            Some("check that wl-clipboard is installed and a Wayland session is running".into()),
        )
    };
    Check {
        name: "clipboard".into(),
        status,
        detail,
        fix,
    }
}

fn daemon_check(socket: Option<&Path>) -> Check {
    match socket {
        Some(path) if path.exists() => Check {
            name: "daemon".into(),
            status: "OK",
            detail: format!("socket {}", path.display()),
            fix: None,
        },
        Some(path) => Check {
            name: "daemon".into(),
            status: "WARN",
            detail: format!("no socket at {}", path.display()),
            fix: Some("systemctl --user start troweld".into()),
        },
        None => Check {
            name: "daemon".into(),
            status: "WARN",
            detail: "socket path unknown".into(),
            fix: None,
        },
    }
}

fn model_check() -> Check {
    match (
        trowel_core::ModelFiles::locate_text(),
        trowel_core::ModelFiles::locate_media(),
    ) {
        (Ok(text), Some(media)) => {
            let tsz = std::fs::metadata(&text).map(|m| m.len()).unwrap_or(0);
            let msz = std::fs::metadata(&media).map(|m| m.len()).unwrap_or(0);
            if tsz != trowel_core::ModelFiles::TEXT.bytes
                || msz != trowel_core::ModelFiles::MEDIA.bytes
            {
                Check {
                    name: "model".into(),
                    status: "FAIL",
                    detail: format!("size mismatch text={tsz} media={msz}"),
                    fix: Some("re-download GGUFs into ~/.local/share/trowel/models".into()),
                }
            } else {
                Check {
                    name: "model".into(),
                    status: "OK",
                    detail: format!("{} + {}", text.display(), media.display()),
                    fix: None,
                }
            }
        }
        (Err(err), _) => Check {
            name: "model".into(),
            status: "FAIL",
            detail: err.to_string(),
            fix: Some("place GGUFs in ~/.local/share/trowel/models".into()),
        },
        (Ok(_), None) => Check {
            name: "model".into(),
            status: "FAIL",
            detail: "mmproj GGUF missing".into(),
            fix: Some("place GGUFs in ~/.local/share/trowel/models".into()),
        },
    }
}

fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|dir| dir.join(bin).is_file()))
}

fn tools_check() -> Check {
    let need = [
        "pdftotext",
        "pdftoppm",
        "pdfinfo",
        "ffmpeg",
        "ffprobe",
        "tesseract",
        "git",
    ];
    let missing: Vec<&str> = need.iter().copied().filter(|bin| !on_path(bin)).collect();
    if missing.is_empty() {
        Check {
            name: "tools".into(),
            status: "OK",
            detail: need.join(" "),
            fix: None,
        }
    } else {
        Check {
            name: "tools".into(),
            status: "FAIL",
            detail: format!("missing {}", missing.join(" ")),
            fix: Some("pacman -S poppler ffmpeg tesseract git".into()),
        }
    }
}

fn inotify_check() -> Check {
    let max = std::fs::read_to_string("/proc/sys/fs/inotify/max_user_watches")
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    Check {
        name: "inotify".into(),
        status: if max >= 8192 { "OK" } else { "WARN" },
        detail: format!("max_user_watches={max}"),
        fix: if max < 8192 {
            Some("raise fs.inotify.max_user_watches".into())
        } else {
            None
        },
    }
}

fn gpu_check() -> Check {
    match Command::new("vulkaninfo").arg("--summary").output() {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            let line = text
                .lines()
                .find(|l| l.contains("deviceName") || l.contains("Intel") || l.contains("GPU"))
                .unwrap_or("vulkan ok")
                .trim();
            Check {
                name: "gpu".into(),
                status: "OK",
                detail: line.into(),
                fix: None,
            }
        }
        _ => Check {
            name: "gpu".into(),
            status: "WARN",
            detail: "vulkaninfo --summary failed".into(),
            fix: Some("install vulkan-tools".into()),
        },
    }
}

fn worker_check() -> Check {
    match crate::encoder::locate_worker(crate::encoder::default_backend()) {
        Ok(path) => Check {
            name: "worker".into(),
            status: "OK",
            detail: path.display().to_string(),
            fix: None,
        },
        Err(err) => Check {
            name: "worker".into(),
            status: "WARN",
            detail: err,
            fix: Some("scripts/build-llama.sh vulkan".into()),
        },
    }
}

fn disk_check() -> Check {
    match crate::disk::too_full(Path::new("/")) {
        None => Check {
            name: "disk".into(),
            status: "OK",
            detail: "enough free space".into(),
            fix: None,
        },
        Some(reason) => Check {
            name: "disk".into(),
            status: "FAIL",
            detail: reason,
            fix: Some("free space on the index volume".into()),
        },
    }
}

fn index_check(dir: &Path) -> Check {
    let db = dir.join("index.sqlite");
    if !db.exists() {
        return Check {
            name: "index".into(),
            status: "WARN",
            detail: "no index yet".into(),
            fix: None,
        };
    }
    match trowel_core::IndexStore::open(dir) {
        Ok(store) => match store.counts() {
            Ok(c) => Check {
                name: "index".into(),
                status: "OK",
                detail: format!("vectors={} segments={}", c.vectors, c.segments),
                fix: None,
            },
            Err(err) => Check {
                name: "index".into(),
                status: "FAIL",
                detail: err.to_string(),
                fix: Some("trowel doctor; the index may need a rebuild".into()),
            },
        },
        Err(err) => Check {
            name: "index".into(),
            status: "FAIL",
            detail: err.to_string(),
            fix: Some("remove the damaged DB after backing it up".into()),
        },
    }
}

pub fn json_report(report: &Report) -> serde_json::Value {
    json!(report)
}
