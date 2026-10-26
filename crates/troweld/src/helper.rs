//! Indexing helper: `trowel index-service`. `upstream/Sources/DigUpApp/Engine.swift` `startHelper`.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;
use std::time::Duration;

use serde_json::Value;

use crate::config_file::expand_tilde;
use trowel_proto::Config;

const STOP_GRACE: Duration = Duration::from_secs(15);

pub struct HelperOpts {
    pub index_dir: PathBuf,
    pub fake: bool,
    pub backfill: bool,
    pub linger: f64,
}

pub struct Helper {
    stdin: Mutex<Option<ChildStdin>>,
    child: Mutex<Option<Child>>,
}

impl Helper {
    pub fn spawn(
        opts: HelperOpts,
        cfg: &Config,
        on_event: impl Fn(Value) + Send + 'static,
    ) -> std::io::Result<Arc<Self>> {
        let bin = locate_trowel();
        let speed = speed_profile(&cfg.index_speed);
        let mut cmd = index_command(&bin, &speed);
        cmd.arg("index-service")
            .arg("--index")
            .arg(&opts.index_dir)
            .arg("--linger")
            .arg(opts.linger.to_string())
            .arg("--pace")
            .arg(speed.pace.to_string());
        if opts.fake {
            cmd.arg("--fake");
        }
        if opts.backfill {
            cmd.arg("--backfill");
        }
        cmd.arg("--prune");
        for folder in &cfg.excluded_folders {
            cmd.arg("--exclude").arg(expand_tilde(folder));
        }
        for ext in &cfg.excluded_types {
            cmd.arg("--exclude-ext").arg(ext);
        }
        for folder in &cfg.code_roots {
            cmd.arg("--code-root").arg(expand_tilde(folder));
        }
        for folder in &cfg.code_excluded {
            cmd.arg("--code-exclude").arg(expand_tilde(folder));
        }
        let roots: Vec<PathBuf> = cfg.roots.iter().map(|r| expand_tilde(r)).collect();
        for root in &roots {
            cmd.arg(root);
        }
        if roots.is_empty() && cfg.code_roots.is_empty() {
            return Err(std::io::Error::other("no folders to index"));
        }
        let log_path = opts.index_dir.join("indexer.log");
        let log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok();
        if let Some(log) = log {
            cmd.stderr(Stdio::from(log));
        } else {
            cmd.stderr(Stdio::null());
        }
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped());
        // D28: the efficiency cores, as a ready-made mask (nothing is allocated after fork).
        let affinity = if speed.efficiency_cores {
            efficiency_cpus(&read_max_freqs()).map(|cpus| cpu_mask(&cpus))
        } else {
            None
        };
        // Die with troweld so a SIGKILL in tests does not leave index-service holding the DB.
        unsafe {
            use std::os::unix::process::CommandExt;
            cmd.pre_exec(move || {
                // SAFETY: called in the child after fork, before exec. Only async-signal-safe
                // syscalls. Priority, affinity and the death signal survive the exec of
                // `systemd-run --scope` into the helper, and the worker, ffmpeg and tesseract it
                // starts inherit priority and affinity.
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM, 0, 0, 0);
                lower_priority();
                if let Some(mask) = affinity.as_ref() {
                    libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), mask);
                }
                Ok(())
            });
        }
        let mut child = spawn_from_lasting_thread(cmd)
            .map_err(|err| std::io::Error::other(format!("spawn {}: {err}", bin.display())))?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take();
        let helper = Arc::new(Self {
            stdin: Mutex::new(stdin),
            child: Mutex::new(Some(child)),
        });
        if let Some(stdout) = stdout {
            let helper_exit = Arc::clone(&helper);
            thread::spawn(move || {
                let reader = BufReader::new(stdout);
                for line in reader.lines() {
                    let Ok(line) = line else { break };
                    if let Ok(event) = serde_json::from_str::<Value>(&line) {
                        on_event(event);
                    }
                }
                let mut signal = None;
                if let Ok(mut slot) = helper_exit.child.lock() {
                    if let Some(mut child) = slot.take() {
                        use std::os::unix::process::ExitStatusExt;
                        signal = child.wait().ok().and_then(|s| s.signal());
                    }
                }
                if let Some(sig) = signal {
                    eprintln!("troweld: helper ended by signal {sig}");
                }
                on_event(serde_json::json!({"event": "exited", "signal": signal}));
            });
        }
        Ok(helper)
    }

    pub fn send(&self, command: &str) {
        if let Ok(mut slot) = self.stdin.lock() {
            if let Some(stdin) = slot.as_mut() {
                let _ = writeln!(stdin, "{command}");
                let _ = stdin.flush();
            }
        }
    }

    pub fn running(&self) -> bool {
        self.child.lock().ok().is_some_and(|c| c.is_some())
    }

    /// Asks the helper to stop (it does between files) and ends it if it is still there 15 s
    /// later: a helper stuck in one file never gets to the next, and Pause or a change of folders
    /// has to work anyway. What it was reading is read again next time.
    /// `upstream/Sources/DigUpApp/Engine.swift` `stopHelper(within:)` (809f185).
    pub fn stop(self: &Arc<Self>) {
        self.stop_within(STOP_GRACE);
    }

    pub fn stop_within(self: &Arc<Self>, grace: Duration) {
        self.send("stop");
        let helper = Arc::clone(self);
        thread::spawn(move || {
            thread::sleep(grace);
            let Ok(mut slot) = helper.child.lock() else {
                return;
            };
            if let Some(child) = slot.as_mut() {
                if matches!(child.try_wait(), Ok(None)) {
                    eprintln!(
                        "troweld: helper still running {} s after stop; ending it",
                        grace.as_secs()
                    );
                    // SAFETY: our own child's pid, not yet reaped (try_wait said it runs).
                    unsafe {
                        libc::kill(child.id() as libc::pid_t, libc::SIGTERM);
                    }
                }
            }
        });
    }
}

/// `PR_SET_PDEATHSIG` fires when the *thread* that forked the child exits, not the process
/// (prctl(2)). `start_helper` runs on whatever thread asked — a client connection, the old
/// helper's event reader — and the new helper got SIGTERM the moment that thread ended (a CLI
/// `resume`, a closed panel connection, the end of the previous helper): indexing stopped with
/// no error. Every helper is forked from this one thread, which lives as long as troweld.
fn spawn_from_lasting_thread(mut cmd: Command) -> std::io::Result<Child> {
    type Job = (Command, std::sync::mpsc::Sender<std::io::Result<Child>>);
    static SPAWNER: OnceLock<Mutex<std::sync::mpsc::Sender<Job>>> = OnceLock::new();
    let tx = SPAWNER.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Job>();
        thread::Builder::new()
            .name("troweld-spawner".into())
            .spawn(move || {
                for (mut cmd, reply) in rx {
                    let _ = reply.send(cmd.spawn());
                }
            })
            .expect("spawner thread");
        Mutex::new(tx)
    });
    let (reply, result) = std::sync::mpsc::channel();
    let sent = tx
        .lock()
        .map_err(|_| std::io::Error::other("spawner lock"))?
        .send((cmd, reply));
    if let Err(std::sync::mpsc::SendError((back, _))) = sent {
        cmd = back;
        return cmd.spawn();
    }
    result
        .recv()
        .map_err(|_| std::io::Error::other("spawner thread gone"))?
}

/// Slice the helper and its worker run in. ROBUSTNESS §2: the daemon's `MemoryHigh=400M` is for
/// the daemon alone; a helper in its cgroup (with a Vulkan worker) was throttled at that line.
pub const INDEX_SLICE: &str = "trowel-index.slice";

/// What an index speed (D28, `config.index_speed`) lets the helper use. Nice 10, the idle I/O
/// class and `CPUWeight=20` hold at every speed; these come on top.
#[derive(Debug, Clone, PartialEq)]
pub struct SpeedProfile {
    /// Rest after each batch, × its embedding time (`index-service --pace`): GPU duty cycle.
    pub pace: f64,
    /// `CPUQuota=` on the helper's scope: the most CPU time it gets, in % of one core.
    pub cpu_quota_percent: Option<u32>,
    /// Run on the efficiency cores when the CPU has them.
    pub efficiency_cores: bool,
}

/// gentle (default; also any unknown value): GPU busy at most ~1/3 of the time, one core's worth
/// of CPU. balanced: ~1/2, two cores. full: no limits beyond priority (the pre-D28 behavior).
pub fn speed_profile(name: &str) -> SpeedProfile {
    match name {
        "full" => SpeedProfile {
            pace: 0.0,
            cpu_quota_percent: None,
            efficiency_cores: false,
        },
        "balanced" => SpeedProfile {
            pace: 1.0,
            cpu_quota_percent: Some(200),
            efficiency_cores: true,
        },
        _ => SpeedProfile {
            pace: 2.0,
            cpu_quota_percent: Some(100),
            efficiency_cores: true,
        },
    }
}

/// The CPUs with the lowest top frequency when cores differ (Lunar Lake: 4–7 are the low-power
/// E-cores at 3.7 GHz, 0–3 the P-cores at 4.7–4.8 GHz). None on a uniform CPU.
pub fn efficiency_cpus(max_freqs: &[(usize, u64)]) -> Option<Vec<usize>> {
    let lowest = max_freqs.iter().map(|(_, f)| *f).min()?;
    let highest = max_freqs.iter().map(|(_, f)| *f).max()?;
    // Within 5 %: boost bins on one core type, not a second type.
    if highest <= lowest + lowest / 20 {
        return None;
    }
    let cpus: Vec<usize> = max_freqs
        .iter()
        .filter(|(_, f)| *f <= lowest + lowest / 20)
        .map(|(cpu, _)| *cpu)
        .collect();
    (!cpus.is_empty()).then_some(cpus)
}

/// `cpuinfo_max_freq` of each online CPU.
fn read_max_freqs() -> Vec<(usize, u64)> {
    let mut out = Vec::new();
    let Ok(dir) = std::fs::read_dir("/sys/devices/system/cpu") else {
        return out;
    };
    for entry in dir.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(n) = name
            .strip_prefix("cpu")
            .and_then(|n| n.parse::<usize>().ok())
        else {
            continue;
        };
        if let Some(f) = std::fs::read_to_string(entry.path().join("cpufreq/cpuinfo_max_freq"))
            .ok()
            .and_then(|t| t.trim().parse::<u64>().ok())
        {
            out.push((n, f));
        }
    }
    out.sort();
    out
}

fn cpu_mask(cpus: &[usize]) -> libc::cpu_set_t {
    // SAFETY: cpu_set_t is plain data; CPU_ZERO/CPU_SET only write inside it.
    unsafe {
        let mut set: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut set);
        for &cpu in cpus {
            libc::CPU_SET(cpu, &mut set);
        }
        set
    }
}

/// `systemd-run --user --scope` around the helper when the user manager answers, else a plain
/// spawn (tests, CI, no systemd). The limits go on the scope itself so they hold even before
/// `trowel setup` has installed the slice unit. `TROWEL_NO_SCOPE=1` turns it off.
fn index_command(bin: &Path, speed: &SpeedProfile) -> Command {
    if scopes_work() {
        let mut cmd = Command::new("systemd-run");
        cmd.args(["--user", "--scope", "--quiet", "--collect"])
            .arg(format!("--slice={INDEX_SLICE}"))
            .args([
                "-p",
                "MemoryHigh=3G",
                "-p",
                "MemoryMax=4G",
                "-p",
                "CPUWeight=20",
            ]);
        if let Some(q) = speed.cpu_quota_percent {
            cmd.arg("-p").arg(format!("CPUQuota={q}%"));
        }
        cmd.arg("--").arg(bin);
        cmd
    } else {
        Command::new(bin)
    }
}

fn scopes_work() -> bool {
    static WORKS: OnceLock<bool> = OnceLock::new();
    *WORKS.get_or_init(|| {
        if std::env::var_os("TROWEL_NO_SCOPE").is_some_and(|v| v != "0") {
            return false;
        }
        Command::new("systemd-run")
            .args(["--user", "--scope", "--quiet", "--collect"])
            .arg(format!("--slice={INDEX_SLICE}"))
            .arg("true")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
    })
}

/// Nice 10 and the idle I/O class (ROBUSTNESS §2). A slice can't carry these: systemd ignores
/// `Nice=` / `IOSchedulingClass=` in `[Slice]`.
fn lower_priority() {
    const IOPRIO_CLASS_IDLE: libc::c_int = 3;
    const IOPRIO_CLASS_SHIFT: libc::c_int = 13;
    const IOPRIO_WHO_PROCESS: libc::c_int = 1;
    // SAFETY: plain syscalls on the calling process; failures are ignored.
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
        libc::syscall(
            libc::SYS_ioprio_set,
            IOPRIO_WHO_PROCESS,
            0,
            IOPRIO_CLASS_IDLE << IOPRIO_CLASS_SHIFT,
        );
    }
}

pub fn locate_trowel() -> PathBuf {
    if let Ok(p) = std::env::var("TROWEL") {
        return PathBuf::from(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        let sibling = exe.with_file_name("trowel");
        if sibling.is_file() {
            return sibling;
        }
    }
    PathBuf::from("trowel")
}

pub fn linger_seconds() -> f64 {
    std::env::var("TROWEL_LINGER")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(60.0)
}

#[cfg(test)]
mod speed_tests {
    use super::*;

    #[test]
    fn profiles() {
        assert_eq!(speed_profile("full").pace, 0.0);
        assert_eq!(speed_profile("full").cpu_quota_percent, None);
        assert_eq!(speed_profile("balanced").cpu_quota_percent, Some(200));
        assert_eq!(speed_profile("gentle").pace, 2.0);
        // Anything unknown is the gentle default.
        assert_eq!(speed_profile("turbo"), speed_profile("gentle"));
    }

    #[test]
    fn efficiency_cores_only_on_hybrid_cpus() {
        // This machine: Core Ultra 7 258V.
        let lunar = [
            (0, 4_700_000),
            (1, 4_800_000),
            (2, 4_700_000),
            (3, 4_800_000),
            (4, 3_700_000),
            (5, 3_700_000),
            (6, 3_700_000),
            (7, 3_700_000),
        ];
        assert_eq!(efficiency_cpus(&lunar), Some(vec![4, 5, 6, 7]));
        // Uniform cores with boost bins a few % apart: no pinning.
        let uniform = [
            (0, 4_500_000),
            (1, 4_600_000),
            (2, 4_500_000),
            (3, 4_550_000),
        ];
        assert_eq!(efficiency_cpus(&uniform), None);
        assert_eq!(efficiency_cpus(&[]), None);
    }
}
