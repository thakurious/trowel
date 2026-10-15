//! Canned PROTOCOL v1 daemon. `PROTOCOL.md` §7; `EXECUTION-PLAN.md` Phase 1 task 6.

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::io::{self, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde::Deserialize;
use serde_json::{json, Value};
use trowel_proto::ops::KNOWN_OPS;
use trowel_proto::{
    Config, ErrorBody, ErrorCode, Meaning, ModelStatus, Op, Request, ResultGroup, ResultRow,
    SearchResult, Status, MAX_LINE_BYTES, PROTOCOL_VERSION,
};

const QUEUE_CAP: usize = 256;
const DEFAULT_FIXTURE: &str = include_str!("fixture_default.json");
const DAEMON_VERSION: &str = env!("CARGO_PKG_VERSION");
const CAPABILITIES: &[&str] = &["code", "audio", "video", "ocr"];

pub struct MockOpts {
    pub socket: Option<PathBuf>,
    pub fixture: Option<PathBuf>,
    pub latency_ms: u64,
}

#[derive(Clone, Deserialize)]
struct CannedSearch {
    #[serde(rename = "match")]
    needle: String,
    #[serde(default)]
    groups: Vec<ResultGroup>,
    #[serde(default)]
    weaker: Vec<ResultRow>,
    #[serde(default = "skipped")]
    meaning: Meaning,
}

fn skipped() -> Meaning {
    Meaning::Skipped
}

#[derive(Clone, Deserialize)]
struct StatusStep {
    #[serde(default)]
    delay_ms: u64,
    #[serde(default = "one")]
    repeat: u32,
    #[serde(default)]
    patch: Value,
}

fn one() -> u32 {
    1
}

#[derive(Clone, Deserialize)]
struct FixtureFile {
    #[serde(default = "ready")]
    state: String,
    #[serde(default)]
    status: Status,
    #[serde(default)]
    model_status: ModelStatus,
    #[serde(default)]
    searches: Vec<CannedSearch>,
    #[serde(default)]
    segments: BTreeMap<String, String>,
    #[serde(default)]
    status_script: Vec<StatusStep>,
}

fn ready() -> String {
    "ready".into()
}

struct MockState {
    fixture: FixtureFile,
    latency: Duration,
    config: Mutex<Config>,
    status: Mutex<Status>,
}

enum Queued {
    Line(String),
    Status(Value),
}

struct Outbox {
    q: Mutex<VecDeque<Queued>>,
    cv: Condvar,
    dead: AtomicBool,
}

impl Outbox {
    fn new() -> Self {
        Self {
            q: Mutex::new(VecDeque::new()),
            cv: Condvar::new(),
            dead: AtomicBool::new(false),
        }
    }

    fn push_line(&self, line: String) {
        let mut q = self.q.lock().expect("outbox");
        q.push_back(Queued::Line(line));
        self.cv.notify_one();
    }

    fn push_status(&self, status: Value) {
        let mut q = self.q.lock().expect("outbox");
        if let Some(existing) = q.iter_mut().find(|item| matches!(item, Queued::Status(_))) {
            *existing = Queued::Status(status);
        } else if q.len() < QUEUE_CAP {
            q.push_back(Queued::Status(status));
        }
        self.cv.notify_one();
    }

    fn pop(&self) -> Option<Queued> {
        let mut q = self.q.lock().expect("outbox");
        loop {
            if self.dead.load(Ordering::Relaxed) && q.is_empty() {
                return None;
            }
            if let Some(item) = q.pop_front() {
                return Some(item);
            }
            q = self.cv.wait(q).expect("outbox wait");
        }
    }

    fn close(&self) {
        self.dead.store(true, Ordering::Relaxed);
        self.cv.notify_all();
    }
}

struct Conn {
    helloed: bool,
    out: Arc<Outbox>,
    inflight: Arc<Mutex<Option<u64>>>,
    epoch: Arc<AtomicU64>,
    last_status: Mutex<Option<Instant>>,
}

pub fn run(opts: MockOpts) -> io::Result<()> {
    let fixture = load_fixture(opts.fixture.as_deref())?;
    let socket = opts.socket.unwrap_or_else(default_socket);
    if let Some(parent) = socket.parent() {
        // `$XDG_RUNTIME_DIR/trowel` is 0700. Do not chmod `/tmp` (test sockets live there).
        if !parent.exists() {
            fs::create_dir_all(parent)?;
            let mut perms = fs::metadata(parent)?.permissions();
            perms.set_mode(0o700);
            fs::set_permissions(parent, perms)?;
        }
    }
    let _ = fs::remove_file(&socket);
    let listener = UnixListener::bind(&socket)?;
    let mut sock_perms = fs::metadata(&socket)?.permissions();
    sock_perms.set_mode(0o600);
    fs::set_permissions(&socket, sock_perms)?;

    let state = Arc::new(MockState {
        status: Mutex::new(fixture.status.clone()),
        config: Mutex::new(Config::default()),
        fixture,
        latency: Duration::from_millis(opts.latency_ms),
    });

    eprintln!("troweld --mock listening on {}", socket.display());
    for incoming in listener.incoming() {
        let stream = incoming?;
        let state = Arc::clone(&state);
        thread::spawn(move || {
            if let Err(err) = handle_conn(stream, state) {
                if err.kind() != io::ErrorKind::BrokenPipe {
                    eprintln!("troweld --mock connection: {err}");
                }
            }
        });
    }
    Ok(())
}

fn default_socket() -> PathBuf {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    runtime.join("trowel/troweld.sock")
}

fn load_fixture(path: Option<&Path>) -> io::Result<FixtureFile> {
    let raw = match path {
        Some(p) => fs::read_to_string(p)?,
        None => DEFAULT_FIXTURE.to_string(),
    };
    serde_json::from_str(&raw).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

fn handle_conn(stream: UnixStream, state: Arc<MockState>) -> io::Result<()> {
    let writer = stream.try_clone()?;
    let out = Arc::new(Outbox::new());
    let out_w = Arc::clone(&out);
    let write_thread = thread::spawn(move || writer_loop(writer, out_w));

    let mut conn = Conn {
        helloed: false,
        out: Arc::clone(&out),
        inflight: Arc::new(Mutex::new(None)),
        epoch: Arc::new(AtomicU64::new(0)),
        last_status: Mutex::new(None),
    };
    let mut reader = BufReader::new(stream);
    loop {
        match read_line(&mut reader) {
            Line::Eof => break,
            Line::TooLarge => {
                conn.out.push_line(r#"{"error":"too_large"}"#.into());
                break;
            }
            Line::Err(err) => return Err(err),
            Line::Ok(buf) => dispatch_line(&buf, &mut conn, &state)?,
        }
    }
    out.close();
    let _ = write_thread.join();
    Ok(())
}

enum Line {
    Ok(Vec<u8>),
    Eof,
    TooLarge,
    Err(io::Error),
}

fn read_line(reader: &mut BufReader<UnixStream>) -> Line {
    let mut buf = Vec::new();
    loop {
        let mut byte = [0u8; 1];
        match reader.read_exact(&mut byte) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => {
                return if buf.is_empty() {
                    Line::Eof
                } else {
                    Line::Ok(buf)
                };
            }
            Err(err) => return Line::Err(err),
        }
        if byte[0] == b'\n' {
            return Line::Ok(buf);
        }
        buf.push(byte[0]);
        if buf.len() > MAX_LINE_BYTES {
            return Line::TooLarge;
        }
    }
}

fn dispatch_line(buf: &[u8], conn: &mut Conn, state: &Arc<MockState>) -> io::Result<()> {
    let text = match std::str::from_utf8(buf) {
        Ok(s) => s.trim(),
        Err(_) => {
            reply_error(conn, None, ErrorCode::BadRequest, "line is not UTF-8");
            return Ok(());
        }
    };
    if text.is_empty() {
        return Ok(());
    }
    let value: Value = match serde_json::from_str(text) {
        Ok(v) => v,
        Err(err) => {
            reply_error(conn, None, ErrorCode::BadRequest, err.to_string());
            return Ok(());
        }
    };
    let id = value.get("id").and_then(Value::as_u64);
    if id == Some(0) {
        reply_error(
            conn,
            id,
            ErrorCode::BadRequest,
            "id must be a positive integer",
        );
        return Ok(());
    }
    let Some(op_name) = value.get("op").and_then(Value::as_str) else {
        reply_error(conn, id, ErrorCode::BadRequest, "missing op");
        return Ok(());
    };
    if !KNOWN_OPS.contains(&op_name) {
        reply_error(
            conn,
            id,
            ErrorCode::UnknownOp,
            format!("unknown op {op_name}"),
        );
        return Ok(());
    }
    let req: Request = match serde_json::from_value(value) {
        Ok(r) => r,
        Err(err) => {
            reply_error(conn, id, ErrorCode::BadRequest, err.to_string());
            return Ok(());
        }
    };
    handle_op(req, conn, state);
    Ok(())
}

fn handle_op(req: Request, conn: &mut Conn, state: &Arc<MockState>) {
    let id = req.id;
    if !matches!(req.op, Op::Hello { .. }) && !conn.helloed {
        reply_error(
            conn,
            Some(id),
            ErrorCode::BadRequest,
            "hello must be the first request",
        );
        return;
    }
    match req.op {
        Op::Hello { protocol, .. } => {
            if protocol != PROTOCOL_VERSION {
                reply_error(
                    conn,
                    Some(id),
                    ErrorCode::BadRequest,
                    "Trowel was updated, restart it",
                );
                return;
            }
            conn.helloed = true;
            reply_ok(
                conn,
                id,
                json!({
                    "protocol": PROTOCOL_VERSION,
                    "daemon": DAEMON_VERSION,
                    "capabilities": CAPABILITIES,
                    "state": state.fixture.state,
                }),
            );
        }
        Op::PanelOpened | Op::Pause | Op::Resume => reply_ok(conn, id, json!({})),
        Op::Typing { .. } => reply_ok(conn, id, json!({})),
        Op::Search {
            text,
            mode: _,
            code: _,
            kinds: _,
            limit: _,
            prefix_last: _,
            gen,
        } => start_search(id, gen, text, conn, state),
        Op::Cancel { target_id } => {
            let cancelled = {
                let mut slot = conn.inflight.lock().expect("inflight");
                if *slot == Some(target_id) {
                    *slot = None;
                    conn.epoch.fetch_add(1, Ordering::SeqCst);
                    true
                } else {
                    false
                }
            };
            if cancelled {
                reply_error(conn, Some(target_id), ErrorCode::Cancelled, "cancelled");
            }
            reply_ok(conn, id, json!({}));
        }
        Op::Thumbnail { .. } => reply_ok(conn, id, json!({"file": "/tmp/trowel-mock/thumb.png"})),
        Op::Preview { .. } => reply_ok(conn, id, json!({"type": "text", "text": "mock preview"})),
        Op::TextOfSegment { segment_id, .. } => {
            match state.fixture.segments.get(&segment_id.to_string()) {
                Some(text) => reply_ok(conn, id, json!({"text": text})),
                None => reply_error(conn, Some(id), ErrorCode::NotFound, "unknown segment"),
            }
        }
        Op::Open { .. } | Op::Reveal { .. } | Op::QuickLook { .. } | Op::Sync { .. } => {
            reply_ok(conn, id, json!({}))
        }
        Op::CopyPath { path } => reply_ok(conn, id, json!({"text": path.display()})),
        Op::Subscribe { topics } => {
            reply_ok(conn, id, json!({}));
            for topic in topics {
                match topic.as_str() {
                    "status" => emit_status(conn, state, true),
                    "folders" => {
                        let roots = state.config.lock().expect("config").roots.clone();
                        conn.out
                            .push_line(json!({"event": "folders", "roots": roots}).to_string());
                    }
                    "model" => {
                        conn.out.push_line(
                            json!({
                                "event": "model",
                                "present": state.fixture.model_status.present,
                                "verified": state.fixture.model_status.verified,
                                "bytes": state.fixture.model_status.bytes,
                                "total": state.fixture.model_status.total,
                                "downloading": state.fixture.model_status.downloading,
                            })
                            .to_string(),
                        );
                    }
                    _ => {}
                }
            }
            if !state.fixture.status_script.is_empty() {
                let script = state.fixture.status_script.clone();
                let state = Arc::clone(state);
                let out = Arc::clone(&conn.out);
                thread::spawn(move || run_status_script(script, state, out));
            }
        }
        Op::Status => {
            let snap = state.status.lock().expect("status").clone();
            reply_ok(conn, id, serde_json::to_value(snap).unwrap_or(json!({})));
        }
        Op::ConfigGet => {
            let cfg = state.config.lock().expect("config").clone();
            reply_ok(conn, id, serde_json::to_value(cfg).unwrap_or(json!({})));
        }
        Op::ConfigSet { patch } => {
            let mut cfg = state.config.lock().expect("config");
            merge_config(&mut cfg, &patch);
            reply_ok(conn, id, serde_json::to_value(&*cfg).unwrap_or(json!({})));
        }
        Op::Estimate { paths, .. } => {
            let mut per_path = Vec::new();
            for path in paths {
                let row = json!({
                    "path": path,
                    "files": 0,
                    "by_kind": {},
                    "est_seconds": 0,
                    "est_bytes": 0,
                    "skips": [],
                });
                conn.out
                    .push_line(json!({"event": "estimate", "per_path": [row.clone()]}).to_string());
                per_path.push(row);
            }
            reply_ok(conn, id, json!({"per_path": per_path}));
        }
        Op::ModelStatus => {
            reply_ok(
                conn,
                id,
                serde_json::to_value(&state.fixture.model_status).unwrap_or(json!({})),
            );
        }
        Op::ModelDownload { .. } => reply_ok(conn, id, json!({})),
        Op::Doctor => reply_ok(conn, id, json!({"ok": true, "checks": []})),
        Op::SetHotkey { .. } => reply_ok(conn, id, json!({})),
    }
}

fn start_search(id: u64, gen: u64, text: String, conn: &Conn, state: &Arc<MockState>) {
    {
        let mut slot = conn.inflight.lock().expect("inflight");
        if let Some(old) = slot.take() {
            reply_error(conn, Some(old), ErrorCode::Superseded, "superseded");
        }
        *slot = Some(id);
    }
    let epoch = conn.epoch.fetch_add(1, Ordering::SeqCst) + 1;
    let result = canned_search(&state.fixture, &text, gen, state.latency);
    let latency = state.latency;
    let out = Arc::clone(&conn.out);
    let inflight = Arc::clone(&conn.inflight);
    let conn_epoch = Arc::clone(&conn.epoch);
    thread::spawn(move || {
        if !latency.is_zero() {
            thread::sleep(latency);
        }
        if conn_epoch.load(Ordering::SeqCst) != epoch {
            return;
        }
        let mut slot = inflight.lock().expect("inflight");
        if *slot != Some(id) {
            return;
        }
        *slot = None;
        drop(slot);
        let mut body = serde_json::to_value(&result).unwrap_or(json!({}));
        if let Value::Object(map) = &mut body {
            map.insert("id".into(), json!(id));
            map.insert("ok".into(), json!(true));
        }
        out.push_line(body.to_string());
    });
}

fn canned_search(fixture: &FixtureFile, text: &str, gen: u64, latency: Duration) -> SearchResult {
    let canned = fixture
        .searches
        .iter()
        .find(|c| !c.needle.is_empty() && text.contains(&c.needle));
    SearchResult {
        gen,
        groups: canned.map(|c| c.groups.clone()).unwrap_or_default(),
        weaker: canned.map(|c| c.weaker.clone()).unwrap_or_default(),
        took_ms: latency.as_millis() as u64,
        meaning: canned.map(|c| c.meaning).unwrap_or(Meaning::Skipped),
        counts: canned
            .map(|c| {
                c.groups
                    .iter()
                    .map(|g| (g.kind.clone(), g.rows.len() as u64))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

fn emit_status(conn: &Conn, state: &MockState, force: bool) {
    let now = Instant::now();
    if !force {
        let mut last = conn.last_status.lock().expect("last_status");
        if let Some(prev) = *last {
            if now.duration_since(prev) < Duration::from_millis(250) {
                let snap = state.status.lock().expect("status").clone();
                let mut v = serde_json::to_value(snap).unwrap_or(json!({}));
                if let Value::Object(map) = &mut v {
                    map.insert("event".into(), json!("status"));
                }
                conn.out.push_status(v);
                return;
            }
        }
        *last = Some(now);
    }
    let snap = state.status.lock().expect("status").clone();
    let mut v = serde_json::to_value(snap).unwrap_or(json!({}));
    if let Value::Object(map) = &mut v {
        map.insert("event".into(), json!("status"));
    }
    conn.out.push_status(v);
}

fn run_status_script(script: Vec<StatusStep>, state: Arc<MockState>, out: Arc<Outbox>) {
    for step in script {
        for _ in 0..step.repeat.max(1) {
            if !Duration::from_millis(step.delay_ms).is_zero() {
                thread::sleep(Duration::from_millis(step.delay_ms));
            }
            if out.dead.load(Ordering::Relaxed) {
                return;
            }
            {
                let mut status = state.status.lock().expect("status");
                apply_patch(&mut status, &step.patch);
                let mut v = serde_json::to_value(&*status).unwrap_or(json!({}));
                if let Value::Object(map) = &mut v {
                    map.insert("event".into(), json!("status"));
                }
                out.push_status(v);
            }
        }
    }
}

fn apply_patch(status: &mut Status, patch: &Value) {
    let Ok(mut base) = serde_json::to_value(&*status) else {
        return;
    };
    merge_objects(&mut base, patch);
    if let Ok(next) = serde_json::from_value(base) {
        *status = next;
    }
}

fn merge_objects(base: &mut Value, patch: &Value) {
    let (Value::Object(dst), Value::Object(src)) = (base, patch) else {
        return;
    };
    for (k, v) in src {
        if matches!(k.as_str(), "main" | "code" | "counts") && v.is_object() {
            merge_objects(dst.entry(k.clone()).or_insert_with(|| json!({})), v);
        } else {
            dst.insert(k.clone(), v.clone());
        }
    }
}

fn merge_config(cfg: &mut Config, patch: &Value) {
    let Ok(mut base) = serde_json::to_value(&*cfg) else {
        return;
    };
    merge_objects(&mut base, patch);
    if let Ok(next) = serde_json::from_value(base) {
        *cfg = next;
    }
}

fn reply_ok(conn: &Conn, id: u64, body: Value) {
    let mut out = body;
    match &mut out {
        Value::Object(map) => {
            map.insert("id".into(), json!(id));
            map.insert("ok".into(), json!(true));
        }
        other => {
            out = json!({"id": id, "ok": true, "value": other});
        }
    }
    conn.out.push_line(out.to_string());
}

fn reply_error(conn: &Conn, id: Option<u64>, code: ErrorCode, message: impl Into<String>) {
    let err = ErrorBody::new(code, message);
    let mut body = json!({"ok": false, "error": err});
    if let (Some(id), Value::Object(map)) = (id, &mut body) {
        map.insert("id".into(), json!(id));
    }
    conn.out.push_line(body.to_string());
}

fn writer_loop(mut stream: UnixStream, out: Arc<Outbox>) {
    while let Some(item) = out.pop() {
        let line = match item {
            Queued::Line(s) => s,
            Queued::Status(v) => v.to_string(),
        };
        if writeln!(stream, "{line}").is_err() {
            break;
        }
    }
}
