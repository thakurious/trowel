//! `upstream/Sources/digup/IndexService.swift`

use std::collections::{BTreeMap, HashSet};
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use trowel_core::{Indexer, ModelFiles, SharedEmbedder};

pub struct Part {
    pub indexer: Indexer,
    pub roots: Vec<PathBuf>,
    pub keep_only: Option<Vec<PathBuf>>,
    pub code: bool,
}

struct Flags {
    sync_requested: bool,
    code_sync_requested: bool,
    stop_requested: bool,
    backfill_allowed: bool,
    /// A sync asked for while files are being indexed waits until then. A crawl of a big folder can take a
    /// while, and a folder that keeps changing asks for the next one meanwhile: yielding at once, every pass
    /// went back to crawling before a file was read. Indexing gets at least as long as the sync before it
    /// (2 s at the least). `upstream/Sources/digup/IndexService.swift:55` `yieldAt` (75d95d0).
    yield_at: Option<Instant>,
}

impl Flags {
    /// A new sync may interrupt indexing now (`Date() >= yieldAt` upstream).
    fn sync_due(&self, code: bool) -> bool {
        self.yield_at.is_none_or(|at| Instant::now() >= at)
            && (self.sync_requested || (code && self.code_sync_requested))
    }
}

pub struct IndexService {
    main: Part,
    code: Option<Part>,
    model: SharedEmbedder,
    model_ready: Box<dyn Fn() -> bool + Send>,
    new_files_limit: usize,
    linger: Duration,
    flags: Arc<Mutex<Flags>>,
    wake: Arc<Condvar>,
    unavailable: BTreeMap<bool, (Vec<String>, Vec<String>)>,
}

impl IndexService {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        main: Part,
        code: Option<Part>,
        model: SharedEmbedder,
        model_ready: impl Fn() -> bool + Send + 'static,
        backfill: bool,
        new_files_limit: usize,
        linger: f64,
    ) -> Self {
        Self {
            main,
            code,
            model,
            model_ready: Box::new(model_ready),
            new_files_limit,
            linger: Duration::from_secs_f64(linger.max(0.0)),
            flags: Arc::new(Mutex::new(Flags {
                sync_requested: true,
                code_sync_requested: true,
                stop_requested: false,
                backfill_allowed: backfill,
                yield_at: None,
            })),
            wake: Arc::new(Condvar::new()),
            unavailable: BTreeMap::new(),
        }
    }

    pub fn serve(mut self) -> i32 {
        remove_stale_scratch_folders();
        let parts_code = [false, true];
        for is_code in parts_code {
            let part = if is_code {
                self.code.as_mut()
            } else {
                Some(&mut self.main)
            };
            let Some(part) = part else { continue };
            if let Err(err) = part.indexer.check_model() {
                emit(&json!({"event": "error", "message": err.to_string(), "fatal": true}));
                self.finish();
                return 1;
            }
            if let Some(keep) = &part.keep_only {
                let roots: Vec<String> = keep
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
                match part.indexer.store.prune(&roots) {
                    Ok(n) if n > 0 => {
                        emit(&json!({"event": "pruned", "files": n, "code": part.code}));
                    }
                    Err(err) => {
                        emit(&json!({"event": "error", "message": err.to_string(), "fatal": true}));
                        self.finish();
                        return 1;
                    }
                    _ => {}
                }
            }
        }
        self.spawn_commands();
        let mut new_files: HashSet<i64> = HashSet::new();
        let mut new_code: HashSet<i64> = HashSet::new();
        let mut code_counted = self.code.is_none();
        loop {
            let sync_now = {
                let mut g = self.flags.lock().unwrap();
                let v = g.sync_requested;
                g.sync_requested = false;
                v
            };
            if sync_now {
                self.sync_main(&mut new_files);
            }
            // The code folders are crawled with the first sync, so their counts show from the start
            // (a big first pass of the main index takes hours); their files are read once the main
            // index has nothing waiting, below. `upstream/Sources/digup/IndexService.swift` (bb7e753).
            if !code_counted {
                code_counted = true;
                let code_sync = {
                    let mut g = self.flags.lock().unwrap();
                    std::mem::take(&mut g.code_sync_requested)
                };
                if code_sync {
                    self.sync_code(&mut new_code);
                }
            }
            if self.flags.lock().unwrap().stop_requested {
                break;
            }
            if !(self.model_ready)() {
                self.emit_counts("waiting", json!({"for": "model"}));
                break;
            }
            if !new_files.is_empty() {
                if !self.run_main(Some(new_files.clone())) {
                    break;
                }
                new_files = self.left_main(&new_files);
                continue;
            }
            let pending = self.main.indexer.store.pending_count().unwrap_or(0);
            let backfill = self.flags.lock().unwrap().backfill_allowed;
            if pending > 0 && backfill {
                if !self.run_main(None) {
                    break;
                }
                if self.flags.lock().unwrap().sync_requested {
                    continue;
                }
            }
            if self.code.is_some()
                && (self.main.indexer.store.pending_count().unwrap_or(1) == 0 || !backfill)
            {
                let code_sync = {
                    let mut g = self.flags.lock().unwrap();
                    let v = g.code_sync_requested;
                    g.code_sync_requested = false;
                    v
                };
                if code_sync {
                    self.sync_code(&mut new_code);
                }
                if !new_code.is_empty() {
                    if !self.run_code(Some(new_code.clone())) {
                        break;
                    }
                    new_code = self.left_code(&new_code);
                    continue;
                }
                let code_pending = self
                    .code
                    .as_ref()
                    .and_then(|c| c.indexer.store.pending_count().ok())
                    .unwrap_or(0);
                if code_pending > 0 && self.flags.lock().unwrap().backfill_allowed {
                    if !self.run_code(None) {
                        break;
                    }
                    let g = self.flags.lock().unwrap();
                    if g.sync_requested || g.code_sync_requested {
                        continue;
                    }
                }
            }
            let backfill = self.flags.lock().unwrap().backfill_allowed;
            let unfinished = self
                .main
                .indexer
                .store
                .unfinished_count()
                .map(|c| c.0)
                .unwrap_or(0);
            if backfill
                && self.main.indexer.store.pending_count().unwrap_or(1) == 0
                && self
                    .code
                    .as_ref()
                    .map(|c| c.indexer.store.pending_count().unwrap_or(0))
                    .unwrap_or(0)
                    == 0
                && unfinished > 0
            {
                if !self.finish_long_files() {
                    break;
                }
                let g = self.flags.lock().unwrap();
                if g.sync_requested || g.code_sync_requested {
                    continue;
                }
            }
            if self.code.is_some() && self.flags.lock().unwrap().code_sync_requested {
                continue;
            }
            self.emit_counts("idle", json!({}));
            if !self.model.is_running() || !self.wait_for_work() {
                break;
            }
        }
        self.finish();
        emit(&json!({"event": "exit"}));
        0
    }

    fn finish(&mut self) {
        self.main.indexer.finish();
        if let Some(code) = &mut self.code {
            code.indexer.finish();
        }
        self.model.shutdown();
    }

    fn spawn_commands(&self) {
        let flags = self.flags.clone();
        let wake = self.wake.clone();
        std::thread::spawn(move || {
            let stdin = io::stdin();
            for line in stdin.lock().lines() {
                let Ok(line) = line else { break };
                {
                    let mut g = flags.lock().unwrap();
                    match line.trim() {
                        "sync" => g.sync_requested = true,
                        "sync code" => g.code_sync_requested = true,
                        "stop" => g.stop_requested = true,
                        "backfill on" => g.backfill_allowed = true,
                        "backfill off" => g.backfill_allowed = false,
                        _ => {}
                    }
                }
                wake.notify_one();
            }
            flags.lock().unwrap().stop_requested = true;
            wake.notify_one();
        });
    }

    fn sync_main(&mut self, new_files: &mut HashSet<i64>) {
        sync_part(
            &mut self.main,
            new_files,
            self.new_files_limit,
            &mut self.unavailable,
            &self.flags,
        );
    }

    fn sync_code(&mut self, new_files: &mut HashSet<i64>) {
        if let Some(code) = &mut self.code {
            sync_part(
                code,
                new_files,
                self.new_files_limit,
                &mut self.unavailable,
                &self.flags,
            );
        }
    }

    fn run_main(&mut self, only: Option<HashSet<i64>>) -> bool {
        run_part(&mut self.main, only, &self.flags)
    }

    fn run_code(&mut self, only: Option<HashSet<i64>>) -> bool {
        let Some(code) = self.code.as_mut() else {
            return true;
        };
        run_part(code, only, &self.flags)
    }

    fn left_main(&self, files: &HashSet<i64>) -> HashSet<i64> {
        left(&self.main, files, &self.flags)
    }

    fn left_code(&self, files: &HashSet<i64>) -> HashSet<i64> {
        self.code
            .as_ref()
            .map(|c| left(c, files, &self.flags))
            .unwrap_or_default()
    }

    fn finish_long_files(&mut self) -> bool {
        let started = Instant::now();
        let flags = self.flags.clone();
        let has_code = self.code.is_some();
        let result = self.main.indexer.finish_long_files(
            &|| {
                let g = flags.lock().unwrap();
                g.stop_requested || !g.backfill_allowed || g.sync_due(has_code)
            },
            |step| {
                emit(&json!({
                    "event": "rest",
                    "done": step.done,
                    "total": step.total,
                    "files": step.files,
                    "path": step.path,
                    "kind": step.kind.as_str(),
                    "state": step.state,
                    "seconds": step.seconds,
                    "note": step.note.unwrap_or_default(),
                }));
            },
        );
        match result {
            Ok(states) if !states.is_empty() => {
                emit(&json!({
                    "event": "summary",
                    "rest": true,
                    "states": states,
                    "ms": started.elapsed().as_millis() as u64,
                }));
                true
            }
            Ok(_) => true,
            Err(err) => {
                emit(&json!({"event": "error", "message": err.to_string(), "fatal": true}));
                false
            }
        }
    }

    fn wait_for_work(&self) -> bool {
        let deadline = Instant::now() + self.linger;
        let mut g = self.flags.lock().unwrap();
        loop {
            if g.stop_requested {
                return false;
            }
            if g.sync_requested || (g.code_sync_requested && self.code.is_some()) {
                return true;
            }
            if g.backfill_allowed {
                let pending = self.main.indexer.store.pending_count().unwrap_or(0) > 0
                    || self
                        .code
                        .as_ref()
                        .and_then(|c| c.indexer.store.pending_count().ok())
                        .unwrap_or(0)
                        > 0
                    || self
                        .main
                        .indexer
                        .store
                        .unfinished_count()
                        .map(|c| c.0)
                        .unwrap_or(0)
                        > 0;
                if pending {
                    return true;
                }
            }
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let (guard, timed) = self
                .wake
                .wait_timeout(g, deadline.saturating_duration_since(now))
                .unwrap();
            g = guard;
            if timed.timed_out() {
                return false;
            }
        }
    }

    fn emit_counts(&self, event: &str, extra: Value) {
        let mut fields = extra.as_object().cloned().unwrap_or_default();
        fields.insert("event".into(), json!(event));
        for (code, part) in [(false, Some(&self.main)), (true, self.code.as_ref())] {
            let Some(part) = part else { continue };
            let Ok(counts) = part.indexer.store.counts() else {
                continue;
            };
            let prefix = if code { "code_" } else { "" };
            let mut files = 0usize;
            let mut searchable = 0usize;
            let mut pending = 0usize;
            for states in counts.by_kind_and_state.values() {
                for (state, n) in states {
                    files += n;
                    if state == "done" || state == "partial" {
                        searchable += n;
                    }
                    if state == "pending" {
                        pending += n;
                    }
                }
            }
            fields.insert(format!("{prefix}files"), json!(files));
            fields.insert(format!("{prefix}searchable"), json!(searchable));
            fields.insert(format!("{prefix}pending"), json!(pending));
            fields.insert(format!("{prefix}vectors"), json!(counts.vectors));
            if !code {
                fields.insert("unfinished".into(), json!(counts.unfinished));
            }
        }
        emit(&Value::Object(fields));
    }
}

fn sync_part(
    part: &mut Part,
    new_files: &mut HashSet<i64>,
    new_files_limit: usize,
    unavailable: &mut BTreeMap<bool, (Vec<String>, Vec<String>)>,
    flags: &Arc<Mutex<Flags>>,
) {
    let started = Instant::now();
    let _yield_later = YieldAfter { started, flags };
    let started_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let roots = available_roots(part, unavailable);
    match part.indexer.sync(&roots) {
        Ok(summary) => {
            if summary.queued.len() <= new_files_limit {
                new_files.extend(summary.queued.iter().copied());
            }
            emit(&json!({
                "event": "sync",
                "code": part.code,
                "new": new_files.len(),
                "added": summary.added,
                "changed": summary.changed,
                "moved": summary.moved,
                "removed": summary.removed,
                "unchanged": summary.unchanged,
                "ms": started.elapsed().as_millis() as u64,
                "started": started_unix,
            }));
        }
        Err(err) => {
            emit(&json!({
                "event": "error",
                "message": format!("sync failed: {err}"),
                "code": part.code,
            }));
        }
    }
}

/// Sets `yield_at` when the sync returns, however it returns (`defer` upstream).
struct YieldAfter<'a> {
    started: Instant,
    flags: &'a Arc<Mutex<Flags>>,
}

impl Drop for YieldAfter<'_> {
    fn drop(&mut self) {
        let took = self.started.elapsed().max(Duration::from_secs(2));
        if let Ok(mut g) = self.flags.lock() {
            g.yield_at = Some(Instant::now() + took);
        }
    }
}

fn available_roots(
    part: &mut Part,
    unavailable: &mut BTreeMap<bool, (Vec<String>, Vec<String>)>,
) -> Vec<PathBuf> {
    let missing: Vec<String> = part
        .roots
        .iter()
        .filter(|r| !r.is_dir())
        .map(|r| r.to_string_lossy().into_owned())
        .collect();
    let unreadable: Vec<String> = part
        .roots
        .iter()
        .filter(|r| r.is_dir() && std::fs::read_dir(r).is_err())
        .map(|r| r.to_string_lossy().into_owned())
        .collect();
    let report = (missing.clone(), unreadable.clone());
    let changed = unavailable
        .get(&part.code)
        .map(|prev| prev != &report)
        .unwrap_or(!(missing.is_empty() && unreadable.is_empty()));
    if changed {
        emit(&json!({
            "event": "missing",
            "folders": missing,
            "unreadable": unreadable,
            "code": part.code,
        }));
    }
    unavailable.insert(part.code, report);
    let available: Vec<PathBuf> = part
        .roots
        .iter()
        .filter(|r| r.is_dir() && std::fs::read_dir(r).is_ok())
        .cloned()
        .collect();
    let scope: Vec<String> = available
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    part.indexer.store.set_scope(Some(scope));
    available
}

fn run_part(part: &mut Part, only: Option<HashSet<i64>>, flags: &Arc<Mutex<Flags>>) -> bool {
    let started = Instant::now();
    let is_new = only.is_some();
    let code = part.code;
    let stop = {
        let flags = flags.clone();
        move || {
            let g = flags.lock().unwrap();
            g.stop_requested || (!is_new && !g.backfill_allowed) || g.sync_due(code)
        }
    };
    let only_ref = only.as_ref();
    match part.indexer.run(None, only_ref, &stop, |event| {
        emit(&json!({
            "event": "file",
            "new": is_new,
            "done": event.done,
            "total": event.total,
            "path": event.path,
            "kind": event.kind.as_str(),
            "state": event.state,
            "seconds": event.seconds,
            "note": event.note.unwrap_or_default(),
            "code": code,
        }));
    }) {
        Ok(states) => {
            if !states.is_empty() {
                emit(&json!({
                    "event": "summary",
                    "new": is_new,
                    "states": states,
                    "code": code,
                    "ms": started.elapsed().as_millis() as u64,
                }));
            }
            true
        }
        Err(err) => {
            part.indexer.close_worker();
            emit(&json!({
                "event": "error",
                "message": err.to_string(),
                "fatal": true,
                "code": code,
            }));
            false
        }
    }
}

fn left(part: &Part, files: &HashSet<i64>, flags: &Arc<Mutex<Flags>>) -> HashSet<i64> {
    let yielded = {
        let g = flags.lock().unwrap();
        g.sync_requested || g.stop_requested || (part.code && g.code_sync_requested)
    };
    if !yielded {
        return HashSet::new();
    }
    part.indexer
        .store
        .pending(None, Some(files))
        .unwrap_or_default()
        .into_iter()
        .map(|f| f.id)
        .collect()
}

fn remove_stale_scratch_folders() {
    let temp = std::env::temp_dir();
    let Ok(entries) = std::fs::read_dir(&temp) else {
        return;
    };
    let me = std::process::id();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(rest) = name.strip_prefix("trowel-") else {
            continue;
        };
        let Ok(pid) = rest.parse::<u32>() else {
            continue;
        };
        if pid == me {
            continue;
        }
        if Path::new(&format!("/proc/{pid}")).exists() {
            continue;
        }
        let _ = std::fs::remove_dir_all(entry.path());
    }
}

pub fn emit(fields: &Value) {
    let mut stdout = io::stdout().lock();
    let _ = writeln!(stdout, "{fields}");
    let _ = stdout.flush();
}

pub fn model_is_ready() -> bool {
    ModelFiles::complete()
}
