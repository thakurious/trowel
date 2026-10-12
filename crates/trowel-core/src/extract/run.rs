//! External tools as subprocesses with a wall-clock timeout and `RLIMIT_AS`.
//! `ROBUSTNESS.md` F5; paths are made absolute so a leading dash cannot look like a flag.

use std::ffi::OsStr;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use super::error::ExtractError;

/// F5: 2 GiB address-space cap on every extractor child.
pub const RLIMIT_AS_BYTES: u64 = 2 * 1024 * 1024 * 1024;
pub const TIMEOUT_PDF_PAGE: Duration = Duration::from_secs(20);
pub const TIMEOUT_OCR: Duration = Duration::from_secs(30);
pub const TIMEOUT_DOCX: Duration = Duration::from_secs(10);
pub const TIMEOUT_IMAGE: Duration = Duration::from_secs(20);
pub const TIMEOUT_FFMPEG: Duration = Duration::from_secs(300);

/// Linux `RLIMIT_AS`.
const RLIMIT_AS: i32 = 9;
const SIGKILL: i32 = 9;

#[repr(C)]
struct Rlimit {
    rlim_cur: u64,
    rlim_max: u64,
}

unsafe extern "C" {
    fn setrlimit(resource: i32, rlim: *const Rlimit) -> i32;
    fn kill(pid: i32, sig: i32) -> i32;
}

pub fn abs_path(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join(path)
    }
}

/// `program args...` with timeout and `RLIMIT_AS`. Prefer [`run_tool`] so paths sit after `--`.
pub fn run_argv(program: &str, args: &[&OsStr], timeout: Duration) -> Result<Output, ExtractError> {
    let mut cmd = Command::new(program);
    cmd.args(args);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    unsafe {
        cmd.pre_exec(|| {
            let lim = Rlimit {
                rlim_cur: RLIMIT_AS_BYTES,
                rlim_max: RLIMIT_AS_BYTES,
            };
            if setrlimit(RLIMIT_AS, &lim) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let child = cmd
        .spawn()
        .map_err(|err| ExtractError::Unreadable(format!("{program}: {err}")))?;
    wait_child(program, child, timeout)
}

/// `program flags... -- path...` then wait up to `timeout`, kill on overrun.
pub fn run_tool(
    program: &str,
    flags: &[&str],
    paths: &[&OsStr],
    timeout: Duration,
) -> Result<Output, ExtractError> {
    let mut args: Vec<&OsStr> = flags.iter().map(|s| OsStr::new(*s)).collect();
    args.push(OsStr::new("--"));
    args.extend(paths.iter().copied());
    run_argv(program, &args, timeout)
}

fn wait_child(
    program: &str,
    child: std::process::Child,
    timeout: Duration,
) -> Result<Output, ExtractError> {
    let pid = child.id();
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    match rx.recv_timeout(timeout) {
        Ok(Ok(output)) => Ok(output),
        Ok(Err(err)) => Err(ExtractError::Unreadable(format!("{program}: {err}"))),
        Err(_) => {
            unsafe {
                kill(pid as i32, SIGKILL);
            }
            let _ = rx.recv();
            Err(ExtractError::Unreadable(format!(
                "{program} timed out after {}s",
                timeout.as_secs()
            )))
        }
    }
}

pub fn stdout_text(output: &Output, program: &str) -> Result<String, ExtractError> {
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(ExtractError::Unreadable(format!(
            "{program} failed: {}",
            err.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// ffmpeg timeout: 5 minutes per hour of media, at least one hour's budget.
pub fn ffmpeg_timeout(duration_secs: f64) -> Duration {
    let hours = (duration_secs / 3600.0).max(1.0);
    Duration::from_secs((300.0 * hours).ceil() as u64)
}

/// Apply the F5 2 GiB `RLIMIT_AS` cap to a child about to be spawned (streaming ffmpeg).
pub fn limit_child_memory(cmd: &mut Command) {
    unsafe {
        cmd.pre_exec(|| {
            let lim = Rlimit {
                rlim_cur: RLIMIT_AS_BYTES,
                rlim_max: RLIMIT_AS_BYTES,
            };
            if setrlimit(RLIMIT_AS, &lim) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    #[test]
    fn overrun_is_killed() {
        let start = Instant::now();
        let err = run_argv("sleep", &["30".as_ref()], Duration::from_millis(400)).unwrap_err();
        assert!(start.elapsed() < Duration::from_secs(5));
        assert!(err.to_string().contains("timed out"));
    }
}
