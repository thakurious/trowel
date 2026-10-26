//! Process Monitor sampling (`PHASE9-PLAN.md` §1.2, P-D30). Per process: CPU from
//! `/proc/<pid>/stat`, RAM (RSS), disk from `/proc/<pid>/io` (own processes only; others are
//! `None`, never 0), GPU from DRM fdinfo (`xe`: Δ`drm-cycles-<engine>` / Δ`drm-total-cycles-…`, P0
//! item 3), grouped by app through the cgroup unit. It runs only while someone subscribes.
//!
//! Cost (P0): ~700 pids, ~10k fds. The fd scan for render nodes is the expensive part, so each
//! process's DRM fds are cached and rescanned only for new processes and every 5 s.

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

pub struct ProcFs {
    root: PathBuf,
    own_uid: u32,
    tick: f64,
    cpus: f64,
    users: HashMap<u32, String>,
    /// (pid, start) → render-node fds, and when they were found.
    drm_fds: HashMap<(i32, u64), (Vec<String>, Instant)>,
    units: HashMap<(i32, u64), String>,
    boot_time: f64,
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
            drm_fds: HashMap::new(),
            units: HashMap::new(),
            boot_time,
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
    let f: Vec<&str> = text[close + 1..].split_whitespace().collect();
    // f[0] = state (field 3), so field n is f[n - 3].
    let num = |n: usize| f.get(n - 3).and_then(|v| v.parse::<u64>().ok());
    Some((
        comm,
        f.get(1)?.parse().ok()?,
        num(14)? + num(15)?,
        num(22)?,
        num(24)?,
    ))
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
        Some("KiB") => n * 1024,
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
        let Ok(rd) = std::fs::read_dir(&self.root) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut seen = HashSet::new();
        let now = Instant::now();
        for e in rd.flatten() {
            let Some(pid) = e.file_name().to_str().and_then(|n| n.parse::<i32>().ok()) else {
                continue;
            };
            let dir = e.path();
            use std::os::unix::fs::MetadataExt;
            let Ok(meta) = std::fs::metadata(&dir) else {
                continue;
            };
            let uid = meta.uid();
            if !all_users && uid != self.own_uid {
                continue;
            }
            let Some((comm, ppid, cpu_ticks, start, rss_pages)) =
                std::fs::read_to_string(dir.join("stat"))
                    .ok()
                    .as_deref()
                    .and_then(parse_stat)
            else {
                continue;
            };
            // Kernel threads have no memory and no cgroup unit worth a row.
            if ppid == 2 || pid == 2 {
                continue;
            }
            let key = (pid, start);
            seen.insert(key);
            let io = std::fs::read_to_string(dir.join("io"))
                .ok()
                .as_deref()
                .and_then(parse_io);
            let unit = match self.units.get(&key) {
                Some(u) => u.clone(),
                None => {
                    let u = std::fs::read_to_string(dir.join("cgroup"))
                        .map(|c| parse_unit(&c))
                        .unwrap_or_default();
                    self.units.insert(key, u.clone());
                    u
                }
            };
            let mut gpu = Vec::new();
            let mut vram = None;
            if uid == self.own_uid {
                let stale = self
                    .drm_fds
                    .get(&key)
                    .is_none_or(|(_, at)| now.duration_since(*at) > Duration::from_secs(5));
                if stale {
                    let fds: Vec<String> = std::fs::read_dir(dir.join("fd"))
                        .into_iter()
                        .flatten()
                        .flatten()
                        .filter(|f| {
                            std::fs::read_link(f.path())
                                .map(|l| l.starts_with("/dev/dri/"))
                                .unwrap_or(false)
                        })
                        .map(|f| f.file_name().to_string_lossy().into_owned())
                        .collect();
                    self.drm_fds.insert(key, (fds, now));
                }
                let mut clients = HashSet::new();
                if let Some((fds, _)) = self.drm_fds.get(&key) {
                    for fd in fds {
                        let Some((client, engines, res)) =
                            std::fs::read_to_string(dir.join("fdinfo").join(fd))
                                .ok()
                                .as_deref()
                                .and_then(parse_fdinfo)
                        else {
                            continue;
                        };
                        if !clients.insert(client) {
                            continue;
                        }
                        *vram.get_or_insert(0) += res;
                        for (e, b, t) in engines {
                            gpu.push((client, e, b, t));
                        }
                    }
                }
            }
            out.push(Raw {
                pid,
                ppid,
                comm,
                uid,
                cpu_ticks,
                start,
                rss: rss_pages * 4096,
                io,
                gpu,
                vram,
                unit,
            });
        }
        self.drm_fds.retain(|k, _| seen.contains(k));
        self.units.retain(|k, _| seen.contains(k));
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
    /// (client, engine) → (busy, total)
    gpu: HashMap<(u64, String), (u64, u64)>,
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
        }
    }

    /// One tick. `dt` is measured; the first tick has no rates (all 0, GPU/disk unknown).
    pub fn sample(&mut self, all_users: bool) {
        let began = Instant::now();
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
                if let Some(&(b0, t0)) = prev.and_then(|p| p.gpu.get(&k)) {
                    let dt_gpu = total.saturating_sub(t0);
                    if dt_gpu > 0 {
                        *per_engine.entry(engine.clone()).or_default() +=
                            busy.saturating_sub(b0) as f64 / dt_gpu as f64 * 100.0;
                    }
                } else {
                    per_engine.entry(engine.clone()).or_default();
                }
                gpu_map.insert(k, (*busy, *total));
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
        self.last_cost = began.elapsed();
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
