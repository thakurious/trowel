//! Supervises `wl-paste --watch trowel clip-capture` (D20, `LAUNCHER-PLAN.md` L3 "Capture"). One
//! watcher sees every type (L0.6). It is stopped while capture is off or paused (incognito), and
//! restarted with backoff when it dies (no compositor yet, a compositor restart).

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// What the capture child needs, passed through the environment of `wl-paste`.
#[derive(Debug, Clone, PartialEq)]
pub struct CaptureEnv {
    pub enabled: bool,
    pub excluded_apps: Vec<String>,
    pub max_bytes: u64,
}

pub struct Watcher {
    /// Bumped on every change; the loop restarts the child when it moves.
    gen: Mutex<(u64, CaptureEnv)>,
    wake: Condvar,
    /// `wl-paste`'s pid while it runs, for the `clip_ingest` peer check.
    pub pid: AtomicU32,
    /// 0 = capturing, -1 = paused until resumed, else a Unix time.
    pub paused_until: AtomicI64,
    pub last_error: Mutex<Option<String>>,
    socket: PathBuf,
    pub spool: PathBuf,
}

pub const ENV_SOCKET: &str = "TROWEL_SOCKET";
pub const ENV_EXCLUDE: &str = "TROWEL_CLIP_EXCLUDE";
pub const ENV_MAX_BYTES: &str = "TROWEL_CLIP_MAX_BYTES";
pub const ENV_SPOOL: &str = "TROWEL_CLIP_SPOOL";

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

impl Watcher {
    pub fn new(socket: &Path, env: CaptureEnv) -> Arc<Self> {
        let spool = socket
            .parent()
            .unwrap_or(Path::new("/tmp"))
            .join("clip-spool");
        Arc::new(Self {
            gen: Mutex::new((0, env)),
            wake: Condvar::new(),
            pid: AtomicU32::new(0),
            paused_until: AtomicI64::new(0),
            last_error: Mutex::new(None),
            socket: socket.to_path_buf(),
            spool,
        })
    }

    pub fn env(&self) -> CaptureEnv {
        self.gen.lock().expect("watch gen").1.clone()
    }

    pub fn set_env(&self, env: CaptureEnv) {
        let mut g = self.gen.lock().expect("watch gen");
        if g.1 != env {
            g.0 += 1;
            g.1 = env;
            self.wake.notify_all();
        }
    }

    /// `None` pauses until resumed, `Some(0)` resumes. Returns `paused_until`.
    pub fn pause(&self, minutes: Option<u32>) -> i64 {
        let until = match minutes {
            None => -1,
            Some(0) => 0,
            Some(m) => now() + i64::from(m) * 60,
        };
        self.paused_until.store(until, Ordering::SeqCst);
        let mut g = self.gen.lock().expect("watch gen");
        g.0 += 1;
        self.wake.notify_all();
        until
    }

    pub fn paused(&self) -> bool {
        match self.paused_until.load(Ordering::SeqCst) {
            0 => false,
            -1 => true,
            until if until > now() => true,
            _ => {
                self.paused_until.store(0, Ordering::SeqCst);
                false
            }
        }
    }

    /// `paused_until` as the protocol reports it (an expired pause reads 0).
    pub fn paused_until(&self) -> i64 {
        if self.paused() {
            self.paused_until.load(Ordering::SeqCst)
        } else {
            0
        }
    }

    pub fn running(&self) -> bool {
        self.pid.load(Ordering::SeqCst) != 0
    }

    /// Waits up to `d` or until something changes; returns the generation seen.
    fn nap(&self, seen: u64, d: Duration) -> u64 {
        let g = self.gen.lock().expect("watch gen");
        if g.0 != seen {
            return g.0;
        }
        let (g, _) = self.wake.wait_timeout(g, d).expect("watch wait");
        g.0
    }

    fn command(&self, env: &CaptureEnv) -> Command {
        let mut cmd = Command::new("wl-paste");
        cmd.arg("--watch")
            .arg(crate::helper::locate_trowel())
            .arg("clip-capture")
            .env(ENV_SOCKET, &self.socket)
            .env(ENV_EXCLUDE, env.excluded_apps.join("\n"))
            .env(ENV_MAX_BYTES, env.max_bytes.to_string())
            .env(ENV_SPOOL, &self.spool)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped());
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(|| {
                // SAFETY: after fork, before exec; prctl is async-signal-safe. The supervisor
                // thread lives as long as troweld, so the death signal means "troweld died".
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM, 0, 0, 0);
                Ok(())
            });
        }
        cmd
    }

    /// Starts the supervisor thread (it lives as long as the daemon).
    pub fn start(self: &Arc<Self>) {
        let me = Arc::clone(self);
        thread::Builder::new()
            .name("troweld-clip-watch".into())
            .spawn(move || me.supervise())
            .ok();
    }

    fn supervise(&self) {
        let mut backoff = Duration::from_secs(1);
        let mut seen = self.gen.lock().expect("watch gen").0;
        loop {
            let env = self.env();
            if !env.enabled || self.paused() {
                seen = self.nap(seen, Duration::from_secs(1));
                continue;
            }
            let _ = std::fs::create_dir_all(&self.spool);
            {
                use std::os::unix::fs::PermissionsExt;
                let _ =
                    std::fs::set_permissions(&self.spool, std::fs::Permissions::from_mode(0o700));
            }
            let child = self.command(&env).spawn();
            let mut child: Child = match child {
                Ok(c) => c,
                Err(err) => {
                    *self.last_error.lock().expect("watch err") = Some(format!("wl-paste: {err}"));
                    seen = self.nap(seen, Duration::from_secs(30));
                    continue;
                }
            };
            self.pid.store(child.id(), Ordering::SeqCst);
            // Drained as it comes (the capture child shares this pipe), keeping the last line.
            let last_line = Arc::new(Mutex::new(String::new()));
            if let Some(stderr) = child.stderr.take() {
                let sink = Arc::clone(&last_line);
                thread::spawn(move || {
                    use std::io::BufRead;
                    for line in std::io::BufReader::new(stderr)
                        .lines()
                        .map_while(Result::ok)
                    {
                        if !line.trim().is_empty() {
                            *sink.lock().expect("stderr line") = line;
                        }
                    }
                });
            }
            let started = Instant::now();
            let exited = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break Some(status),
                    Ok(None) => {}
                    Err(_) => break None,
                }
                let now_gen = self.nap(seen, Duration::from_millis(250));
                if now_gen != seen || self.paused() {
                    seen = now_gen;
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
            };
            self.pid.store(0, Ordering::SeqCst);
            let Some(status) = exited else {
                backoff = Duration::from_secs(1);
                continue;
            };
            thread::sleep(Duration::from_millis(50));
            let err = last_line.lock().expect("stderr line").clone();
            *self.last_error.lock().expect("watch err") = Some(format!(
                "wl-paste --watch exited ({status}): {}",
                err.trim()
            ));
            backoff = if started.elapsed() > Duration::from_secs(30) {
                Duration::from_secs(1)
            } else {
                (backoff * 2).min(Duration::from_secs(60))
            };
            seen = self.nap(seen, backoff);
        }
    }

    /// Whether `pid` is `trowel clip-capture` run by our `wl-paste` (its child, or a grandchild
    /// through a shell).
    pub fn is_capture_peer(&self, pid: u32) -> bool {
        let watcher = self.pid.load(Ordering::SeqCst);
        if watcher == 0 || pid == 0 {
            return false;
        }
        let mut cur = pid;
        for _ in 0..3 {
            match parent_pid(cur) {
                Some(p) if p == watcher => return true,
                Some(p) if p > 1 => cur = p,
                _ => return false,
            }
        }
        false
    }
}

fn parent_pid(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // `pid (comm) state ppid …`; comm may contain spaces and parentheses.
    let rest = &stat[stat.rfind(')')? + 1..];
    rest.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parent_of_self_is_known() {
        let me = std::process::id();
        let parent = parent_pid(me).unwrap();
        assert_eq!(parent, std::os::unix::process::parent_id());
    }

    #[test]
    fn pause_values() {
        let w = Watcher::new(
            Path::new("/tmp/x.sock"),
            CaptureEnv {
                enabled: true,
                excluded_apps: vec![],
                max_bytes: 1,
            },
        );
        assert_eq!(w.paused_until(), 0);
        assert_eq!(w.pause(None), -1);
        assert!(w.paused());
        let until = w.pause(Some(5));
        assert!(until > now() + 290);
        assert_eq!(w.pause(Some(0)), 0);
        assert!(!w.paused());
        w.paused_until.store(now() - 1, Ordering::SeqCst);
        assert_eq!(w.paused_until(), 0, "an expired pause reads as capturing");
        assert!(!w.is_capture_peer(std::process::id()), "no watcher running");
    }
}
