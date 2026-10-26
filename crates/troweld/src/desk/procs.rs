//! Process Monitor sampling (`PHASE9-PLAN.md` §1.2, P-D30). Per process: CPU from
//! `/proc/<pid>/stat`, RAM (RSS), disk from `/proc/<pid>/io` (own processes only; others are
//! `None`, never 0), GPU from DRM fdinfo (`xe`: Δ`drm-cycles-<engine>` / Δ`drm-total-cycles-…`, P0
//! item 3), grouped by app through the cgroup unit. It runs only while someone subscribes.
//!
//! Cost (P0): ~700 pids, ~10k fds. The fd scan for render nodes is the expensive part (18 ms for
//! all of them), so each process's DRM fds are cached and rescanned only when its open-fd count
//! changes (`stat(/proc/<pid>/fd).st_size` is that count since Linux 6.2, checked 2026-10-26),
//! only for fd numbers not seen before, and in full after 10–20 min (spread by pid).

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use trowel_proto::{KillSignal, ProcGroup, ProcRow, ProcSort, ProcTotals};

/// One process as read in one pass (counters, not rates).
#[derive(Debug, Clone, Default)]
pub struct Raw {
    pub pid: i32,
    pub ppid: i32,
    pub comm: String,
    pub uid: u32,
    /// utime + stime, clock ticks.
    pub cpu_ticks: u64,
    /// Start time in clock ticks after boot: (pid, start) names a process across samples.
    pub start: u64,
    pub rss: u64,
    pub io: Option<(u64, u64)>,
    /// (drm client id, engine, busy cycles, total cycles).
    pub gpu: Vec<(u64, String, u64, u64)>,
    pub vram: Option<u64>,
    /// The cgroup unit (`app-…scope`, `foo.service`); empty when unknown.
    pub unit: String,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SysRaw {
    /// All CPU ticks and the idle part of them (`/proc/stat` line `cpu`).
    pub cpu_total: u64,
    pub cpu_idle: u64,
    pub mem_total: u64,
    pub mem_available: u64,
}

/// Where samples come from: `/proc` on the real daemon, a fixture in `--mock`.
pub trait Source: Send {
    fn read(&mut self, all_users: bool) -> Vec<Raw>;
    fn system(&mut self) -> SysRaw;
    fn ticks_per_second(&self) -> f64;
    fn cpus(&self) -> f64;
    /// cmdline, PSS, threads, state, start (Unix seconds) for the pane.
    fn detail(&mut self, pid: i32) -> Option<Value>;
    fn kill(&mut self, pid: i32, signal: KillSignal) -> Result<(), String>;
    fn alive(&mut self, pid: i32) -> bool;
    fn own_uid(&self) -> u32;
    fn user_name(&mut self, uid: u32) -> String;
    /// What the last pass cost, for the `proc` event's `profile` (diagnostics).
    fn profile(&self) -> Value {
        Value::Null
    }
}

/// Ending these would end the session or the lock screen: refused outright.
const REFUSED: [&str; 4] = ["Hyprland", "quickshell", "systemd", "troweld"];
/// These need the name typed to confirm (P-D30).
const PROTECTED: [&str; 11] = [
    "dbus-broker",
    "dbus-daemon",
    "pipewire",
    "pipewire-pulse",
    "wireplumber",
    "uwsm",
    "fcitx5",
    "xdg-desktop-portal",
    "xdg-desktop-portal-hyprland",
    "hyprsunset",
    "Xwayland",
];

pub fn refused(comm: &str) -> bool {
    REFUSED.contains(&comm) || comm.starts_with("uwsm")
}

pub fn protected(comm: &str) -> bool {
    refused(comm) || PROTECTED.contains(&comm)
}

// ---------------------------------------------------------------- the real /proc source

/// A render-node fd (its fdinfo kept open) and what its last fdinfo said. `xe` fdinfo costs ~85 µs a read (it samples
/// GPU timestamps), so an fd that dups an earlier one's client is not read, and a client that has
/// never run anything is read every 5th tick, its zero counters reused in between.
#[derive(Debug, Clone, Default)]
struct DrmFd {
    fd: String,
    info: Option<std::sync::Arc<std::fs::File>>,
    client: Option<u64>,
    busy_ever: bool,
    engines: EngineCycles,
    resident: u64,
}

pub struct ProcFs {
    root: PathBuf,
    own_uid: u32,
    tick: f64,
    cpus: f64,
    users: HashMap<u32, String>,
    /// Every process seen, by pid, with its files kept open between passes.
    tracked: HashMap<i32, Tracked>,
    pass: u64,
    boot_time: f64,
    /// Last pass, for profiling: fd rescans, time in them, DRM fdinfo reads and their time.
    pub last_rescans: (u32, Duration, u32, Duration),
}

/// One process across passes. Its `stat` and `io` stay open and are re-read from offset 0
/// (procfs regenerates them on each read): one `pread` each per pass instead of a path lookup,
/// open, read and close — most of a pass is kernel time (2026-10-26: 12 of 14.5 ms).
struct Tracked {
    start: u64,
    uid: u32,
    stat: std::fs::File,
    io: Option<std::fs::File>,
    unit: String,
    /// Render-node fds, the open-fd count when they were found, and when the fd table was last
    /// read in full.
    drm: Option<(Vec<DrmFd>, u64, Instant)>,
    /// Every fd number seen and whether it is a render node: a count change only `readlink`s the
    /// new numbers (a browser churns fds; reading all 3,000 of its links cost 20 ms a pass).
    known: HashMap<String, bool>,
    /// The last `stat`/`io` read: (comm, ppid, cpu ticks, rss pages, io), and for how many passes
    /// in a row the CPU ticks stood still. An idle process is re-read every other pass and its
    /// last values reused in between (its deltas are 0 either way): most of the ~400 are idle,
    /// and their reads were most of a pass.
    last: Option<LastRead>,
    still: u32,
}

/// (comm, ppid, cpu ticks, rss pages, io) of a process's last `stat`/`io` read.
type LastRead = (String, i32, u64, u64, Option<(u64, u64)>);

/// The whole of a small procfs file, read from offset 0 of an open handle. procfs hands over
/// the whole file when the buffer holds it, so one `pread` is enough (no second one for EOF).
fn pread_all(f: &std::fs::File, buf: &mut Vec<u8>) -> Option<()> {
    use std::os::unix::fs::FileExt;
    buf.clear();
    let mut chunk = [0u8; 4096];
    loop {
        let n = f.read_at(&mut chunk, buf.len() as u64).ok()?;
        buf.extend_from_slice(&chunk[..n]);
        if n < chunk.len() || buf.len() > 64 * 1024 {
            return Some(());
        }
    }
}

impl ProcFs {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        // SAFETY: plain libc queries.
        let (uid, tick, cpus) = unsafe {
            (
                libc::getuid(),
                libc::sysconf(libc::_SC_CLK_TCK),
                libc::sysconf(libc::_SC_NPROCESSORS_ONLN),
            )
        };
        // Two files stay open per process (`Tracked`): ~800 at 400 processes. A soft limit of
        // 1024 (a daemon started outside systemd) is raised toward the hard one.
        // SAFETY: getrlimit/setrlimit on a struct we own.
        unsafe {
            let mut lim: libc::rlimit = std::mem::zeroed();
            if libc::getrlimit(libc::RLIMIT_NOFILE, &mut lim) == 0 && lim.rlim_cur < 8192 {
                lim.rlim_cur = lim.rlim_max.min(8192);
                libc::setrlimit(libc::RLIMIT_NOFILE, &lim);
            }
        }
        let boot_time = std::fs::read_to_string(root.join("stat"))
            .ok()
            .and_then(|s| {
                s.lines()
                    .find_map(|l| l.strip_prefix("btime "))
                    .and_then(|v| v.trim().parse::<f64>().ok())
            })
            .unwrap_or(0.0);
        Self {
            root,
            own_uid: uid,
            tick: if tick > 0 { tick as f64 } else { 100.0 },
            cpus: if cpus > 0 { cpus as f64 } else { 1.0 },
            users: HashMap::new(),
            tracked: HashMap::new(),
            pass: 0,
            boot_time,
            last_rescans: (0, Duration::ZERO, 0, Duration::ZERO),
        }
    }

    fn pid_dir(&self, pid: i32) -> PathBuf {
        self.root.join(pid.to_string())
    }
}

/// `/proc/<pid>/stat` → (comm, ppid, utime+stime, start, rss pages).
pub fn parse_stat(text: &str) -> Option<(String, i32, u64, u64, u64)> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let comm = text[open + 1..close].to_string();
    // After ") ": field 3 (state) first; ppid is field 4, utime/stime 14/15, start 22, rss 24.
    let (mut ppid, mut utime, mut stime, mut start, mut rss) = (None, None, None, None, None);
    for (i, v) in text[close + 1..].split_whitespace().enumerate() {
        match i + 3 {
            4 => ppid = v.parse::<i32>().ok(),
            14 => utime = v.parse::<u64>().ok(),
            15 => stime = v.parse::<u64>().ok(),
            22 => start = v.parse::<u64>().ok(),
            24 => {
                rss = v.parse::<u64>().ok();
                break;
            }
            _ => {}
        }
    }
    Some((comm, ppid?, utime? + stime?, start?, rss?))
}

pub fn parse_io(text: &str) -> Option<(u64, u64)> {
    let mut r = None;
    let mut w = None;
    for l in text.lines() {
        if let Some(v) = l.strip_prefix("read_bytes:") {
            r = v.trim().parse().ok();
        } else if let Some(v) = l.strip_prefix("write_bytes:") {
            w = v.trim().parse().ok();
        }
    }
    Some((r?, w?))
}

/// `0::/user.slice/…/app.slice/app-foo.scope` → `app-foo.scope`.
pub fn parse_unit(cgroup: &str) -> String {
    cgroup
        .lines()
        .find_map(|l| l.strip_prefix("0::"))
        .and_then(|p| p.trim().rsplit('/').find(|s| !s.is_empty()))
        .unwrap_or("")
        .to_string()
}

fn kib(v: &str) -> Option<u64> {
    let mut it = v.split_whitespace();
    let n: u64 = it.next()?.parse().ok()?;
    Some(match it.next() {
        Some("KiB") | Some("kB") => n * 1024,
        Some("MiB") => n * 1024 * 1024,
        Some("GiB") => n * 1024 * 1024 * 1024,
        _ => n,
    })
}

/// One DRM fd's fdinfo → (client id, [(engine, cycles, total)], resident bytes).
/// (engine, busy cycles, total cycles)
pub type EngineCycles = Vec<(String, u64, u64)>;

pub fn parse_fdinfo(text: &str) -> Option<(u64, EngineCycles, u64)> {
    let mut client = None;
    let mut busy: HashMap<String, u64> = HashMap::new();
    let mut total: HashMap<String, u64> = HashMap::new();
    let mut resident = 0;
    for l in text.lines() {
        let Some((k, v)) = l.split_once(':') else {
            continue;
        };
        let v = v.trim();
        if k == "drm-client-id" {
            client = v.parse().ok();
        } else if let Some(e) = k.strip_prefix("drm-total-cycles-") {
            total.insert(e.into(), v.parse().unwrap_or(0));
        } else if let Some(e) = k.strip_prefix("drm-cycles-") {
            busy.insert(e.into(), v.parse().unwrap_or(0));
        } else if k == "drm-resident-gtt"
            || k == "drm-resident-stolen"
            || k.starts_with("drm-resident-vram")
        {
            resident += kib(v).unwrap_or(0);
        }
    }
    let mut engines: Vec<(String, u64, u64)> = busy
        .into_iter()
        .filter_map(|(e, b)| total.get(&e).map(|t| (e, b, *t)))
        .collect();
    engines.sort();
    Some((client?, engines, resident))
}

impl Source for ProcFs {
    fn read(&mut self, all_users: bool) -> Vec<Raw> {
        use std::os::unix::fs::MetadataExt;
        let Ok(rd) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let now = Instant::now();
        let mut prof = (0u32, Duration::ZERO, 0u32, Duration::ZERO);
        let mut buf = Vec::with_capacity(4096);
        for e in rd.flatten() {
            let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<i32>().ok()) else {
                continue;
            };
            let dir = e.path();
            if let std::collections::hash_map::Entry::Vacant(slot) = self.tracked.entry(pid) {
                let Ok(stat) = std::fs::File::open(dir.join("stat")) else {
                    continue;
                };
                // The stat file belongs to the process's user.
                let Ok(uid) = stat.metadata().map(|m| m.uid()) else {
                    continue;
                };
                let unit = std::fs::read_to_string(dir.join("cgroup"))
                    .map(|c| parse_unit(&c))
                    .unwrap_or_default();
                slot.insert(Tracked {
                    start: 0,
                    uid,
                    stat,
                    io: std::fs::File::open(dir.join("io")).ok(),
                    unit,
                    drm: None,
                    known: HashMap::new(),
                    last: None,
                    still: 0,
                });
            }
            let t = self.tracked.get_mut(&pid).expect("tracked");
            if !all_users && t.uid != self.own_uid {
                seen.insert(pid);
                continue;
            }
            let reuse = t.still >= 3 && !(self.pass + pid as u64).is_multiple_of(2);
            let (comm, ppid, cpu_ticks, start, rss_pages, io) = match (&t.last, reuse) {
                (Some((comm, ppid, ticks, rss, io)), true) => {
                    (comm.clone(), *ppid, *ticks, t.start, *rss, *io)
                }
                _ => {
                    if pread_all(&t.stat, &mut buf).is_none() {
                        continue;
                    }
                    let Some((comm, ppid, cpu_ticks, start, rss_pages)) =
                        std::str::from_utf8(&buf).ok().and_then(parse_stat)
                    else {
                        continue;
                    };
                    // A pid used again by a new process: track it afresh next pass.
                    if t.start != 0 && t.start != start {
                        continue;
                    }
                    let io = t.io.as_ref().and_then(|f| {
                        pread_all(f, &mut buf)?;
                        parse_io(std::str::from_utf8(&buf).ok()?)
                    });
                    let moved = t.last.as_ref().is_none_or(|l| l.2 != cpu_ticks);
                    t.still = if moved { 0 } else { t.still + 1 };
                    t.last = Some((comm.clone(), ppid, cpu_ticks, rss_pages, io));
                    (comm, ppid, cpu_ticks, start, rss_pages, io)
                }
            };
            t.start = start;
            seen.insert(pid);
            // Kernel threads have no memory and no cgroup unit worth a row.
            if ppid == 2 || pid == 2 {
                continue;
            }
            let mut gpu = Vec::new();
            let mut vram = None;
            if t.uid == self.own_uid {
                // The fd count is checked every third pass; a new process is scanned at once.
                let check = t.drm.is_none() || (self.pass + pid as u64).is_multiple_of(3);
                // A full re-read only catches an fd number reused for another file with the count
                // unchanged (rare: a new GPU client almost always changes the count, or is a new
                // process). At 30–60 s these re-reads were most of a 60 s run's cost.
                let max_age = Duration::from_secs(600 + (pid as u64 % 600));
                let fd_count = check.then(|| {
                    std::fs::metadata(dir.join("fd"))
                        .map(|m| m.len())
                        .unwrap_or(0)
                });
                let expired = t
                    .drm
                    .as_ref()
                    .is_some_and(|(_, _, at)| now.duration_since(*at) > max_age);
                let rescan = fd_count
                    .filter(|c| expired || t.drm.as_ref().is_none_or(|(_, count, _)| count != c));
                if let Some(count) = rescan {
                    let started = Instant::now();
                    prof.0 += 1;
                    let (old, full_at) = match t.drm.take() {
                        Some((old, _, at)) if !expired => (old, at),
                        // A full read: an fd number reused for another file is caught here.
                        _ => {
                            t.known.clear();
                            (Vec::new(), now)
                        }
                    };
                    let mut known = HashMap::with_capacity(t.known.len());
                    let mut fds = Vec::new();
                    for f in std::fs::read_dir(dir.join("fd"))
                        .into_iter()
                        .flatten()
                        .flatten()
                    {
                        let fd = f.file_name().to_string_lossy().into_owned();
                        let drm = match t.known.get(&fd) {
                            Some(&d) => d,
                            None => std::fs::read_link(f.path())
                                .map(|l| l.starts_with("/dev/dri/"))
                                .unwrap_or(false),
                        };
                        if drm {
                            let slot = old.iter().find(|o| o.fd == fd).cloned();
                            fds.push(slot.unwrap_or(DrmFd {
                                fd: fd.clone(),
                                ..DrmFd::default()
                            }));
                        }
                        known.insert(fd, drm);
                    }
                    t.known = known;
                    t.drm = Some((fds, count, full_at));
                    prof.1 += started.elapsed();
                }
                let mut clients = HashSet::new();
                let sparse_tick = (self.pass + pid as u64).is_multiple_of(5);
                if let Some((fds, _, _)) = t.drm.as_mut() {
                    for slot in fds.iter_mut() {
                        if slot.client.is_some_and(|c| clients.contains(&c)) {
                            continue;
                        }
                        // Busy clients every other pass (half the processes each pass: the
                        // rate is over the GPU's own cycle counter, so a 2 s window is exact),
                        // never-busy ones every fifth, unknown ones now.
                        let fresh = slot.client.is_none()
                            || (slot.busy_ever && (self.pass + pid as u64).is_multiple_of(2))
                            || sparse_tick;
                        if fresh {
                            let started = Instant::now();
                            prof.2 += 1;
                            if slot.info.is_none() {
                                slot.info = std::fs::File::open(dir.join("fdinfo").join(&slot.fd))
                                    .ok()
                                    .map(std::sync::Arc::new);
                            }
                            let parsed = slot.info.as_ref().and_then(|f| {
                                pread_all(f, &mut buf)?;
                                parse_fdinfo(std::str::from_utf8(&buf).ok()?)
                            });
                            prof.3 += started.elapsed();
                            let Some((client, engines, res)) = parsed else {
                                // Closed or reused for something else: look again next time.
                                slot.client = None;
                                slot.info = None;
                                continue;
                            };
                            slot.busy_ever |= engines.iter().any(|e| e.1 > 0);
                            slot.client = Some(client);
                            slot.engines = engines;
                            slot.resident = res;
                        }
                        let Some(client) = slot.client else {
                            continue;
                        };
                        if !clients.insert(client) {
                            continue;
                        }
                        *vram.get_or_insert(0) += slot.resident;
                        for (e, b, t) in &slot.engines {
                            gpu.push((client, e.clone(), *b, *t));
                        }
                    }
                }
            }
            out.push(Raw {
                pid,
                ppid,
                comm,
                uid: t.uid,
                cpu_ticks,
                start,
                rss: rss_pages * 4096,
                io,
                gpu,
                vram,
                unit: t.unit.clone(),
            });
        }
        self.last_rescans = prof;
        self.pass += 1;
        // Gone processes close their files; a reused pid starts over.
        self.tracked.retain(|pid, _| seen.contains(pid));
        out
    }

    fn system(&mut self) -> SysRaw {
        let mut s = SysRaw::default();
        if let Ok(stat) = std::fs::read_to_string(self.root.join("stat")) {
            if let Some(l) = stat.lines().find(|l| l.starts_with("cpu ")) {
                let v: Vec<u64> = l
                    .split_whitespace()
                    .skip(1)
                    .filter_map(|x| x.parse().ok())
                    .collect();
                s.cpu_total = v.iter().take(8).sum();
                s.cpu_idle = v.get(3).copied().unwrap_or(0) + v.get(4).copied().unwrap_or(0);
            }
        }
        if let Ok(mem) = std::fs::read_to_string(self.root.join("meminfo")) {
            for l in mem.lines() {
                if let Some(v) = l.strip_prefix("MemTotal:") {
                    s.mem_total = kib(v.trim()).unwrap_or(0);
                } else if let Some(v) = l.strip_prefix("MemAvailable:") {
                    s.mem_available = kib(v.trim()).unwrap_or(0);
                }
            }
        }
        s
    }

    fn ticks_per_second(&self) -> f64 {
        self.tick
    }

    fn cpus(&self) -> f64 {
        self.cpus
    }

    fn detail(&mut self, pid: i32) -> Option<Value> {
        let dir = self.pid_dir(pid);
        let stat = std::fs::read_to_string(dir.join("stat")).ok()?;
        let (comm, ppid, _, start, _) = parse_stat(&stat)?;
        let cmdline = std::fs::read(dir.join("cmdline"))
            .map(|b| {
                let s: String = String::from_utf8_lossy(&b[..b.len().min(4096)])
                    .split('\0')
                    .filter(|p| !p.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                s
            })
            .unwrap_or_default();
        let status = std::fs::read_to_string(dir.join("status")).unwrap_or_default();
        let field = |k: &str| {
            status
                .lines()
                .find_map(|l| l.strip_prefix(k))
                .map(|v| v.trim().to_string())
        };
        let pss = std::fs::read_to_string(dir.join("smaps_rollup"))
            .ok()
            .and_then(|s| {
                s.lines()
                    .find_map(|l| l.strip_prefix("Pss:"))
                    .and_then(|v| kib(v.trim()))
            });
        let exe = std::fs::read_link(dir.join("exe"))
            .map(|p| p.display().to_string())
            .ok();
        Some(json!({
            "pid": pid,
            "ppid": ppid,
            "name": comm,
            "cmdline": cmdline,
            "exe": exe,
            "threads": field("Threads:").and_then(|v| v.parse::<u64>().ok()),
            "state": field("State:"),
            "pss": pss,
            "started": self.boot_time + start as f64 / self.tick,
        }))
    }

    fn kill(&mut self, pid: i32, signal: KillSignal) -> Result<(), String> {
        let sig = match signal {
            KillSignal::Term => libc::SIGTERM,
            KillSignal::Kill => libc::SIGKILL,
        };
        // SAFETY: kill(2) on a pid the caller checked is the user's own.
        let rc = unsafe { libc::kill(pid, sig) };
        if rc == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error().to_string())
        }
    }

    fn alive(&mut self, pid: i32) -> bool {
        // A zombie is gone for the user: it holds no memory and can't be killed again.
        std::fs::read_to_string(self.pid_dir(pid).join("stat"))
            .ok()
            .and_then(|s| {
                let close = s.rfind(')')?;
                s[close + 1..].split_whitespace().next().map(|st| st != "Z")
            })
            .unwrap_or(false)
    }

    fn own_uid(&self) -> u32 {
        self.own_uid
    }

    fn profile(&self) -> Value {
        let (rescans, rescan_t, reads, read_t) = self.last_rescans;
        json!({"fd_rescans": rescans, "fd_rescan_ms": rescan_t.as_secs_f64() * 1000.0,
            "fdinfo_reads": reads, "fdinfo_ms": read_t.as_secs_f64() * 1000.0})
    }

    fn user_name(&mut self, uid: u32) -> String {
        if self.users.is_empty() {
            if let Ok(pw) = std::fs::read_to_string("/etc/passwd") {
                for l in pw.lines() {
                    let f: Vec<&str> = l.split(':').collect();
                    if let (Some(n), Some(u)) = (f.first(), f.get(2).and_then(|u| u.parse().ok())) {
                        self.users.insert(u, n.to_string());
                    }
                }
            }
        }
        self.users
            .get(&uid)
            .cloned()
            .unwrap_or_else(|| uid.to_string())
    }
}

// ---------------------------------------------------------------- the mock's fake source

/// Eight processes with steady CPU, GPU and disk rates, for `--mock` and the harness. Counters
/// grow with real time, so rates come out the same however often it is sampled. `kill` removes
/// a process (the mock never sends a signal).
pub struct Fake {
    born: Instant,
    /// pid, comm, unit, CPU in hundredths of a percent of the machine.
    procs: Vec<(i32, &'static str, &'static str, u64)>,
    pub killed: Vec<(i32, KillSignal)>,
}

const FAKE_TICKS: f64 = 1_000_000.0;

impl Default for Fake {
    fn default() -> Self {
        Self {
            born: Instant::now(),
            procs: vec![
                (1001, "firefox", "app-Hyprland-firefox-1.scope", 600),
                (1002, "Isolated Web Co", "app-Hyprland-firefox-1.scope", 250),
                (1100, "yes", "app-ghostty-surface-transient-9.scope", 1250),
                (1101, "bash", "app-ghostty-surface-transient-9.scope", 10),
                (1200, "trowel-worker", "troweld.service", 300),
                (1300, "Hyprland", "wayland-wm@hyprland.desktop.service", 100),
                (1400, "pipewire", "pipewire.service", 20),
                (1500, "obsidian", "app-Hyprland-obsidian-2.scope", 50),
            ],
            killed: Vec::new(),
        }
    }
}

impl Source for Fake {
    fn read(&mut self, _all_users: bool) -> Vec<Raw> {
        let secs = self.born.elapsed().as_secs_f64() + 1.0;
        let ns = (secs * 1e9) as u64;
        self.procs
            .iter()
            .map(|&(pid, comm, unit, rate)| Raw {
                pid,
                ppid: 1,
                comm: comm.into(),
                uid: 1000,
                cpu_ticks: (rate as f64 / 10_000.0 * 8.0 * FAKE_TICKS * secs) as u64,
                start: pid as u64,
                rss: (pid as u64 % 97 + 3) * 10 * 1024 * 1024,
                io: (pid != 1300)
                    .then_some(((secs * pid as f64 * 100.0) as u64, (secs * 4096.0) as u64)),
                gpu: match pid {
                    1200 => vec![(1, "ccs".into(), ns * 3 / 10, ns)],
                    1300 => vec![(2, "rcs".into(), ns / 20, ns)],
                    _ => Vec::new(),
                },
                vram: (pid == 1200).then_some(512 * 1024 * 1024),
                unit: unit.into(),
            })
            .collect()
    }

    fn system(&mut self) -> SysRaw {
        let secs = self.born.elapsed().as_secs_f64() + 1.0;
        SysRaw {
            cpu_total: (secs * 8.0 * FAKE_TICKS) as u64,
            cpu_idle: (secs * 8.0 * FAKE_TICKS * 0.75) as u64,
            mem_total: 30 * 1024 * 1024 * 1024,
            mem_available: 18 * 1024 * 1024 * 1024,
        }
    }

    fn ticks_per_second(&self) -> f64 {
        FAKE_TICKS
    }

    fn cpus(&self) -> f64 {
        8.0
    }

    fn detail(&mut self, pid: i32) -> Option<Value> {
        let p = self.procs.iter().find(|p| p.0 == pid)?;
        Some(
            json!({"pid": pid, "ppid": 1, "name": p.1, "cmdline": format!("/usr/bin/{}", p.1),
            "exe": format!("/usr/bin/{}", p.1), "threads": 4, "state": "S (sleeping)",
            "pss": 50 * 1024 * 1024, "started": 1_793_000_000.0}),
        )
    }

    fn kill(&mut self, pid: i32, signal: KillSignal) -> Result<(), String> {
        self.killed.push((pid, signal));
        self.procs.retain(|p| p.0 != pid);
        Ok(())
    }

    fn alive(&mut self, pid: i32) -> bool {
        self.procs.iter().any(|p| p.0 == pid)
    }

    fn own_uid(&self) -> u32 {
        1000
    }

    fn user_name(&mut self, _uid: u32) -> String {
        "me".into()
    }
}

// ---------------------------------------------------------------- rates, grouping, history

#[derive(Debug, Clone, Default)]
struct Prev {
    cpu_ticks: u64,
    io: Option<(u64, u64)>,
    /// (client, engine) → (busy, total, the last rate): a counter not re-read this pass keeps
    /// its rate rather than reading as 0 %.
    gpu: HashMap<(u64, String), (u64, u64, f64)>,
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct Point {
    pub cpu: f64,
    pub gpu: f64,
    pub ram: u64,
    pub disk: f64,
}

/// A process with its rates for the last interval.
#[derive(Debug, Clone)]
pub struct Rated {
    pub raw: Raw,
    pub cpu: f64,
    pub gpu: Option<f64>,
    pub engines: Vec<(String, f64)>,
    pub disk: Option<(f64, f64)>,
}

pub const HISTORY: usize = 60;

/// Turns passes into rates and keeps 60 samples of history per row key.
pub struct Sampler {
    pub source: Box<dyn Source>,
    prev: HashMap<(i32, u64), Prev>,
    prev_sys: Option<SysRaw>,
    prev_at: Option<Instant>,
    pub history: HashMap<String, VecDeque<Point>>,
    pub last: Vec<Rated>,
    pub totals: ProcTotals,
    pub last_cost: Duration,
    /// CPU time (user + system) of the last pass on this thread: the BENCHMARKS budget.
    pub last_cpu: Duration,
    /// Passes so far: the first has no rates, so the second comes sooner.
    pub samples: u64,
}

impl Sampler {
    pub fn new(source: Box<dyn Source>) -> Self {
        Self {
            source,
            prev: HashMap::new(),
            prev_sys: None,
            prev_at: None,
            history: HashMap::new(),
            last: Vec::new(),
            totals: ProcTotals::default(),
            last_cost: Duration::ZERO,
            last_cpu: Duration::ZERO,
            samples: 0,
        }
    }

    /// One tick. `dt` is measured; the first tick has no rates (all 0, GPU/disk unknown).
    pub fn sample(&mut self, all_users: bool) {
        let began = Instant::now();
        let cpu0 = thread_cpu();
        let raws = self.source.read(all_users);
        let sys = self.source.system();
        let dt = self
            .prev_at
            .map(|p| began.duration_since(p).as_secs_f64())
            .unwrap_or(0.0);
        let ticks = self.source.ticks_per_second();
        let cpus = self.source.cpus();
        let mut next_prev = HashMap::with_capacity(raws.len());
        let mut rated = Vec::with_capacity(raws.len());
        for raw in raws {
            let key = (raw.pid, raw.start);
            let prev = self.prev.get(&key);
            let cpu = match prev {
                Some(p) if dt > 0.0 => {
                    (raw.cpu_ticks.saturating_sub(p.cpu_ticks) as f64 / ticks / dt / cpus * 100.0)
                        .clamp(0.0, 100.0)
                }
                _ => 0.0,
            };
            let disk = match (prev.and_then(|p| p.io), raw.io) {
                (Some((r0, w0)), Some((r1, w1))) if dt > 0.0 => Some((
                    r1.saturating_sub(r0) as f64 / dt,
                    w1.saturating_sub(w0) as f64 / dt,
                )),
                (None, Some(_)) if prev.is_none() || dt == 0.0 => Some((0.0, 0.0)),
                _ => None,
            };
            let mut per_engine: HashMap<String, f64> = HashMap::new();
            let mut gpu_map = HashMap::new();
            for (client, engine, busy, total) in &raw.gpu {
                let k = (*client, engine.clone());
                let rate = match prev.and_then(|p| p.gpu.get(&k)) {
                    Some(&(b0, t0, last)) => {
                        let dt_gpu = total.saturating_sub(t0);
                        if dt_gpu > 0 {
                            busy.saturating_sub(b0) as f64 / dt_gpu as f64 * 100.0
                        } else {
                            last
                        }
                    }
                    None => 0.0,
                };
                *per_engine.entry(engine.clone()).or_default() += rate;
                gpu_map.insert(k, (*busy, *total, rate));
            }
            let mut engines: Vec<(String, f64)> = per_engine
                .into_iter()
                .map(|(e, v)| (e, v.min(100.0)))
                .collect();
            engines.sort_by(|a, b| a.0.cmp(&b.0));
            let gpu =
                (!raw.gpu.is_empty()).then(|| engines.iter().map(|e| e.1).fold(0.0, f64::max));
            next_prev.insert(
                key,
                Prev {
                    cpu_ticks: raw.cpu_ticks,
                    io: raw.io,
                    gpu: gpu_map,
                },
            );
            rated.push(Rated {
                raw,
                cpu,
                gpu,
                engines,
                disk,
            });
        }
        let cpu_total = match self.prev_sys {
            Some(p) if sys.cpu_total > p.cpu_total => {
                let all = (sys.cpu_total - p.cpu_total) as f64;
                let idle = sys.cpu_idle.saturating_sub(p.cpu_idle) as f64;
                ((all - idle) / all * 100.0).clamp(0.0, 100.0)
            }
            _ => 0.0,
        };
        let (dr, dw) = rated
            .iter()
            .filter_map(|r| r.disk)
            .fold((0.0, 0.0), |a, d| (a.0 + d.0, a.1 + d.1));
        let mut gpu_engines: HashMap<&str, f64> = HashMap::new();
        for r in &rated {
            for (e, v) in &r.engines {
                *gpu_engines.entry(e.as_str()).or_default() += v;
            }
        }
        let any_gpu = rated.iter().any(|r| r.gpu.is_some());
        self.totals = ProcTotals {
            cpu: cpu_total,
            gpu: any_gpu.then(|| gpu_engines.values().copied().fold(0.0, f64::max).min(100.0)),
            ram_used: sys.mem_total.saturating_sub(sys.mem_available),
            ram_total: sys.mem_total,
            disk_read: dr,
            disk_write: dw,
            processes: rated.len() as u32,
        };
        self.prev = next_prev;
        self.prev_sys = Some(sys);
        self.prev_at = Some(began);
        self.last = rated;
        self.record_history();
        self.samples += 1;
        self.last_cost = began.elapsed();
        self.last_cpu = thread_cpu().saturating_sub(cpu0);
    }

    fn record_history(&mut self) {
        let mut live = HashSet::new();
        let mut push = |history: &mut HashMap<String, VecDeque<Point>>, key: String, p: Point| {
            let h = history.entry(key.clone()).or_default();
            if h.len() == HISTORY {
                h.pop_front();
            }
            h.push_back(p);
            live.insert(key);
        };
        let mut apps: HashMap<String, Point> = HashMap::new();
        for r in &self.last {
            let p = Point {
                cpu: r.cpu,
                gpu: r.gpu.unwrap_or(0.0),
                ram: r.raw.rss,
                disk: r.disk.map(|d| d.0 + d.1).unwrap_or(0.0),
            };
            push(&mut self.history, format!("pid:{}", r.raw.pid), p);
            let a = apps.entry(app_key(&r.raw)).or_default();
            a.cpu += p.cpu;
            a.gpu += p.gpu;
            a.ram += p.ram;
            a.disk += p.disk;
        }
        for (k, p) in apps {
            push(&mut self.history, k, p);
        }
        self.history.retain(|k, _| live.contains(k));
    }

    pub fn rows(
        &mut self,
        sort: ProcSort,
        group: ProcGroup,
        all_users: bool,
        limit: usize,
    ) -> Vec<ProcRow> {
        let own = self.source.own_uid();
        let mut rows: Vec<ProcRow> = Vec::new();
        let last = self.last.clone();
        let visible: Vec<&Rated> = last
            .iter()
            .filter(|r| all_users || r.raw.uid == own)
            .collect();
        match group {
            ProcGroup::Pid => {
                for r in visible {
                    let user = self.source.user_name(r.raw.uid);
                    rows.push(ProcRow {
                        key: format!("pid:{}", r.raw.pid),
                        name: r.raw.comm.clone(),
                        app_id: (!r.raw.unit.is_empty()).then(|| r.raw.unit.clone()),
                        pids: vec![r.raw.pid],
                        user,
                        cpu: r.cpu,
                        gpu: r.gpu,
                        ram: r.raw.rss,
                        vram: r.raw.vram,
                        disk_read: r.disk.map(|d| d.0),
                        disk_write: r.disk.map(|d| d.1),
                        protected: protected(&r.raw.comm),
                        kill_refused: refused(&r.raw.comm) || r.raw.uid != own,
                    });
                }
            }
            ProcGroup::App => {
                let mut groups: Vec<(String, Vec<&Rated>)> = Vec::new();
                for r in visible {
                    let k = app_key(&r.raw);
                    match groups.iter_mut().find(|g| g.0 == k) {
                        Some(g) => g.1.push(r),
                        None => groups.push((k, vec![r])),
                    }
                }
                for (key, members) in groups {
                    let root = members
                        .iter()
                        .min_by_key(|r| (r.raw.start, r.raw.pid))
                        .expect("non-empty");
                    let sum_opt = |f: &dyn Fn(&Rated) -> Option<f64>| {
                        members
                            .iter()
                            .filter_map(|r| f(r))
                            .fold(None, |a: Option<f64>, v| Some(a.unwrap_or(0.0) + v))
                    };
                    let user = self.source.user_name(root.raw.uid);
                    rows.push(ProcRow {
                        key,
                        name: root.raw.comm.clone(),
                        app_id: (!root.raw.unit.is_empty()).then(|| root.raw.unit.clone()),
                        pids: members.iter().map(|r| r.raw.pid).collect(),
                        user,
                        cpu: members.iter().map(|r| r.cpu).sum::<f64>().min(100.0),
                        gpu: sum_opt(&|r| r.gpu).map(|g| g.min(100.0)),
                        ram: members.iter().map(|r| r.raw.rss).sum(),
                        vram: members
                            .iter()
                            .filter_map(|r| r.raw.vram)
                            .fold(None, |a: Option<u64>, v| Some(a.unwrap_or(0) + v)),
                        disk_read: sum_opt(&|r| r.disk.map(|d| d.0)),
                        disk_write: sum_opt(&|r| r.disk.map(|d| d.1)),
                        protected: members.iter().any(|r| protected(&r.raw.comm)),
                        kill_refused: members
                            .iter()
                            .any(|r| refused(&r.raw.comm) || r.raw.uid != own),
                    });
                }
            }
        }
        sort_rows(&mut rows, sort);
        rows.truncate(limit);
        rows
    }

    pub fn engines(&self, pid: i32) -> Vec<(String, f64)> {
        self.last
            .iter()
            .find(|r| r.raw.pid == pid)
            .map(|r| r.engines.clone())
            .unwrap_or_default()
    }

    pub fn raw(&self, pid: i32) -> Option<&Raw> {
        self.last.iter().find(|r| r.raw.pid == pid).map(|r| &r.raw)
    }
}

/// This thread's CPU time so far (`CLOCK_THREAD_CPUTIME_ID`: user + system).
fn thread_cpu() -> Duration {
    // SAFETY: clock_gettime writes into the timespec we own.
    let mut ts: libc::timespec = unsafe { std::mem::zeroed() };
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut ts) };
    Duration::new(
        ts.tv_sec.max(0) as u64,
        ts.tv_nsec.clamp(0, 999_999_999) as u32,
    )
}

/// A process's app: its cgroup unit, or itself when it has none.
pub fn app_key(raw: &Raw) -> String {
    if raw.unit.is_empty() || raw.unit == "init.scope" {
        format!("pid:{}", raw.pid)
    } else {
        format!("app:{}", raw.unit)
    }
}

fn sort_value(r: &ProcRow, sort: ProcSort) -> f64 {
    match sort {
        ProcSort::Cpu => r.cpu,
        ProcSort::Gpu => r.gpu.unwrap_or(-1.0),
        ProcSort::Ram => r.ram as f64,
        ProcSort::Disk => r
            .disk_read
            .zip(r.disk_write)
            .map(|(a, b)| a + b)
            .unwrap_or(-1.0),
    }
}

/// Highest first; ties (a lot of 0 %) by RAM, then name, so the order holds still between ticks.
pub fn sort_rows(rows: &mut [ProcRow], sort: ProcSort) {
    rows.sort_by(|a, b| {
        sort_value(b, sort)
            .total_cmp(&sort_value(a, sort))
            .then(b.ram.cmp(&a.ram))
            .then(a.name.cmp(&b.name))
            .then(a.key.cmp(&b.key))
    });
}

/// Why `pids` can't be signalled, or None. `confirm` is the typed name for protected ones.
pub fn kill_check(
    sampler: &mut Sampler,
    pids: &[i32],
    confirm: Option<&str>,
    daemon_pid: i32,
) -> Option<String> {
    if pids.is_empty() {
        return Some("no process given".into());
    }
    let own = sampler.source.own_uid();
    for &pid in pids {
        if pid <= 1 || pid == daemon_pid {
            return Some("Trowel won't end that process".into());
        }
        let Some(raw) = sampler.raw(pid).cloned() else {
            continue;
        };
        if raw.uid != own {
            return Some(format!("{} belongs to another user", raw.comm));
        }
        if refused(&raw.comm) {
            return Some(format!(
                "Ending {} would end your session, so Trowel won't",
                raw.comm
            ));
        }
        if protected(&raw.comm)
            && !confirm.is_some_and(|c| c.trim().eq_ignore_ascii_case(&raw.comm))
        {
            return Some(format!("type {} to confirm", raw.comm));
        }
    }
    None
}

pub fn proc_root() -> PathBuf {
    Path::new("/proc").to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stat_with_spaces_and_parens_in_comm() {
        let s = "4242 (Web Content (x)) S 1 4242 4242 0 -1 4194560 100 0 0 0 250 50 0 0 20 0 30 0 123456 1000000 2560 18446744073709551615";
        let (comm, ppid, cpu, start, rss) = parse_stat(s).unwrap();
        assert_eq!(comm, "Web Content (x)");
        assert_eq!((ppid, cpu, start, rss), (1, 300, 123456, 2560));
    }

    #[test]
    fn fdinfo_xe() {
        let t = "drm-driver:\txe\ndrm-client-id:\t5\ndrm-resident-gtt:\t2048 KiB\ndrm-resident-system:\t0\ndrm-cycles-rcs:\t100\ndrm-total-cycles-rcs:\t1000\ndrm-cycles-ccs:\t7\ndrm-total-cycles-ccs:\t1000\n";
        let (client, engines, res) = parse_fdinfo(t).unwrap();
        assert_eq!(client, 5);
        assert_eq!(res, 2048 * 1024);
        assert_eq!(
            engines,
            [("ccs".to_string(), 7, 1000), ("rcs".to_string(), 100, 1000)]
        );
        assert_eq!(
            parse_unit("0::/user.slice/user-1000.slice/user@1000.service/app.slice/app-x.scope\n"),
            "app-x.scope"
        );
        assert_eq!(
            parse_io("rchar: 1\nread_bytes: 10\nwrite_bytes: 20\n"),
            Some((10, 20))
        );
    }

    #[test]
    fn rates_grouping_sort_and_policy() {
        let mut s = Sampler::new(Box::<Fake>::default());
        s.sample(false);
        std::thread::sleep(Duration::from_millis(20));
        s.sample(false);
        let rows = s.rows(ProcSort::Cpu, ProcGroup::Pid, false, 100);
        assert_eq!(rows[0].name, "yes", "{rows:?}");
        assert!(rows[0].cpu > rows[1].cpu);
        let worker = rows.iter().find(|r| r.name == "trowel-worker").unwrap();
        assert!((worker.gpu.unwrap() - 30.0).abs() < 0.01, "{worker:?}");
        let gpu = s.rows(ProcSort::Gpu, ProcGroup::Pid, false, 100);
        assert_eq!(gpu[0].name, "trowel-worker");
        let hypr = gpu.iter().find(|r| r.name == "Hyprland").unwrap();
        assert!(hypr.disk_read.is_none(), "unreadable io is None, not 0");
        assert!(hypr.kill_refused);
        let apps = s.rows(ProcSort::Ram, ProcGroup::App, false, 100);
        let ff = apps.iter().find(|r| r.name == "firefox").unwrap();
        assert_eq!(ff.pids, [1001, 1002]);
        assert_eq!(s.history["pid:1100"].len(), 2);
        assert!(s.history.contains_key("app:app-Hyprland-firefox-1.scope"));

        assert!(kill_check(&mut s, &[1300], None, 99)
            .unwrap()
            .contains("session"));
        assert!(kill_check(&mut s, &[1400], None, 99)
            .unwrap()
            .contains("type pipewire"));
        assert!(kill_check(&mut s, &[1400], Some("PipeWire"), 99).is_none());
        assert!(kill_check(&mut s, &[1100], None, 99).is_none());
        assert!(kill_check(&mut s, &[99], None, 99).is_some());
        assert!(kill_check(&mut s, &[1], None, 99).is_some());
    }

    #[test]
    fn real_proc_reads_this_process() {
        let mut fs = ProcFs::new("/proc");
        let me = std::process::id() as i32;
        let raws = fs.read(false);
        let mine = raws.iter().find(|r| r.pid == me).expect("own pid listed");
        assert!(mine.rss > 0 && mine.io.is_some());
        assert!(fs.alive(me));
        assert!(!fs.detail(me).unwrap()["cmdline"]
            .as_str()
            .unwrap()
            .is_empty());
        let sys = fs.system();
        assert!(sys.mem_total > sys.mem_available && sys.cpu_total > 0);
    }
}
