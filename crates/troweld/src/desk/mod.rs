//! Phase 9 (`PHASE9-PLAN.md`): the Island, process monitor, system commands, timers and snippets.
//! The same code serves `troweld` and `troweld --mock`; what differs (state file, clipboard, the
//! commands it may run, where process samples come from) is the [`Host`].

use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, Weak};
use std::time::{Duration, Instant};

use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use trowel_proto::{ErrorCode, IslandActivity, KillSignal, Op, PasteMode, ProcGroup, ProcSort};

use crate::state::StateDb;

pub mod clock;
pub mod island;
pub mod procs;
pub mod snippets;
pub mod system;
pub mod timers;

/// A connection's outgoing lines, for events pushed to a subscriber. `id` is unique per
/// connection; `alive` turns false once it closed.
#[derive(Clone)]
pub struct Sink {
    pub id: u64,
    push: Arc<dyn Fn(String) + Send + Sync>,
    alive: Arc<dyn Fn() -> bool + Send + Sync>,
}

impl Sink {
    pub fn new(
        push: impl Fn(String) + Send + Sync + 'static,
        alive: impl Fn() -> bool + Send + Sync + 'static,
    ) -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self {
            id: NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            push: Arc::new(push),
            alive: Arc::new(alive),
        }
    }

    pub fn send(&self, line: String) {
        (self.push)(line)
    }

    pub fn alive(&self) -> bool {
        (self.alive)()
    }
}

pub type OpResult = Result<Value, (ErrorCode, String)>;

/// What the desk needs from the daemon around it.
pub trait Host: Send + Sync {
    /// Runs `f` on `state.sqlite`, opening it on first use.
    fn with_state(
        &self,
        f: &mut dyn FnMut(&StateDb) -> Result<Value, trowel_core::DbError>,
    ) -> Result<Value, String>;
    /// Puts `text` on the clipboard and, unless `copy_only`, sends the paste keystroke (D21).
    fn paste_text(&self, text: &str, copy_only: bool) -> Result<(), String>;
    /// Runs an allow-listed argv detached (system commands, keyboard unlock, notifications).
    fn spawn(&self, argv: &[String]) -> Result<(), String>;
    /// Runs an allow-listed argv and returns its stdout (toggle state probes, theme list).
    fn capture(&self, argv: &[String], timeout: Duration) -> Result<String, String>;
    /// `$HOME` (toggle flag files live under it).
    fn home(&self) -> PathBuf;
    /// Where `omarchy-*` scripts are scanned for.
    fn script_dirs(&self) -> Vec<PathBuf>;
    /// `$XDG_RUNTIME_DIR/trowel/keyboard-lock.json`, if a lock is recorded.
    fn keylock_state(&self) -> Option<Value>;
    /// Process samples: `/proc`, or a fixture.
    fn proc_source(&self) -> Box<dyn procs::Source>;
    /// The clipboard's text, for `{clipboard}`.
    fn clipboard_text(&self) -> String;
    /// The `trowel` binary (keyboard unlock).
    fn trowel_exe(&self) -> String;
    /// `timer_sound` from `config.toml`.
    fn timer_sound(&self) -> bool;
    /// Whether `state.sqlite` exists yet: the desk never creates it just to look for timers.
    fn state_exists(&self) -> bool;
}

/// Runs `f` on the state DB with a typed result (the host's interface speaks `Value`).
pub fn with_db<T: Serialize + DeserializeOwned>(
    host: &dyn Host,
    mut f: impl FnMut(&StateDb) -> Result<T, trowel_core::DbError>,
) -> Result<T, String> {
    let v = host.with_state(&mut |db| {
        f(db).and_then(|t| {
            serde_json::to_value(t).map_err(|e| trowel_core::DbError::new(e.to_string()))
        })
    })?;
    serde_json::from_value(v).map_err(|e| e.to_string())
}

/// `argv` with a deadline: stdout on success, killed (and an error) when it takes longer.
pub fn capture_command(argv: &[String], timeout: Duration) -> Result<String, String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let (program, args) = argv.split_first().ok_or("empty command")?;
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| format!("{program}: {e}"))?;
    let mut stdout = child.stdout.take().ok_or("no stdout")?;
    let reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = stdout.read_to_string(&mut s);
        s
    });
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait().map_err(|e| e.to_string())? {
            Some(status) => {
                let out = reader.join().unwrap_or_default();
                return if status.success() {
                    Ok(out)
                } else {
                    Err(format!("{program} exited with {status}"))
                };
            }
            None if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("{program} took longer than {timeout:?}"));
            }
            None => std::thread::sleep(Duration::from_millis(10)),
        }
    }
}

struct ProcSub {
    sink: Sink,
    interval: Duration,
    sort: ProcSort,
    group: ProcGroup,
    all_users: bool,
}

#[derive(Default)]
struct ProcMon {
    sampler: Option<procs::Sampler>,
    subs: Vec<ProcSub>,
    running: bool,
}

/// Wakes the ticker early (a timer started, an activity with a TTL was posted).
type Wake = Arc<(Mutex<bool>, Condvar)>;

pub struct Desk {
    host: Arc<dyn Host>,
    pub island: island::Island,
    wake: Wake,
    procs: Arc<Mutex<ProcMon>>,
    scripts: Mutex<Option<(Instant, Vec<system::Script>)>>,
    themes: Mutex<Vec<String>>,
    /// Some timer may be running, paused or freshly done; while false the ticker leaves the DB
    /// alone.
    timers_live: std::sync::atomic::AtomicBool,
}

const NOTIFY_GLYPH: &str = "\u{f051b}";
const ALARM: &str = "/usr/share/sounds/freedesktop/stereo/alarm-clock-elapsed.oga";

fn bad(msg: impl Into<String>) -> (ErrorCode, String) {
    (ErrorCode::BadRequest, msg.into())
}

fn io(msg: impl Into<String>) -> (ErrorCode, String) {
    (ErrorCode::Io, msg.into())
}

fn to_value<T: Serialize>(t: T) -> Value {
    serde_json::to_value(t).unwrap_or(Value::Null)
}

impl Desk {
    pub fn new(host: Arc<dyn Host>) -> Arc<Self> {
        let desk = Arc::new(Self {
            host,
            island: island::Island::default(),
            wake: Arc::new((Mutex::new(false), Condvar::new())),
            procs: Arc::new(Mutex::new(ProcMon::default())),
            scripts: Mutex::new(None),
            themes: Mutex::new(Vec::new()),
            timers_live: std::sync::atomic::AtomicBool::new(false),
        });
        desk.repost_keylock();
        desk.repost_timers();
        let weak = Arc::downgrade(&desk);
        let wake = Arc::clone(&desk.wake);
        std::thread::Builder::new()
            .name("desk-ticker".into())
            .spawn(move || ticker(weak, wake))
            .expect("desk ticker");
        desk
    }

    /// Capabilities this build serves, for `hello`.
    pub fn capabilities(&self) -> &'static [&'static str] {
        &["island", "procs", "system", "timers", "snippets"]
    }

    fn poke(&self) {
        let (lock, cv) = &*self.wake;
        *lock.lock().expect("wake") = true;
        cv.notify_all();
    }

    /// A keyboard lock that outlived a daemon restart gets its island back.
    fn repost_keylock(&self) {
        let Some(state) = self.host.keylock_state() else {
            return;
        };
        let until = state["until"].as_f64().unwrap_or(0.0);
        if until > clock::now() {
            self.island
                .post(keylock_activity(until, state["seconds"].as_f64()));
        }
    }

    /// Running, paused and recently finished timers are back on the island after a restart.
    fn repost_timers(&self) {
        if !self.host.state_exists() {
            return;
        }
        let now = clock::now();
        if let Ok(list) = timers::list(&*self.host, now) {
            self.set_timers_live(!list.is_empty());
            for t in list {
                self.island.post(timers::activity(&t, now));
            }
        }
    }

    fn set_timers_live(&self, live: bool) {
        self.timers_live
            .store(live, std::sync::atomic::Ordering::SeqCst);
    }

    fn timers_live(&self) -> bool {
        self.timers_live.load(std::sync::atomic::Ordering::SeqCst)
    }

    /// One ticker pass: expired activities go, due timers fire.
    fn tick(&self) {
        self.island.prune();
        if !self.timers_live() {
            return;
        }
        let now = clock::now();
        match timers::due(&*self.host, now) {
            Ok(fired) => {
                for t in fired {
                    self.announce(&t, now);
                }
            }
            Err(e) => eprintln!("troweld: timers: {e}"),
        }
    }

    fn announce(&self, t: &trowel_proto::Timer, now: f64) {
        self.island.post(timers::activity(t, now));
        let title = if t.label.is_empty() {
            "Timer done".to_string()
        } else {
            t.label.clone()
        };
        let body = if t.late {
            format!(
                "{} timer · finished while asleep",
                clock::countdown(t.total)
            )
        } else {
            format!("{} timer", clock::countdown(t.total))
        };
        let notify: Vec<String> = [
            "omarchy-notification-send",
            "-g",
            NOTIFY_GLYPH,
            "-u",
            "normal",
            &title,
            &body,
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        if self.host.spawn(&notify).is_err() {
            let _ = self.host.spawn(&[
                "notify-send".into(),
                "-a".into(),
                "Trowel".into(),
                title,
                body,
            ]);
        }
        if self.host.timer_sound() {
            let _ = self.host.spawn(&["pw-play".into(), ALARM.into()]);
        }
    }

    /// When the ticker should look again: the next TTL or timer end, at most 2 s ahead while a
    /// timer runs (the monotonic wait doesn't count a suspend; this bounds the catch-up after
    /// one), 60 s otherwise.
    fn next_wait(&self) -> Duration {
        let now = clock::now();
        let mut wait = Duration::from_secs(60);
        if let Some(t) = self.island.next_expiry() {
            wait = wait.min(t.saturating_duration_since(Instant::now()));
        }
        if let Some(end) = self
            .timers_live()
            .then(|| timers::next_end(&*self.host))
            .flatten()
        {
            let until = Duration::from_secs_f64((end - now).max(0.0));
            wait = wait.min(until).min(Duration::from_secs(2));
        }
        wait.max(Duration::from_millis(20))
    }

    fn post_timer(&self, t: &trowel_proto::Timer) {
        self.set_timers_live(true);
        self.island.post(timers::activity(t, clock::now()));
        self.poke();
    }

    /// Handles one v3 op (`Op::is_v3`). `sink` is the caller's connection, for subscriptions.
    pub fn handle(&self, op: Op, sink: Sink) -> OpResult {
        let host = &*self.host;
        let now = clock::now();
        match op {
            Op::IslandPost { activity } => {
                validate_activity(&activity)?;
                let ttl = activity.ttl.is_some();
                self.island.post(activity);
                if ttl {
                    self.poke();
                }
                Ok(json!({}))
            }
            Op::IslandDismiss { activity_id } => {
                if let Some(id) = timer_id(&activity_id) {
                    if let Ok(list) = timers::list(host, now) {
                        if list
                            .iter()
                            .any(|t| t.id == id && t.state == trowel_proto::TimerState::Done)
                        {
                            let _ = timers::cancel(host, id);
                        }
                    }
                }
                Ok(json!({"dismissed": self.island.dismiss(&activity_id)}))
            }
            Op::IslandList => Ok(to_value(self.island.list())),
            Op::IslandAction {
                activity_id,
                action,
            } => self.island_action(&activity_id, &action),
            Op::ProcSubscribe {
                interval_ms,
                sort,
                group,
                all_users,
            } => {
                self.proc_subscribe(ProcSub {
                    sink,
                    interval: Duration::from_millis(u64::from(interval_ms.clamp(250, 10_000))),
                    sort,
                    group,
                    all_users,
                });
                Ok(json!({}))
            }
            Op::ProcUnsubscribe => {
                self.proc_forget(sink.id);
                Ok(json!({}))
            }
            Op::ProcDetail { pid } => self.proc_detail(pid),
            Op::ProcKill {
                pids,
                signal,
                confirm,
            } => self.proc_kill(&pids, signal, confirm.as_deref()),
            Op::SystemCommands => {
                let scripts = self.scripts();
                let (themes, current) = system::themes(host);
                *self.themes.lock().expect("themes") = themes.clone();
                Ok(json!({
                    "commands": system::curated(host),
                    "omarchy": scripts.iter().map(system::script_json).collect::<Vec<_>>(),
                    "themes": themes,
                    "current_theme": current,
                }))
            }
            Op::SystemRun { command_id } => {
                let scripts = self.scripts();
                let mut themes = self.themes.lock().expect("themes").clone();
                if command_id.starts_with("theme:") && themes.is_empty() {
                    themes = system::themes(host).0;
                }
                match system::plan(&command_id, &scripts, &themes) {
                    system::Plan::Unknown => Err((
                        ErrorCode::NotFound,
                        format!("no system command {command_id}"),
                    )),
                    system::Plan::Run(argv, true) => {
                        host.capture(&argv, Duration::from_secs(4)).map_err(io)?;
                        Ok(json!({"state": system::state_after(host, &command_id)}))
                    }
                    system::Plan::Run(argv, false) => {
                        host.spawn(&argv).map_err(io)?;
                        Ok(json!({}))
                    }
                }
            }
            Op::TimerStart {
                text,
                seconds,
                label,
            } => {
                let (secs, label) = match (text, seconds) {
                    (_, Some(s)) => (s, label.unwrap_or_default()),
                    (Some(text), None) => {
                        let spec = timers::parse(&text, now, true)
                            .ok_or_else(|| bad(format!("“{text}” isn't a duration or a time")))?;
                        (spec.seconds, label.unwrap_or(spec.label))
                    }
                    (None, None) => return Err(bad("timer_start needs text or seconds")),
                };
                let t = timers::start(host, secs, &label, now).map_err(bad)?;
                self.post_timer(&t);
                // Wrapped: a timer's own `id` would collide with the reply envelope's.
                Ok(json!({"timer": t}))
            }
            Op::TimerParse { text } => Ok(json!({"spec": timers::parse(&text, now, false)})),
            Op::TimerList => {
                if !host.state_exists() {
                    return Ok(json!({"timers": []}));
                }
                let list = timers::list(host, now).map_err(io)?;
                self.set_timers_live(!list.is_empty());
                Ok(json!({"timers": list}))
            }
            Op::TimerPause { timer_id } => self.timer_change(timer_id, timers::Change::Pause),
            Op::TimerResume { timer_id } => self.timer_change(timer_id, timers::Change::Resume),
            Op::TimerAdd { timer_id, seconds } => {
                if !(seconds.is_finite() && seconds > 0.0) {
                    return Err(bad("add a positive number of seconds"));
                }
                self.timer_change(timer_id, timers::Change::Add(seconds))
            }
            Op::TimerCancel { timer_id } => {
                let gone = timers::cancel(host, timer_id).map_err(io)?;
                self.island.dismiss(&timers::activity_id(timer_id));
                self.poke();
                Ok(json!({"cancelled": gone}))
            }
            Op::SnippetQuery { text, limit } => {
                Ok(json!({"rows": snippets::query(host, &text, limit).map_err(io)?}))
            }
            Op::SnippetGet { snippet_id } => snippets::get(host, snippet_id)
                .map_err(io)?
                .map(|s| json!({"snippet": s}))
                .ok_or((ErrorCode::NotFound, format!("no snippet {snippet_id}"))),
            Op::SnippetSave { snippet } => snippets::save(host, snippet, now as i64)
                .map(|s| json!({"snippet": s}))
                .map_err(bad),
            Op::SnippetDelete { snippet_id } => {
                Ok(json!({"deleted": snippets::delete(host, snippet_id).map_err(io)?}))
            }
            Op::SnippetPaste {
                snippet_id,
                mode,
                args,
            } => {
                let s = snippets::get(host, snippet_id)
                    .map_err(io)?
                    .ok_or((ErrorCode::NotFound, format!("no snippet {snippet_id}")))?;
                let clip = || host.clipboard_text();
                let text = snippets::expand(&s.body, &args, &clip, now).map_err(bad)?;
                host.paste_text(&text, mode == PasteMode::Copy)
                    .map_err(io)?;
                let _ = snippets::mark_used(host, snippet_id, now as i64);
                Ok(json!({"text": text}))
            }
            Op::SnippetImport { path } => {
                let text =
                    std::fs::read_to_string(&path).map_err(|e| io(format!("{path}: {e}")))?;
                let (imported, skipped) = snippets::import(host, &text, now as i64).map_err(bad)?;
                Ok(json!({"imported": imported, "skipped": skipped}))
            }
            Op::SnippetExport { path } => {
                let text = snippets::export(host).map_err(io)?;
                let n = serde_json::from_str::<Vec<Value>>(&text)
                    .map(|v| v.len())
                    .unwrap_or(0);
                std::fs::write(&path, text).map_err(|e| io(format!("{path}: {e}")))?;
                Ok(json!({"exported": n}))
            }
            other => Err((
                ErrorCode::UnknownOp,
                format!("{} is not a desk op", other.name()),
            )),
        }
    }

    fn timer_change(&self, id: i64, how: timers::Change) -> OpResult {
        let t = timers::change(&*self.host, id, how, clock::now()).map_err(bad)?;
        self.post_timer(&t);
        Ok(json!({"timer": t}))
    }

    fn island_action(&self, activity_id: &str, action: &str) -> OpResult {
        // Producers outside the daemon (`trowel island post`) hear about their buttons.
        self.island.notify(json!({
            "event": "island_action", "activity_id": activity_id, "action": action,
        }));
        if activity_id == "keylock" && action == "unlock" {
            self.host
                .spawn(&[self.host.trowel_exe(), "keyboard".into(), "unlock".into()])
                .map_err(io)?;
            self.island.dismiss("keylock");
            return Ok(json!({}));
        }
        if let Some(id) = timer_id(activity_id) {
            return match action {
                "pause" => self.timer_change(id, timers::Change::Pause),
                "resume" => self.timer_change(id, timers::Change::Resume),
                "add60" => self.timer_change(id, timers::Change::Add(60.0)),
                "add300" => self.timer_change(id, timers::Change::Add(300.0)),
                "cancel" | "dismiss" => {
                    let _ = timers::cancel(&*self.host, id);
                    self.island.dismiss(activity_id);
                    self.poke();
                    Ok(json!({}))
                }
                _ => Err(bad(format!("timers have no {action} button"))),
            };
        }
        if action == "dismiss" {
            self.island.dismiss(activity_id);
        }
        Ok(json!({}))
    }

    fn scripts(&self) -> Vec<system::Script> {
        let mut slot = self.scripts.lock().expect("scripts");
        if let Some((at, list)) = slot.as_ref() {
            if at.elapsed() < Duration::from_secs(60) {
                return list.clone();
            }
        }
        let list = system::scan(&self.host.script_dirs());
        *slot = Some((Instant::now(), list.clone()));
        list
    }

    fn proc_subscribe(&self, sub: ProcSub) {
        let start = {
            let mut mon = self.procs.lock().expect("procs");
            mon.subs
                .retain(|s| s.sink.id != sub.sink.id && s.sink.alive());
            mon.subs.push(sub);
            let start = !mon.running;
            mon.running = true;
            if mon.sampler.is_none() {
                mon.sampler = Some(procs::Sampler::new(self.host.proc_source()));
            }
            start
        };
        if start {
            let mon = Arc::clone(&self.procs);
            let pid = std::process::id() as i32;
            std::thread::Builder::new()
                .name("desk-procs".into())
                .spawn(move || proc_loop(mon, pid))
                .expect("proc sampler");
        }
    }

    fn proc_forget(&self, sink_id: u64) {
        self.procs
            .lock()
            .expect("procs")
            .subs
            .retain(|s| s.sink.id != sink_id);
    }

    /// The sampler with a sample under 2 s old (one taken now when there's none).
    fn with_sampler<T>(&self, f: impl FnOnce(&mut procs::Sampler) -> T) -> T {
        let mut mon = self.procs.lock().expect("procs");
        let fresh = mon.running;
        let sampler = mon
            .sampler
            .get_or_insert_with(|| procs::Sampler::new(self.host.proc_source()));
        if !fresh || sampler.last.is_empty() {
            sampler.sample(true);
        }
        f(sampler)
    }

    fn proc_detail(&self, pid: i32) -> OpResult {
        self.with_sampler(|s| {
            let mut d = s
                .source
                .detail(pid)
                .ok_or((ErrorCode::NotFound, format!("no process {pid}")))?;
            let app = s.raw(pid).map(procs::app_key);
            d["user"] = json!(s.raw(pid).map(|r| r.uid).map(|u| s.source.user_name(u)));
            d["engines"] = json!(s
                .engines(pid)
                .into_iter()
                .map(|(e, v)| json!({"engine": e, "busy": v}))
                .collect::<Vec<_>>());
            d["history"] = json!(s.history.get(&format!("pid:{pid}")));
            d["app_key"] = json!(app);
            d["app_history"] = json!(app.as_ref().and_then(|k| s.history.get(k)));
            d["protected"] = json!(s.raw(pid).map(|r| procs::protected(&r.comm)));
            Ok(d)
        })
    }

    fn proc_kill(&self, pids: &[i32], signal: KillSignal, confirm: Option<&str>) -> OpResult {
        let daemon = std::process::id() as i32;
        let alive = self.with_sampler(|s| {
            if let Some(why) = procs::kill_check(s, pids, confirm, daemon) {
                return Err((ErrorCode::NotAllowed, why));
            }
            let mut errors = Vec::new();
            for &pid in pids {
                if let Err(e) = s.source.kill(pid, signal) {
                    if s.source.alive(pid) {
                        errors.push(format!("{pid}: {e}"));
                    }
                }
            }
            if !errors.is_empty() && errors.len() == pids.len() {
                return Err(io(errors.join("; ")));
            }
            Ok(())
        });
        alive?;
        // Up to 500 ms for SIGTERM to land; the panel offers Force Quit for what's left.
        let deadline = Instant::now() + Duration::from_millis(500);
        loop {
            let left: Vec<i32> = {
                let mut mon = self.procs.lock().expect("procs");
                match mon.sampler.as_mut() {
                    Some(s) => pids
                        .iter()
                        .copied()
                        .filter(|&p| s.source.alive(p))
                        .collect(),
                    None => Vec::new(),
                }
            };
            if left.is_empty() || Instant::now() >= deadline {
                return Ok(json!({"sent": pids.len(), "alive": left}));
            }
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// The connection closed: drop its subscriptions.
    pub fn forget(&self, sink: &Sink) {
        self.island.forget(sink);
        self.proc_forget(sink.id);
    }
}

fn proc_loop(mon: Arc<Mutex<ProcMon>>, _daemon_pid: i32) {
    loop {
        let interval = {
            let mut m = mon.lock().expect("procs");
            m.subs.retain(|s| s.sink.alive());
            if m.subs.is_empty() {
                // Nothing runs while nobody looks (BENCHMARKS: zero cost when closed).
                m.running = false;
                m.sampler = None;
                return;
            }
            let all = m.subs.iter().any(|s| s.all_users);
            let ProcMon { sampler, subs, .. } = &mut *m;
            let Some(sampler) = sampler.as_mut() else {
                m.running = false;
                return;
            };
            sampler.sample(all);
            for s in subs.iter() {
                let rows = sampler.rows(s.sort, s.group, s.all_users, 200);
                s.sink.send(
                    json!({
                        "event": "proc",
                        "sort": s.sort,
                        "group": s.group,
                        "totals": sampler.totals,
                        "rows": rows,
                        "cost_ms": sampler.last_cost.as_secs_f64() * 1000.0,
                    })
                    .to_string(),
                );
            }
            subs.iter()
                .map(|s| s.interval)
                .min()
                .unwrap_or(Duration::from_secs(1))
        };
        std::thread::sleep(interval);
    }
}

fn ticker(desk: Weak<Desk>, wake: Wake) {
    loop {
        let wait = match desk.upgrade() {
            Some(d) => {
                d.tick();
                d.next_wait()
            }
            None => return,
        };
        let (lock, cv) = &*wake;
        let mut poked = lock.lock().expect("wake");
        if !*poked {
            poked = cv.wait_timeout(poked, wait).expect("wake wait").0;
        }
        *poked = false;
    }
}

fn timer_id(activity_id: &str) -> Option<i64> {
    activity_id.strip_prefix("timer:")?.parse().ok()
}

fn validate_activity(a: &IslandActivity) -> Result<(), (ErrorCode, String)> {
    if a.id.trim().is_empty() || a.id.len() > 64 {
        return Err(bad("an activity needs an id of 1 to 64 characters"));
    }
    if a.title.chars().count() > 120 || a.subtitle.as_deref().unwrap_or("").chars().count() > 200 {
        return Err(bad("title ≤ 120 and subtitle ≤ 200 characters"));
    }
    if a.actions.len() > 3 {
        return Err(bad("at most 3 buttons"));
    }
    if a.progress.is_some_and(|p| !(0.0..=1.0).contains(&p)) {
        return Err(bad("progress is 0 to 1"));
    }
    Ok(())
}

/// The keyboard lock's island (D27 as amended by P-D29): a countdown and Unlock.
pub fn keylock_activity(until: f64, seconds: Option<f64>) -> IslandActivity {
    serde_json::from_value(json!({
        "id": "keylock", "icon": "lock", "title": "Keyboard locked",
        "subtitle": "The mouse still works", "ends_at": until, "total": seconds,
        "priority": 100, "actions": [{"id": "unlock", "label": "Unlock"}],
    }))
    .expect("keylock activity")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    struct TestHost {
        dir: PathBuf,
        state: Mutex<Option<StateDb>>,
        log: Mutex<Vec<String>>,
        keylock: Option<Value>,
    }

    impl TestHost {
        fn new(keylock: Option<Value>) -> Arc<Self> {
            let dir = std::env::temp_dir().join(format!(
                "trowel-desk-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            Arc::new(Self {
                dir,
                state: Mutex::new(None),
                log: Mutex::new(Vec::new()),
                keylock,
            })
        }
        fn logged(&self) -> Vec<String> {
            self.log.lock().unwrap().clone()
        }
    }

    impl Host for TestHost {
        fn with_state(
            &self,
            f: &mut dyn FnMut(&StateDb) -> Result<Value, trowel_core::DbError>,
        ) -> Result<Value, String> {
            let mut slot = self.state.lock().unwrap();
            if slot.is_none() {
                *slot = Some(StateDb::open(&self.dir).map_err(|e| e.to_string())?);
            }
            f(slot.as_ref().unwrap()).map_err(|e| e.to_string())
        }
        fn paste_text(&self, text: &str, copy_only: bool) -> Result<(), String> {
            self.log.lock().unwrap().push(format!(
                "{} {text}",
                if copy_only { "copy" } else { "paste" }
            ));
            Ok(())
        }
        fn spawn(&self, argv: &[String]) -> Result<(), String> {
            self.log.lock().unwrap().push(argv.join(" "));
            Ok(())
        }
        fn capture(&self, argv: &[String], _t: Duration) -> Result<String, String> {
            self.log
                .lock()
                .unwrap()
                .push(format!("capture {}", argv.join(" ")));
            Ok(match argv[0].as_str() {
                "omarchy-theme-list" => "Catppuccin\nTokyo Night\n".into(),
                "omarchy-toggle-nightlight" => r#"{"enabled":true}"#.into(),
                _ => String::new(),
            })
        }
        fn home(&self) -> PathBuf {
            self.dir.clone()
        }
        fn script_dirs(&self) -> Vec<PathBuf> {
            vec![self.dir.join("bin")]
        }
        fn keylock_state(&self) -> Option<Value> {
            self.keylock.clone()
        }
        fn proc_source(&self) -> Box<dyn procs::Source> {
            Box::<procs::Fake>::default()
        }
        fn clipboard_text(&self) -> String {
            "CLIP".into()
        }
        fn trowel_exe(&self) -> String {
            "trowel".into()
        }
        fn timer_sound(&self) -> bool {
            true
        }
        fn state_exists(&self) -> bool {
            self.dir.join("state.sqlite").exists()
        }
    }

    fn sink() -> (Sink, Arc<Mutex<Vec<Value>>>) {
        let got = Arc::new(Mutex::new(Vec::new()));
        let g = Arc::clone(&got);
        (
            Sink::new(
                move |l| g.lock().unwrap().push(serde_json::from_str(&l).unwrap()),
                || true,
            ),
            got,
        )
    }

    fn op(v: Value) -> Op {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn keylock_reposts_and_unlock_runs_trowel() {
        let host = TestHost::new(Some(json!({"until": clock::now() + 60.0, "seconds": 60})));
        let desk = Desk::new(host.clone());
        assert_eq!(desk.island.list().activities[0].id, "keylock");
        let (s, _) = sink();
        desk.handle(
            op(json!({"op": "island_action", "activity_id": "keylock", "action": "unlock"})),
            s,
        )
        .unwrap();
        assert!(host
            .logged()
            .contains(&"trowel keyboard unlock".to_string()));
        assert!(desk.island.list().activities.is_empty());
    }

    #[test]
    fn timer_fires_onto_the_island_with_sound_and_notification() {
        let host = TestHost::new(None);
        let desk = Desk::new(host.clone());
        let (s, got) = sink();
        desk.island.subscribe(s.clone());
        let t = desk
            .handle(
                op(json!({"op": "timer_start", "text": "1s tea"})),
                s.clone(),
            )
            .unwrap()["timer"]
            .clone();
        assert_eq!(t["label"], "tea");
        assert_eq!(
            desk.island.list().activities[0].id,
            format!("timer:{}", t["id"])
        );
        let fired = Instant::now();
        while Instant::now().duration_since(fired) < Duration::from_secs(3) {
            if desk
                .island
                .list()
                .activities
                .first()
                .is_some_and(|a| a.alert)
            {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let ends = t["ends_at"].as_f64().unwrap();
        let a = &desk.island.list().activities[0];
        assert!(a.alert, "fired");
        assert!(clock::now() - ends < 0.5, "within 500 ms of its end");
        let log = host.logged();
        assert!(
            log.iter()
                .any(|l| l.starts_with("omarchy-notification-send")),
            "{log:?}"
        );
        assert!(log.iter().any(|l| l.starts_with("pw-play")), "{log:?}");
        assert!(got.lock().unwrap().len() >= 3);
        // Dismiss from the island removes the finished timer.
        desk.handle(
            op(json!({"op": "island_action", "activity_id": a.id, "action": "dismiss"})),
            s.clone(),
        )
        .unwrap();
        let list = desk.handle(op(json!({"op": "timer_list"})), s).unwrap();
        assert!(list["timers"].as_array().unwrap().is_empty());
    }

    #[test]
    fn timers_pause_resume_add_and_survive_a_new_desk() {
        let host = TestHost::new(None);
        let desk = Desk::new(host.clone());
        let (s, _) = sink();
        let t = desk
            .handle(
                op(json!({"op": "timer_start", "seconds": 120, "label": "x"})),
                s.clone(),
            )
            .unwrap()["timer"]
            .clone();
        let id = t["id"].as_i64().unwrap();
        let p = desk
            .handle(op(json!({"op": "timer_pause", "timer_id": id})), s.clone())
            .unwrap()["timer"]
            .clone();
        assert_eq!(p["state"], "paused");
        assert!(p.get("ends_at").is_none());
        let a = desk
            .handle(
                op(json!({"op": "timer_add", "timer_id": id, "seconds": 60})),
                s.clone(),
            )
            .unwrap()["timer"]
            .clone();
        assert!((a["left"].as_f64().unwrap() - 180.0).abs() < 1.0);
        let r = desk
            .handle(op(json!({"op": "timer_resume", "timer_id": id})), s.clone())
            .unwrap()["timer"]
            .clone();
        assert_eq!(r["state"], "running");
        // A restart: the row is still there and the island gets it back on the next start.
        drop(desk);
        let again = Desk::new(host.clone());
        let list = again.handle(op(json!({"op": "timer_list"})), s).unwrap();
        assert_eq!(list["timers"][0]["id"], id);
    }

    #[test]
    fn system_commands_are_ids_only() {
        let host = TestHost::new(None);
        std::fs::create_dir_all(host.dir.join("bin")).unwrap();
        std::fs::write(host.dir.join("bin/omarchy-hello"), "# omarchy:summary=Hi\n").unwrap();
        let desk = Desk::new(host.clone());
        let (s, _) = sink();
        let list = desk
            .handle(op(json!({"op": "system_commands"})), s.clone())
            .unwrap();
        let night = list["commands"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["id"] == "night-light")
            .unwrap();
        assert_eq!(night["state"], true);
        assert_eq!(list["omarchy"][0]["id"], "omarchy:omarchy-hello");
        assert_eq!(list["themes"][1], "Tokyo Night");
        let r = desk
            .handle(
                op(json!({"op": "system_run", "command_id": "night-light"})),
                s.clone(),
            )
            .unwrap();
        assert_eq!(r["state"], true);
        assert!(host
            .logged()
            .contains(&"capture omarchy-toggle-nightlight".to_string()));
        desk.handle(
            op(json!({"op": "system_run", "command_id": "lock-screen"})),
            s.clone(),
        )
        .unwrap();
        assert!(host.logged().contains(&"omarchy-system-lock".to_string()));
        let err = desk
            .handle(
                op(json!({"op": "system_run", "command_id": "rm -rf ~"})),
                s.clone(),
            )
            .unwrap_err();
        assert_eq!(err.0, ErrorCode::NotFound);
        desk.handle(
            op(json!({"op": "system_run", "command_id": "theme:Tokyo Night"})),
            s,
        )
        .unwrap();
        assert!(host
            .logged()
            .contains(&"omarchy-theme-set Tokyo Night".to_string()));
    }

    #[test]
    fn snippets_save_query_paste_import_export() {
        let host = TestHost::new(None);
        let desk = Desk::new(host.clone());
        let (s, _) = sink();
        let saved = desk
            .handle(
                op(
                    json!({"op": "snippet_save", "snippet": {"name": "Signature", "keyword": ";sig",
                    "body": "Best, {argument:who}\n{clipboard}"}}),
                ),
                s.clone(),
            )
            .unwrap()["snippet"]
            .clone();
        assert_eq!(saved["arguments"], json!(["who"]));
        let id = saved["id"].as_i64().unwrap();
        let dup = desk.handle(
            op(json!({"op": "snippet_save", "snippet": {"name": "Other", "keyword": ";SIG", "body": "x"}})),
            s.clone(),
        );
        assert!(dup.unwrap_err().1.contains("already uses"));
        let q = desk
            .handle(
                op(json!({"op": "snippet_query", "text": ";sig"})),
                s.clone(),
            )
            .unwrap();
        assert_eq!(q["rows"][0]["id"], id);
        let q = desk
            .handle(
                op(json!({"op": "snippet_query", "text": "best"})),
                s.clone(),
            )
            .unwrap();
        assert_eq!(q["rows"][0]["id"], id);
        let missing = desk.handle(
            op(json!({"op": "snippet_paste", "snippet_id": id})),
            s.clone(),
        );
        assert_eq!(missing.unwrap_err().1, "needs who");
        let pasted = desk
            .handle(
                op(json!({"op": "snippet_paste", "snippet_id": id, "args": {"who": "Ana"}})),
                s.clone(),
            )
            .unwrap();
        assert_eq!(pasted["text"], "Best, Ana\nCLIP");
        assert!(host.logged().contains(&"paste Best, Ana\nCLIP".to_string()));
        let file = host.dir.join("raycast.json");
        std::fs::write(
            &file,
            r#"[{"name":"Addr","text":"1 Main St","keyword":"!addr"},{"name":"Signature","text":"Best, {argument:who}\n{clipboard}","keyword":";sig"}]"#,
        )
        .unwrap();
        let imp = desk
            .handle(op(json!({"op": "snippet_import", "path": file})), s.clone())
            .unwrap();
        assert_eq!(
            (imp["imported"].as_u64(), imp["skipped"].as_u64()),
            (Some(1), Some(1))
        );
        let out = host.dir.join("out.json");
        let exp = desk
            .handle(op(json!({"op": "snippet_export", "path": out})), s.clone())
            .unwrap();
        assert_eq!(exp["exported"], 2);
        let back: Vec<Value> =
            serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert!(back
            .iter()
            .any(|v| v["keyword"] == "!addr" && v["text"] == "1 Main St"));
        assert_eq!(
            desk.handle(op(json!({"op": "snippet_delete", "snippet_id": id})), s)
                .unwrap()["deleted"],
            true
        );
    }

    #[test]
    fn proc_subscribe_streams_and_kill_follows_policy() {
        let host = TestHost::new(None);
        let desk = Desk::new(host.clone());
        let (s, got) = sink();
        desk.handle(
            op(json!({"op": "proc_subscribe", "interval_ms": 250, "sort": "cpu", "group": "pid"})),
            s.clone(),
        )
        .unwrap();
        let t0 = Instant::now();
        while got.lock().unwrap().len() < 2 && t0.elapsed() < Duration::from_secs(3) {
            std::thread::sleep(Duration::from_millis(20));
        }
        let last = got.lock().unwrap().last().cloned().unwrap();
        assert_eq!(last["event"], "proc");
        assert_eq!(last["rows"][0]["name"], "yes");
        let refused = desk
            .handle(
                op(json!({"op": "proc_kill", "pids": [1300], "signal": "term"})),
                s.clone(),
            )
            .unwrap_err();
        assert_eq!(refused.0, ErrorCode::NotAllowed);
        let killed = desk
            .handle(
                op(json!({"op": "proc_kill", "pids": [1100], "signal": "term"})),
                s.clone(),
            )
            .unwrap();
        assert_eq!(killed["alive"], json!([]));
        let d = desk
            .handle(op(json!({"op": "proc_detail", "pid": 1200})), s.clone())
            .unwrap();
        assert_eq!(d["engines"][0]["engine"], "ccs");
        desk.handle(op(json!({"op": "proc_unsubscribe"})), s)
            .unwrap();
        std::thread::sleep(Duration::from_millis(400));
        assert!(
            !desk.procs.lock().unwrap().running,
            "the sampler stops with no one watching"
        );
    }
}
