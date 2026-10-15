//! Real PROTOCOL daemon. Owns the Searcher; watches folders; speaks the socket.

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::io::{self, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::{AsRawFd, FromRawFd};
use std::os::unix::net::{UnixDatagram, UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use trowel_core::{Embedder, FakeEmbedder, FileKind, IndexStore, Prompts, Rules, Searcher};
use trowel_proto::ops::KNOWN_OPS;
use trowel_proto::{
    CalcReply, Config, ErrorBody, ErrorCode, Meaning, Op, PasteMode, Request, SearchMode, Status,
    MAX_LINE_BYTES, PROTOCOL_VERSION,
};

use crate::actions;
use crate::cache::QueryCache;
use crate::calc::Calculator;
use crate::config_file::{self, expand_tilde};
use crate::disk;
use crate::doctor;
use crate::encoder::QueryEncoder;
use crate::fs_kind::{self, RootKind};
use crate::helper::{self, Helper, HelperOpts};
use crate::hotkey;
use crate::lock::InstanceLock;
use crate::logind;
use crate::power;
use crate::present;
use crate::previews;
use crate::state::{self, StateDb};
use crate::watcher::{FolderWatch, CODE_LONGEST_WAIT, CODE_QUIET, DEBOUNCE};

const QUEUE_CAP: usize = 256;
const DAEMON_VERSION: &str = env!("CARGO_PKG_VERSION");
const CAPABILITIES: &[&str] = &["code", "audio", "video", "ocr"];
const SEARCHER_REFRESH: Duration = Duration::from_secs(2);

pub struct EngineOpts {
    pub socket: Option<PathBuf>,
    pub index: Option<PathBuf>,
    pub config: Option<PathBuf>,
    pub fake: bool,
    /// Where `state.sqlite` lives. Default `$XDG_DATA_HOME/trowel`.
    pub state_dir: Option<PathBuf>,
}

struct Engine {
    config_path: PathBuf,
    index_dir: PathBuf,
    socket: PathBuf,
    fake: bool,
    config: Mutex<Config>,
    status: Mutex<Status>,
    searcher: Mutex<Searcher>,
    /// Separate `code.sqlite` searcher. CLI `cmd_search` already opens
    /// `IndexStore::CODE_FILE` for `code:` queries; the daemon must too.
    /// `upstream/Sources/DigUpKit/Searcher.swift` `isCode`.
    code_searcher: Mutex<Option<Searcher>>,
    last_reload: Mutex<Instant>,
    paused: AtomicBool,
    sync_wanted: AtomicBool,
    helper: Mutex<Option<Arc<Helper>>>,
    helper_starting: AtomicBool,
    subscribers: Mutex<Vec<Arc<Outbox>>>,
    query_cache: QueryCache,
    preview_cache: Mutex<PreviewCache>,
    newest_typed: Mutex<Option<String>>,
    last_use: Mutex<Instant>,
    watch: Mutex<Option<FolderWatch>>,
    code_watch: Mutex<Option<FolderWatch>>,
    config_watch: Mutex<Option<FolderWatch>>,
    encoder: Mutex<Option<QueryEncoder>>,
    overflow: Arc<AtomicBool>,
    sleeping: Arc<AtomicBool>,
    was_sleeping: AtomicBool,
    was_on_battery: AtomicBool,
    calc: Calculator,
    state_dir: PathBuf,
    /// Opened on first use, so a daemon that never launches or copies never creates the file.
    state: Mutex<Option<StateDb>>,
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
    /// Bumped by each `preview`; a queued render that is no longer the newest is skipped.
    preview_epoch: Arc<AtomicU64>,
    last_status: Mutex<Option<Instant>>,
}

pub fn run(opts: EngineOpts) -> io::Result<()> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let dir = runtime.join("trowel");
    let lock_path = dir.join("troweld.lock");
    let Some(_lock) = InstanceLock::acquire(&lock_path)? else {
        eprintln!("troweld already running");
        return Ok(());
    };
    let config_path = opts.config.unwrap_or_else(config_file::default_config_path);
    let cfg = config_file::load_or_create(&config_path)?;
    let index_dir = opts.index.unwrap_or_else(|| expand_tilde(&cfg.index_dir));
    fs::create_dir_all(&index_dir)?;
    let store = IndexStore::open(&index_dir).map_err(io_err)?;
    let searcher = Searcher::new(store, true).map_err(io_err)?;
    let socket = opts.socket.unwrap_or_else(|| dir.join("troweld.sock"));
    let listener = bind_listener(&socket)?;

    let status = Status {
        vectors_loaded: searcher.vectors_loaded,
        worker: if opts.fake { "ready" } else { "down" }.into(),
        ..Default::default()
    };
    let engine = Arc::new(Engine {
        config_path: config_path.clone(),
        index_dir: index_dir.clone(),
        socket: socket.clone(),
        fake: opts.fake,
        config: Mutex::new(cfg.clone()),
        status: Mutex::new(status),
        searcher: Mutex::new(searcher),
        code_searcher: Mutex::new(open_code_searcher(&index_dir)),
        last_reload: Mutex::new(Instant::now()),
        paused: AtomicBool::new(false),
        sync_wanted: AtomicBool::new(false),
        helper: Mutex::new(None),
        helper_starting: AtomicBool::new(false),
        subscribers: Mutex::new(Vec::new()),
        query_cache: QueryCache::default(),
        preview_cache: Mutex::new(PreviewCache::default()),
        newest_typed: Mutex::new(None),
        last_use: Mutex::new(Instant::now()),
        watch: Mutex::new(None),
        code_watch: Mutex::new(None),
        config_watch: Mutex::new(None),
        encoder: Mutex::new(None),
        overflow: Arc::new(AtomicBool::new(false)),
        sleeping: Arc::new(AtomicBool::new(false)),
        was_sleeping: AtomicBool::new(false),
        was_on_battery: AtomicBool::new(power::on_battery()),
        calc: Calculator::default(),
        state_dir: opts.state_dir.unwrap_or_else(state::default_state_dir),
        state: Mutex::new(None),
    });

    logind::bind_signals(&engine.overflow, &engine.sleeping);
    let sleep_eng = Arc::clone(&engine);
    logind::start(move |going| {
        sleep_eng.sleeping.store(going, Ordering::SeqCst);
    });
    start_config_watch(&engine);
    if cfg.onboarded {
        start_watch(&engine);
        engine.request_sync();
    }
    spawn_idle_reaper(Arc::clone(&engine));
    spawn_watchdog(Arc::clone(&engine));

    sd_notify("READY=1");
    eprintln!("troweld listening on {}", socket.display());
    for incoming in listener.incoming() {
        let stream = incoming?;
        if let Err(err) = reject_foreign_uid(&stream) {
            eprintln!("troweld drop peer: {err}");
            continue;
        }
        let engine = Arc::clone(&engine);
        thread::spawn(move || {
            if let Err(err) = handle_conn(stream, engine) {
                if err.kind() != io::ErrorKind::BrokenPipe {
                    eprintln!("troweld connection: {err}");
                }
            }
        });
    }
    Ok(())
}

fn io_err(err: impl std::fmt::Display) -> io::Error {
    io::Error::other(err.to_string())
}

fn reject_foreign_uid(stream: &UnixStream) -> io::Result<()> {
    let mut cred = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    if rc != 0 {
        return Err(io::Error::last_os_error());
    }
    let me = unsafe { libc::getuid() };
    if cred.uid != me {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "peer uid mismatch",
        ));
    }
    Ok(())
}

/// systemd socket activation (`LISTEN_FDS`) or bind `$XDG_RUNTIME_DIR/trowel/troweld.sock`.
fn bind_listener(socket: &Path) -> io::Result<UnixListener> {
    if let Some(listener) = take_systemd_listener() {
        return Ok(listener);
    }
    if let Some(parent) = socket.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)?;
            let mut perms = fs::metadata(parent)?.permissions();
            perms.set_mode(0o700);
            fs::set_permissions(parent, perms)?;
        }
    }
    let _ = fs::remove_file(socket);
    let listener = UnixListener::bind(socket)?;
    let mut sock_perms = fs::metadata(socket)?.permissions();
    sock_perms.set_mode(0o600);
    fs::set_permissions(socket, sock_perms)?;
    Ok(listener)
}

fn take_systemd_listener() -> Option<UnixListener> {
    let fds: u32 = std::env::var("LISTEN_FDS").ok()?.parse().ok()?;
    if fds == 0 {
        return None;
    }
    if let Ok(pid) = std::env::var("LISTEN_PID") {
        let pid: u32 = pid.parse().ok()?;
        if pid != std::process::id() {
            return None;
        }
    }
    // SD_LISTEN_FDS_START == 3
    let fd = 3i32;
    unsafe { Some(UnixListener::from_raw_fd(fd)) }
}

fn sd_notify(msg: &str) {
    // systemd's notify socket is SOCK_DGRAM (often `@abstract` or a path).
    // A stream connect silently fails, so Type=notify never sees READY and
    // kills troweld after TimeoutStartSec.
    let Ok(raw) = std::env::var("NOTIFY_SOCKET") else {
        return;
    };
    if raw.is_empty() {
        return;
    }
    let Ok(sock) = UnixDatagram::unbound() else {
        return;
    };
    let _ = sock.set_nonblocking(true);
    if let Some(name) = raw.strip_prefix('@') {
        let mut addr = libc::sockaddr_un {
            sun_family: libc::AF_UNIX as libc::sa_family_t,
            sun_path: [0; 108],
        };
        let bytes = name.as_bytes();
        if bytes.len() + 1 >= addr.sun_path.len() {
            return;
        }
        for (i, b) in bytes.iter().enumerate() {
            addr.sun_path[i + 1] = *b as libc::c_char;
        }
        let len = std::mem::offset_of!(libc::sockaddr_un, sun_path) + 1 + bytes.len();
        unsafe {
            libc::sendto(
                sock.as_raw_fd(),
                msg.as_ptr().cast(),
                msg.len(),
                0,
                std::ptr::from_ref(&addr).cast(),
                len as libc::socklen_t,
            );
        }
        return;
    }
    let _ = sock.send_to(msg.as_bytes(), Path::new(&raw));
}

fn handle_conn(stream: UnixStream, engine: Arc<Engine>) -> io::Result<()> {
    let writer = stream.try_clone()?;
    let out = Arc::new(Outbox::new());
    let out_w = Arc::clone(&out);
    let write_thread = thread::spawn(move || writer_loop(writer, out_w));
    let mut conn = Conn {
        helloed: false,
        out: Arc::clone(&out),
        inflight: Arc::new(Mutex::new(None)),
        epoch: Arc::new(AtomicU64::new(0)),
        preview_epoch: Arc::new(AtomicU64::new(0)),
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
            Line::Ok(buf) => dispatch_line(&buf, &mut conn, &engine)?,
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

fn dispatch_line(buf: &[u8], conn: &mut Conn, engine: &Arc<Engine>) -> io::Result<()> {
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
    handle_op(req, conn, engine);
    Ok(())
}

fn handle_op(req: Request, conn: &mut Conn, engine: &Arc<Engine>) {
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
            let cfg = engine.config.lock().expect("config");
            let state = if !cfg.onboarded {
                "onboarding"
            } else if trowel_core::ModelFiles::complete() || engine.fake {
                "ready"
            } else {
                "model_missing"
            };
            reply_ok(
                conn,
                id,
                json!({
                    "protocol": PROTOCOL_VERSION,
                    "daemon": DAEMON_VERSION,
                    "capabilities": CAPABILITIES,
                    "state": state,
                }),
            );
        }
        Op::PanelOpened => {
            engine.touch();
            // Reply at once: the reload takes the searcher lock (60–80 ms with vectors), and the
            // panel's first `typing`/`search` lines are queued right behind this one.
            reply_ok(conn, id, json!({}));
            prewarm(engine);
        }
        Op::Pause => {
            engine.paused.store(true, Ordering::SeqCst);
            if let Some(helper) = engine.helper.lock().expect("helper").as_ref() {
                helper.stop();
            }
            {
                let mut status = engine.status.lock().expect("status");
                status.paused_reason = Some("user".into());
                status.phase = "paused".into();
            }
            engine.broadcast_status();
            reply_ok(conn, id, json!({}));
        }
        Op::Resume => {
            engine.paused.store(false, Ordering::SeqCst);
            {
                let mut status = engine.status.lock().expect("status");
                status.paused_reason = None;
                status.phase = "idle".into();
            }
            engine.broadcast_status();
            engine.start_helper();
            reply_ok(conn, id, json!({}));
        }
        Op::Typing { text, code } => {
            reply_ok(conn, id, json!({}));
            engine.embed_ahead(text, code);
        }
        Op::Search {
            text,
            mode,
            code,
            kinds,
            limit,
            prefix_last,
            gen,
        } => start_search(
            id,
            gen,
            text,
            mode,
            code,
            kinds,
            limit,
            prefix_last,
            conn,
            engine,
        ),
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
        Op::Status => {
            let snap = engine.status.lock().expect("status").clone();
            reply_ok(conn, id, serde_json::to_value(snap).unwrap_or(json!({})));
        }
        Op::Subscribe { topics } => {
            reply_ok(conn, id, json!({}));
            if topics.iter().any(|t| t == "status") {
                engine
                    .subscribers
                    .lock()
                    .expect("subscribers")
                    .push(Arc::clone(&conn.out));
                emit_status(conn, engine, true);
            }
        }
        Op::ConfigGet => {
            let cfg = engine.config.lock().expect("config").clone();
            reply_ok(conn, id, serde_json::to_value(cfg).unwrap_or(json!({})));
        }
        Op::ConfigSet { patch } => {
            let applied = {
                let mut cfg = engine.config.lock().expect("config");
                match config_file::apply_patch(&cfg, &patch) {
                    Ok(next) => {
                        if let Err(err) = config_file::save(&engine.config_path, &next) {
                            engine.status.lock().expect("status").last_error =
                                Some(format!("config write: {err}"));
                            reply_error(conn, Some(id), ErrorCode::Io, err.to_string());
                            return;
                        }
                        let prev = cfg.clone();
                        *cfg = next.clone();
                        Some((prev, next))
                    }
                    Err(err) => {
                        engine.status.lock().expect("status").last_error =
                            Some(format!("invalid config: {err}"));
                        reply_error(conn, Some(id), ErrorCode::BadRequest, err);
                        return;
                    }
                }
            };
            if let Some((prev, next)) = applied {
                reply_ok(conn, id, serde_json::to_value(&next).unwrap_or(json!({})));
                let folders_changed = next.roots != prev.roots
                    || next.code_roots != prev.code_roots
                    || next.onboarded != prev.onboarded;
                if next.onboarded && folders_changed {
                    if let Some(helper) = engine.helper.lock().expect("helper").clone() {
                        helper.stop();
                    }
                    start_watch(engine);
                    engine.request_sync();
                }
            }
        }
        Op::Sync { code } => {
            if code {
                engine.request_code_sync();
            } else {
                engine.request_sync();
            }
            reply_ok(conn, id, json!({}));
        }
        Op::Doctor => {
            let report = doctor::run(Some(&engine.index_dir), Some(&engine.socket));
            reply_ok(conn, id, doctor::json_report(&report));
        }
        Op::CopyPath { path } => {
            let dest = PathBuf::from(path.display());
            thread::spawn(move || actions::copy(&dest));
            reply_ok(conn, id, json!({"text": path.display()}));
        }
        // Renders (vips, pdftoppm, ffmpeg: 80–500 ms) run off the connection thread, so a
        // keystroke's `search` never waits behind the previews of the rows it is arrowing over.
        Op::Thumbnail {
            segment_id,
            path,
            size,
            code,
        } => {
            let file = path.as_ref().map(|p| PathBuf::from(p.display()));
            let prefer_code = prefer_code_store(engine, code);
            let (out, eng) = (Arc::clone(&conn.out), Arc::clone(engine));
            render_pool().run(move || {
                let body = thumbnail_body(&eng, segment_id, file.as_deref(), size, prefer_code);
                reply_preview_line(&out, id, body);
            });
        }
        Op::Preview {
            segment_id,
            width,
            code,
        } => {
            let query = engine
                .newest_typed
                .lock()
                .expect("typed")
                .clone()
                .and_then(|k| k.split('\u{0}').next_back().map(str::to_string));
            let prefer_code = prefer_code_store(engine, code);
            // Only the newest preview of a connection is worth rendering: arrowing down a list
            // asks for one per row.
            let ticket = conn.preview_epoch.fetch_add(1, Ordering::SeqCst) + 1;
            let epoch = Arc::clone(&conn.preview_epoch);
            let (out, eng) = (Arc::clone(&conn.out), Arc::clone(engine));
            render_pool().run(move || {
                if epoch.load(Ordering::SeqCst) != ticket {
                    reply_error_line(&out, id, ErrorCode::Superseded, "superseded");
                    return;
                }
                let body = cached_preview(&eng, segment_id, width, query.as_deref(), prefer_code);
                reply_preview_line(&out, id, body);
            });
        }
        Op::TextOfSegment { segment_id, code } => {
            let prefer_code = prefer_code_store(engine, code);
            let (out, eng) = (Arc::clone(&conn.out), Arc::clone(engine));
            render_pool().run(move || {
                reply_preview_line(&out, id, text_of_body(&eng, segment_id, prefer_code));
            });
        }
        Op::Open {
            segment_id,
            path,
            code,
            loc,
        } => {
            let prefer_code = prefer_code_store(engine, code);
            let meta = segment_id.and_then(|sid| segment_meta_any(engine, sid, prefer_code));
            let dest = meta
                .as_ref()
                .map(|m| m.path.clone())
                .or_else(|| path.as_ref().map(|p| PathBuf::from(p.display())));
            let mut kind = meta.as_ref().map(|m| m.file_kind).unwrap_or(FileKind::Doc);
            if prefer_code {
                kind = FileKind::Code;
            }
            let loc = loc.or_else(|| meta.as_ref().and_then(|m| m.loc));
            let editor = engine.config.lock().expect("config").code_editor.clone();
            if let Some(dest) = dest {
                thread::spawn(move || actions::open(&dest, kind, loc, &editor));
            }
            reply_ok(conn, id, json!({}));
        }
        Op::Reveal { path } => {
            let dest = PathBuf::from(path.display());
            thread::spawn(move || actions::reveal(&dest));
            reply_ok(conn, id, json!({}));
        }
        Op::QuickLook { .. } => reply_ok(conn, id, json!({})),
        Op::Estimate { paths, code } => {
            let out = Arc::clone(&conn.out);
            thread::spawn(move || {
                let rows: Vec<Value> = paths
                    .iter()
                    .map(|p| estimate_path(PathBuf::from(p.display()), code))
                    .collect();
                out.push_line(json!({"event": "estimate", "per_path": rows.clone()}).to_string());
                let mut body = json!({"per_path": rows});
                if let Value::Object(map) = &mut body {
                    map.insert("id".into(), json!(id));
                    map.insert("ok".into(), json!(true));
                }
                out.push_line(body.to_string());
            });
        }
        Op::ModelStatus => reply_ok(conn, id, model_status_json()),
        Op::ModelDownload { .. } => reply_ok(conn, id, model_status_json()),
        Op::SetHotkey { chord } => match hotkey::write_chord(&chord) {
            Ok(conflict) => {
                engine.config.lock().expect("config").hotkey = chord.clone();
                let _ =
                    config_file::save(&engine.config_path, &engine.config.lock().expect("config"));
                reply_ok(conn, id, json!({"conflict": conflict}));
            }
            Err(err) => reply_error(conn, Some(id), ErrorCode::Io, err.to_string()),
        },
        Op::Calc { text, force, gen } => {
            let out = engine.calc.eval(&text, force);
            reply_ok(
                conn,
                id,
                json!(CalcReply {
                    gen,
                    result: out.result,
                    error: out.error,
                    took_us: Some(out.took_us),
                }),
            );
        }
        Op::CalcHistory { limit } => match engine.with_state(|s| s.calc_history(limit)) {
            Ok(rows) => reply_ok(conn, id, json!({ "rows": rows })),
            Err(err) => reply_error(conn, Some(id), ErrorCode::Io, err),
        },
        Op::CalcCopy { text, expr, mode } => {
            // The UI has already hidden the panel (D21), so the keystroke reaches the app below.
            if let Err(err) = actions::copy_text(&text) {
                reply_error(conn, Some(id), ErrorCode::Io, err.to_string());
                return;
            }
            // `plain` is a paste too: a result is plain text already.
            if mode != PasteMode::Copy {
                actions::paste_keystroke();
            }
            if let Some(expr) = expr {
                let at = unix_now();
                if let Err(err) = engine.with_state(|s| s.record_calc(&expr, &text, at)) {
                    eprintln!("troweld: calc history: {err}");
                }
            }
            reply_ok(conn, id, json!({}));
        }
        // Protocol v2 ops whose engines land in L2 (apps) and L3 (clipboard).
        // `hello` does not advertise `apps`/`clipboard` until then.
        op => reply_error(
            conn,
            Some(id),
            ErrorCode::NotReady,
            format!("{} is not available in this build", op.name()),
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn start_search(
    id: u64,
    gen: u64,
    text: String,
    mode: SearchMode,
    code: bool,
    kinds: Option<Vec<String>>,
    limit: u32,
    prefix_last: bool,
    conn: &Conn,
    engine: &Arc<Engine>,
) {
    {
        let mut slot = conn.inflight.lock().expect("inflight");
        if let Some(old) = slot.take() {
            reply_error(conn, Some(old), ErrorCode::Superseded, "superseded");
        }
        *slot = Some(id);
    }
    let epoch = conn.epoch.fetch_add(1, Ordering::SeqCst) + 1;
    let out = Arc::clone(&conn.out);
    let inflight = Arc::clone(&conn.inflight);
    let conn_epoch = Arc::clone(&conn.epoch);
    let engine = Arc::clone(engine);
    thread::spawn(move || {
        if conn_epoch.load(Ordering::SeqCst) != epoch {
            return;
        }
        let started = Instant::now();
        engine.touch();
        let kind_set: Option<HashSet<FileKind>> =
            kinds.map(|list| list.into_iter().filter_map(|s| s.parse().ok()).collect());
        let mut q = text.clone();
        let mut use_code = code;
        if let Some(rest) = trowel_core::CodeQuery::text_of(&q) {
            q = rest;
            use_code = true;
        }
        // Hybrid with prefix_last=false is upstream-correct once document
        // vectors exist. With none yet (first crawl / battery backfill), that
        // path drops prefix keyword hits like "bitw"→bitwig.pdf and the UI
        // shows "No good matches". Keep words+prefix until embeddings land.
        if mode == SearchMode::Hybrid {
            // Meaning search needs the vectors; usually `panel_opened` loaded them already.
            ensure_vectors(&engine, use_code);
        }
        let has_doc_vectors = if use_code {
            ensure_code_searcher(&engine);
            engine
                .code_searcher
                .lock()
                .expect("code_searcher")
                .as_ref()
                .map(Searcher::has_document_vectors)
                .unwrap_or(false)
        } else {
            engine
                .searcher
                .lock()
                .expect("searcher")
                .has_document_vectors()
        };
        let (use_query_vector, fts_prefix) = search_plan(mode, prefix_last, has_doc_vectors);
        let vector = if use_query_vector {
            query_vector(&engine, &q, use_code)
        } else {
            None
        };
        let meaning = if vector.is_some() {
            Meaning::Used
        } else if mode == SearchMode::Keyword {
            Meaning::Skipped
        } else {
            Meaning::Unavailable
        };
        let mut tab_counts = None;
        let hits = {
            let searched = if use_code {
                ensure_code_searcher(&engine);
                let mut slot = engine.code_searcher.lock().expect("code_searcher");
                slot.as_mut().map(|searcher| {
                    searcher.search(
                        &q,
                        vector.as_deref(),
                        kind_set.as_ref(),
                        limit as usize,
                        Searcher::Z_GATE,
                        None,
                        fts_prefix,
                    )
                })
            } else {
                let mut searcher = engine.searcher.lock().expect("searcher");
                // A kind tab: the counts on the tabs come from the same query unfiltered
                // (`SearchWindow.swift:311`). The filter is applied after scoring, so the
                // filtered list ranks exactly as the All tab does.
                if kind_set.is_some() {
                    if let Ok(all) = searcher.search(
                        &q,
                        vector.as_deref(),
                        None,
                        limit as usize,
                        Searcher::Z_GATE,
                        None,
                        fts_prefix,
                    ) {
                        tab_counts = Some(present::kind_counts(&all));
                    }
                }
                Some(searcher.search(
                    &q,
                    vector.as_deref(),
                    kind_set.as_ref(),
                    limit as usize,
                    Searcher::Z_GATE,
                    None,
                    fts_prefix,
                ))
            };
            match searched {
                Some(Ok(h)) => h,
                Some(Err(err)) => {
                    reply_error_line(&out, id, ErrorCode::Internal, err.to_string());
                    return;
                }
                None => Vec::new(),
            }
        };
        let took_ms = started.elapsed().as_millis() as u64;
        let mut result = present::result_set(&hits, gen, took_ms, meaning);
        if let Some(counts) = tab_counts {
            result.counts = counts;
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

fn query_vector(engine: &Engine, text: &str, code: bool) -> Option<Vec<f32>> {
    let key = QueryCache::key(text, code);
    if let Some(cached) = engine.query_cache.get(&key) {
        return Some(cached);
    }
    let prompt = if code {
        Prompts::code_query(text)
    } else {
        Prompts::query(text)
    };
    let vector = if engine.fake {
        let mut embedder = FakeEmbedder::new();
        embedder.embed_texts(&[prompt]).ok()?.into_iter().next()
    } else {
        let source = vector_source_for(engine, code);
        if source.as_deref().is_some_and(|s| s.starts_with("fake@")) {
            let mut embedder = FakeEmbedder::new();
            embedder.embed_texts(&[prompt]).ok()?.into_iter().next()
        } else {
            let mut slot = engine.encoder.lock().expect("encoder");
            if !start_encoder(engine, &mut slot) {
                return None;
            }
            slot.as_mut()?
                .embed_texts(&[prompt])
                .ok()?
                .into_iter()
                .next()
        }
    };
    if let Some(values) = &vector {
        engine.query_cache.insert(key, values.clone());
    }
    vector
}

/// `(embed_query, fts_prefix_last)`. Hybrid without document vectors must not
/// claim meaning-search or drop the in-progress last token.
fn search_plan(mode: SearchMode, prefix_last: bool, has_doc_vectors: bool) -> (bool, bool) {
    match mode {
        SearchMode::Keyword => (false, prefix_last),
        SearchMode::Hybrid if has_doc_vectors => (true, prefix_last),
        SearchMode::Hybrid => (false, true),
    }
}

fn start_encoder(engine: &Engine, slot: &mut Option<QueryEncoder>) -> bool {
    if slot.is_some() {
        return true;
    }
    engine.status.lock().expect("status").worker = "starting".into();
    match QueryEncoder::spawn() {
        Ok(enc) => {
            engine.status.lock().expect("status").worker = "ready".into();
            *slot = Some(enc);
            true
        }
        Err(err) => {
            let mut status = engine.status.lock().expect("status");
            status.worker = "down".into();
            status.last_error = Some(err);
            false
        }
    }
}

/// Gets meaning search ready as the panel opens: starts the query encoder (~1 s cold here) and
/// loads the vectors, in parallel, so both are usually done by the time a few words are typed.
/// Vectors already loaded are not read again. `upstream/Sources/DigUpApp/Engine.swift:847` `prewarm`.
fn prewarm(engine: &Arc<Engine>) {
    let eng = Arc::clone(engine);
    thread::spawn(move || {
        ensure_vectors(&eng, false);
        ensure_vectors(&eng, true);
    });
    if engine.fake {
        return;
    }
    let eng = Arc::clone(engine);
    thread::spawn(move || {
        let fake_vectors = vector_source_for(&eng, false).is_some_and(|s| s.starts_with("fake@"));
        if !fake_vectors {
            if let Ok(mut slot) = eng.encoder.lock() {
                start_encoder(&eng, &mut slot);
            }
        }
    });
}

/// Loads the vectors of one searcher if they aren't (released when idle, or never loaded).
fn ensure_vectors(engine: &Engine, code: bool) {
    if code {
        ensure_code_searcher(engine);
        if let Ok(mut slot) = engine.code_searcher.lock() {
            if let Some(searcher) = slot.as_mut().filter(|s| !s.vectors_loaded) {
                let _ = searcher.reload();
            }
        }
    } else if let Ok(mut searcher) = engine.searcher.lock() {
        if !searcher.vectors_loaded {
            let _ = searcher.reload();
        }
        engine.status.lock().expect("status").vectors_loaded = searcher.vectors_loaded;
    }
}

/// New vectors reach search, at most every 2 s during a long pass. A searcher whose vectors
/// aren't loaded (the panel hasn't been used lately) only re-reads its files; its vectors load
/// fresh when the panel opens. Reloading them all here kept ~all vectors in memory through an
/// index pass and re-read them every 2 s. `upstream/Sources/DigUpApp/Engine.swift:673`.
fn reload_searcher(engine: &Engine, force: bool) {
    {
        let mut last = engine.last_reload.lock().expect("reload");
        if !force && last.elapsed() < SEARCHER_REFRESH {
            return;
        }
        *last = Instant::now();
    }
    let refresh = |searcher: &mut Searcher| {
        let _ = if searcher.vectors_loaded {
            searcher.reload()
        } else {
            searcher.refresh_files()
        };
    };
    if let Ok(mut searcher) = engine.searcher.lock() {
        refresh(&mut searcher);
        engine.status.lock().expect("status").vectors_loaded = searcher.vectors_loaded;
    }
    ensure_code_searcher(engine);
    if let Ok(mut slot) = engine.code_searcher.lock() {
        if let Some(searcher) = slot.as_mut() {
            refresh(searcher);
        }
    }
}

fn open_code_searcher(index_dir: &Path) -> Option<Searcher> {
    IndexStore::open_named(index_dir, IndexStore::CODE_FILE)
        .and_then(|store| Searcher::new(store, true))
        .ok()
}

fn ensure_code_searcher(engine: &Engine) {
    let mut slot = engine.code_searcher.lock().expect("code_searcher");
    if slot.is_none() {
        *slot = open_code_searcher(&engine.index_dir);
    }
}

fn vector_source_for(engine: &Engine, code: bool) -> Option<String> {
    if code {
        if let Ok(slot) = engine.code_searcher.lock() {
            if let Some(source) = slot.as_ref().and_then(|s| s.vector_source.clone()) {
                return Some(source);
            }
        }
    }
    engine
        .searcher
        .lock()
        .ok()
        .and_then(|s| s.vector_source.clone())
}

fn prefer_code_store(engine: &Engine, explicit: bool) -> bool {
    if explicit {
        return true;
    }
    engine
        .newest_typed
        .lock()
        .ok()
        .and_then(|k| k.clone())
        .is_some_and(|k| k.starts_with("code\u{0}"))
}

fn preview_body(
    engine: &Engine,
    segment_id: i64,
    width: u32,
    query: Option<&str>,
    prefer_code: bool,
) -> Value {
    lookup_segment(engine, segment_id, prefer_code, |searcher| {
        previews::preview(searcher, segment_id, width, query)
    })
    .unwrap_or_else(|| json!({"error": {"code": "not_found", "message": "unknown segment"}}))
}

fn text_of_body(engine: &Engine, segment_id: i64, prefer_code: bool) -> Value {
    lookup_segment(engine, segment_id, prefer_code, |searcher| {
        previews::text_of(searcher, segment_id)
    })
    .unwrap_or_else(|| json!({"error": {"code": "not_found", "message": "unknown segment"}}))
}

fn thumbnail_body(
    engine: &Engine,
    segment_id: Option<i64>,
    path: Option<&Path>,
    size: u32,
    prefer_code: bool,
) -> Value {
    if let Some(id) = segment_id {
        if let Some(body) = lookup_segment(engine, id, prefer_code, |searcher| {
            previews::thumbnail(searcher, Some(id), path, size)
        }) {
            return body;
        }
        if path.is_none() {
            return json!({"error": {"code": "not_found", "message": "no file"}});
        }
    }
    let searcher = engine.searcher.lock().expect("searcher");
    previews::thumbnail(&searcher, segment_id, path, size)
}

fn segment_meta_any(
    engine: &Engine,
    segment_id: i64,
    prefer_code: bool,
) -> Option<previews::SegmentMeta> {
    lookup_segment(engine, segment_id, prefer_code, |searcher| {
        previews::segment_meta(searcher, segment_id)
    })
    .flatten()
}

fn lookup_segment<T>(
    engine: &Engine,
    segment_id: i64,
    prefer_code: bool,
    mut f: impl FnMut(&Searcher) -> T,
) -> Option<T> {
    let try_main = |engine: &Engine, f: &mut dyn FnMut(&Searcher) -> T| -> Option<T> {
        let searcher = engine.searcher.lock().expect("searcher");
        previews::segment_meta(&searcher, segment_id).map(|_| f(&searcher))
    };
    let try_code = |engine: &Engine, f: &mut dyn FnMut(&Searcher) -> T| -> Option<T> {
        ensure_code_searcher(engine);
        let slot = engine.code_searcher.lock().expect("code_searcher");
        slot.as_ref()
            .and_then(|searcher| previews::segment_meta(searcher, segment_id).map(|_| f(searcher)))
    };
    if prefer_code {
        try_code(engine, &mut f).or_else(|| try_main(engine, &mut f))
    } else {
        try_main(engine, &mut f).or_else(|| try_code(engine, &mut f))
    }
}

fn emit_status(conn: &Conn, engine: &Engine, force: bool) {
    let now = Instant::now();
    if !force {
        let mut last = conn.last_status.lock().expect("last_status");
        if let Some(prev) = *last {
            if now.duration_since(prev) < Duration::from_millis(250) {
                push_status_event(conn, engine);
                return;
            }
        }
        *last = Some(now);
    }
    push_status_event(conn, engine);
}

fn push_status_event(conn: &Conn, engine: &Engine) {
    let snap = engine.status.lock().expect("status").clone();
    let mut v = serde_json::to_value(snap).unwrap_or(json!({}));
    if let Value::Object(map) = &mut v {
        map.insert("event".into(), json!("status"));
    }
    conn.out.push_status(v);
}

fn reply_preview_line(out: &Outbox, id: u64, body: Value) {
    if let Some(err) = body.get("error") {
        let code = match err.get("code").and_then(Value::as_str) {
            Some("not_found") => ErrorCode::NotFound,
            Some("io") => ErrorCode::Io,
            _ => ErrorCode::Internal,
        };
        let message = err
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("preview failed");
        reply_error_line(out, id, code, message);
        return;
    }
    let mut body = body;
    if let Value::Object(map) = &mut body {
        map.insert("id".into(), json!(id));
        map.insert("ok".into(), json!(true));
    }
    out.push_line(body.to_string());
}

/// A few threads for renders, so ten thumbnails don't start ten ffmpegs at once.
struct RenderPool {
    tx: Mutex<std::sync::mpsc::Sender<Box<dyn FnOnce() + Send>>>,
}

impl RenderPool {
    fn run(&self, job: impl FnOnce() + Send + 'static) {
        if let Ok(tx) = self.tx.lock() {
            let _ = tx.send(Box::new(job));
        }
    }
}

fn render_pool() -> &'static RenderPool {
    static POOL: std::sync::OnceLock<RenderPool> = std::sync::OnceLock::new();
    POOL.get_or_init(|| {
        let (tx, rx) = std::sync::mpsc::channel::<Box<dyn FnOnce() + Send>>();
        let rx = Arc::new(Mutex::new(rx));
        for n in 0..3 {
            let rx = Arc::clone(&rx);
            let _ = thread::Builder::new()
                .name(format!("troweld-render-{n}"))
                .spawn(move || loop {
                    let job = match rx.lock() {
                        Ok(rx) => rx.recv(),
                        Err(_) => return,
                    };
                    match job {
                        Ok(job) => job(),
                        Err(_) => return,
                    }
                });
        }
        RenderPool { tx: Mutex::new(tx) }
    })
}

/// Previews already rendered, by what was asked and the file's mtime: going back up the list
/// (or an audio waveform, 450 ms each time) is instant. Upstream keeps `Previews.shared`.
#[derive(Default)]
struct PreviewCache {
    entries: VecDeque<(PreviewKey, Value)>,
}

#[derive(PartialEq, Eq, Clone)]
struct PreviewKey {
    code: bool,
    segment: i64,
    width: u32,
    query: Option<String>,
    path: PathBuf,
    mtime: Option<std::time::SystemTime>,
}

const PREVIEW_CACHE_ENTRIES: usize = 48;

fn cached_preview(
    engine: &Engine,
    segment_id: i64,
    width: u32,
    query: Option<&str>,
    prefer_code: bool,
) -> Value {
    let Some(meta) = segment_meta_any(engine, segment_id, prefer_code) else {
        return preview_body(engine, segment_id, width, query, prefer_code);
    };
    let key = PreviewKey {
        code: prefer_code,
        segment: segment_id,
        width,
        query: query.map(str::to_string),
        mtime: fs::metadata(&meta.path).and_then(|m| m.modified()).ok(),
        path: meta.path,
    };
    if let Ok(cache) = engine.preview_cache.lock() {
        if let Some((_, body)) = cache.entries.iter().find(|(k, _)| *k == key) {
            if rendered_files_exist(body) {
                return body.clone();
            }
        }
    }
    let body = preview_body(engine, segment_id, width, query, prefer_code);
    if body.get("error").is_none() {
        if let Ok(mut cache) = engine.preview_cache.lock() {
            cache.entries.retain(|(k, _)| *k != key);
            cache.entries.push_back((key, body.clone()));
            while cache.entries.len() > PREVIEW_CACHE_ENTRIES {
                cache.entries.pop_front();
            }
        }
    }
    body
}

/// Rendered files may have been cleaned up since (`previews` keeps a bounded folder).
fn rendered_files_exist(body: &Value) -> bool {
    ["file", "frame", "thumb"].iter().all(|k| {
        body.get(*k)
            .and_then(Value::as_str)
            .is_none_or(|p| Path::new(p).exists())
    })
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
    reply_error_line(&conn.out, id.unwrap_or(0), code, message);
}

fn reply_error_line(out: &Outbox, id: u64, code: ErrorCode, message: impl Into<String>) {
    let err = ErrorBody::new(code, message);
    let mut body = json!({"ok": false, "error": err});
    if id != 0 {
        if let Value::Object(map) = &mut body {
            map.insert("id".into(), json!(id));
        }
    }
    out.push_line(body.to_string());
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

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

impl Engine {
    /// Runs `f` on `state.sqlite`, opening it on first use.
    fn with_state<T>(
        &self,
        f: impl FnOnce(&StateDb) -> Result<T, trowel_core::DbError>,
    ) -> Result<T, String> {
        let mut slot = self.state.lock().expect("state");
        if slot.is_none() {
            *slot = Some(StateDb::open(&self.state_dir).map_err(|e| e.to_string())?);
        }
        f(slot.as_ref().expect("state open")).map_err(|e| e.to_string())
    }

    fn touch(&self) {
        if let Ok(mut last) = self.last_use.lock() {
            *last = Instant::now();
        }
    }

    fn idle_for(&self) -> Duration {
        self.last_use
            .lock()
            .ok()
            .map(|t| t.elapsed())
            .unwrap_or_default()
    }

    fn encoder_idle(&self) -> Duration {
        let secs = self
            .config
            .lock()
            .ok()
            .map(|c| c.encoder_idle_seconds)
            .unwrap_or(600);
        Duration::from_secs(secs.max(1))
    }

    fn onboarded(&self) -> bool {
        self.config.lock().ok().is_some_and(|c| c.onboarded)
    }

    fn request_sync(self: &Arc<Self>) {
        if !self.onboarded() || self.paused.load(Ordering::SeqCst) {
            self.sync_wanted.store(true, Ordering::SeqCst);
            return;
        }
        self.sync_wanted.store(true, Ordering::SeqCst);
        if let Some(helper) = self.helper.lock().expect("helper").clone() {
            if helper.running() {
                helper.send("sync");
                return;
            }
        }
        self.start_helper();
    }

    /// Tell a running helper to start or stop the backfill, or spawn one when
    /// AC (or `index_on_battery`) now allows it.
    /// `upstream/Sources/DigUpApp/Engine.swift:707` `setBackfillBlocker`.
    fn apply_backfill_policy(self: &Arc<Self>) {
        if !self.onboarded() || self.paused.load(Ordering::SeqCst) {
            return;
        }
        let index_on_battery = self
            .config
            .lock()
            .ok()
            .map(|c| c.index_on_battery)
            .unwrap_or(false);
        let allowed = power::backfill_blocker(index_on_battery).is_none();
        if let Some(helper) = self.helper.lock().expect("helper").clone() {
            if helper.running() {
                helper.send(if allowed {
                    "backfill on"
                } else {
                    "backfill off"
                });
                return;
            }
        }
        if allowed {
            self.start_helper();
        }
    }

    fn request_code_sync(self: &Arc<Self>) {
        if self.paused.load(Ordering::SeqCst) {
            return;
        }
        self.sync_wanted.store(true, Ordering::SeqCst);
        if let Some(helper) = self.helper.lock().expect("helper").clone() {
            if helper.running() {
                helper.send("sync code");
                return;
            }
        }
        self.start_helper();
    }

    fn start_helper(self: &Arc<Self>) {
        if self.paused.load(Ordering::SeqCst) || !self.onboarded() {
            eprintln!(
                "troweld: not starting the helper: paused={} onboarded={}",
                self.paused.load(Ordering::SeqCst),
                self.onboarded()
            );
            return;
        }
        if self.sleeping.load(Ordering::SeqCst) {
            eprintln!("troweld: not starting the helper: the system is going to sleep");
            return;
        }
        if let Some(reason) = disk::too_full(&self.index_dir) {
            eprintln!("troweld: not starting the helper: {reason}");
            let mut status = self.status.lock().expect("status");
            status.paused_reason = Some(reason);
            status.phase = "paused".into();
            return;
        }
        if let Some(helper) = self.helper.lock().expect("helper").clone() {
            if helper.running() {
                eprintln!("troweld: helper running; asked it to sync");
                helper.send("sync");
                return;
            }
        }
        if self.helper_starting.swap(true, Ordering::SeqCst) {
            eprintln!("troweld: not starting the helper: one is starting");
            return;
        }
        let cfg = self.config.lock().expect("config").clone();
        if cfg.roots.is_empty() && cfg.code_roots.is_empty() {
            self.helper_starting.store(false, Ordering::SeqCst);
            return;
        }
        let backfill = power::backfill_blocker(cfg.index_on_battery).is_none();
        let opts = HelperOpts {
            index_dir: self.index_dir.clone(),
            fake: self.fake,
            backfill,
            linger: helper::linger_seconds(),
        };
        let eng = Arc::clone(self);
        match Helper::spawn(opts, &cfg, move |event| eng.on_helper_event(event)) {
            Ok(h) => {
                if !backfill {
                    h.send("backfill off");
                }
                *self.helper.lock().expect("helper") = Some(h);
                let mut status = self.status.lock().expect("status");
                if status.phase == "idle" {
                    status.phase = "syncing".into();
                }
            }
            Err(err) => {
                eprintln!("troweld helper: {err}");
                self.status.lock().expect("status").last_error =
                    Some(format!("Can't start indexing: {err}"));
                self.status.lock().expect("status").phase = "error".into();
            }
        }
        self.helper_starting.store(false, Ordering::SeqCst);
        self.broadcast_status();
    }

    fn on_helper_event(self: &Arc<Self>, event: Value) {
        let Some(kind) = event.get("event").and_then(Value::as_str) else {
            return;
        };
        let code = event.get("code").and_then(Value::as_bool).unwrap_or(false);
        match kind {
            "sync" => {
                let new = event.get("new").and_then(Value::as_u64).unwrap_or(0);
                if new > 0 {
                    // A code sync is the code pass's progress, never the main one's: it set
                    // main to 0/N, and nothing about the main index ever corrected it (the
                    // badge and bar sat at 0% while code was indexed).
                    // `upstream/Sources/DigUpApp/Engine.swift` handleCode "sync".
                    let mut status = self.status.lock().expect("status");
                    status.phase = "indexing".into();
                    let pass = if code {
                        &mut status.code
                    } else {
                        &mut status.main
                    };
                    pass.total = new;
                    pass.done = 0;
                } else {
                    self.status.lock().expect("status").phase = "syncing".into();
                }
                self.apply_counts(&event);
            }
            "file" => {
                let mut status = self.status.lock().expect("status");
                status.phase = "indexing".into();
                if code {
                    // The helper reads code only once the main index has nothing waiting:
                    // the main pass is over.
                    status.main = Default::default();
                    status.code.done = event.get("done").and_then(Value::as_u64).unwrap_or(0);
                    status.code.total = event.get("total").and_then(Value::as_u64).unwrap_or(0);
                    status.code.current = event
                        .get("path")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                } else {
                    status.main.done = event.get("done").and_then(Value::as_u64).unwrap_or(0);
                    status.main.total = event.get("total").and_then(Value::as_u64).unwrap_or(0);
                    status.main.current = event
                        .get("path")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                }
                drop(status);
                reload_searcher(self, false);
            }
            "rest" => {
                self.status.lock().expect("status").phase = "finishing_long_files".into();
                reload_searcher(self, false);
            }
            "idle" => {
                self.sync_wanted.store(false, Ordering::SeqCst);
                {
                    let mut status = self.status.lock().expect("status");
                    status.phase = "idle".into();
                    status.main = Default::default();
                    status.code = Default::default();
                }
                self.apply_counts(&event);
                reload_searcher(self, true);
            }
            "waiting" => {
                self.status.lock().expect("status").phase = "waiting_for_model".into();
                self.apply_counts(&event);
            }
            "error" => {
                let msg = event
                    .get("message")
                    .and_then(Value::as_str)
                    .unwrap_or("indexing error")
                    .to_string();
                let mut status = self.status.lock().expect("status");
                status.last_error = Some(msg);
                if event.get("fatal").and_then(Value::as_bool).unwrap_or(false) {
                    status.phase = "error".into();
                }
            }
            "reindex" => {
                let from = event.get("from").and_then(Value::as_str).unwrap_or("?");
                let to = event.get("to").and_then(Value::as_str).unwrap_or("?");
                eprintln!("troweld indexer: model or settings changed, reindexing (was: {from}; now: {to})");
            }
            "exit" | "exited" => {
                *self.helper.lock().expect("helper") = None;
                // Must drop this before start_helper(); otherwise the next
                // spawn is skipped forever after the first helper exits.
                self.helper_starting.store(false, Ordering::SeqCst);
                {
                    let mut status = self.status.lock().expect("status");
                    if status.phase == "indexing"
                        || status.phase == "syncing"
                        || status.phase == "finishing_long_files"
                    {
                        status.phase = "idle".into();
                    }
                }
                reload_searcher(self, true);
                if self.sync_wanted.load(Ordering::SeqCst) && !self.paused.load(Ordering::SeqCst) {
                    self.start_helper();
                }
            }
            _ => {}
        }
        self.broadcast_status();
    }

    /// `status.counts`: searchable files per kind in both indexes (PROTOCOL §5).
    fn apply_counts(&self, _event: &Value) {
        let mut counts = trowel_proto::status::Counts::default();
        let mut add = |by_kind: &std::collections::BTreeMap<
            String,
            std::collections::BTreeMap<String, usize>,
        >| {
            for (kind, states) in by_kind {
                let n = states
                    .iter()
                    .filter(|(state, _)| state.as_str() == "done" || state.as_str() == "partial")
                    .map(|(_, n)| *n as u64)
                    .sum::<u64>();
                match kind.as_str() {
                    "screenshot" => counts.screenshots += n,
                    "image" => counts.images += n,
                    "pdf" | "doc" => counts.documents += n,
                    "video" => counts.video += n,
                    "audio" => counts.audio += n,
                    "code" => counts.code += n,
                    _ => {}
                }
            }
        };
        if let Ok(searcher) = self.searcher.lock() {
            if let Ok(c) = searcher.store.counts() {
                add(&c.by_kind_and_state);
            }
        }
        if let Ok(slot) = self.code_searcher.lock() {
            if let Some(c) = slot.as_ref().and_then(|s| s.store.counts().ok()) {
                add(&c.by_kind_and_state);
            }
        }
        self.status.lock().expect("status").counts = counts;
    }

    fn broadcast_status(&self) {
        let snap = self.status.lock().expect("status").clone();
        let mut v = serde_json::to_value(snap).unwrap_or(json!({}));
        if let Value::Object(map) = &mut v {
            map.insert("event".into(), json!("status"));
        }
        let mut subs = self.subscribers.lock().expect("subscribers");
        subs.retain(|out| !out.dead.load(Ordering::Relaxed));
        for out in subs.iter() {
            out.push_status(v.clone());
        }
    }

    fn embed_ahead(self: &Arc<Self>, text: String, code: bool) {
        self.touch();
        let key = QueryCache::key(&text, code);
        *self.newest_typed.lock().expect("typed") = Some(key.clone());
        let engine = Arc::clone(self);
        thread::spawn(move || {
            let newest = engine.newest_typed.lock().expect("typed").clone();
            if newest.as_deref() != Some(key.as_str()) {
                return;
            }
            let _ = query_vector(&engine, &text, code);
        });
    }

    fn release_idle_memory(&self) {
        if let Ok(mut searcher) = self.searcher.lock() {
            searcher.release_vectors();
            self.status.lock().expect("status").vectors_loaded = false;
        }
        if let Ok(mut slot) = self.code_searcher.lock() {
            if let Some(searcher) = slot.as_mut() {
                searcher.release_vectors();
            }
        }
        if let Ok(mut slot) = self.encoder.lock() {
            if let Some(mut enc) = slot.take() {
                enc.close();
            }
        }
        if !self.fake {
            self.status.lock().expect("status").worker = "down".into();
        }
        self.query_cache.clear();
    }

    fn enter_sleep(&self) {
        if let Some(helper) = self.helper.lock().expect("helper").as_ref() {
            helper.stop();
        }
        *self.watch.lock().expect("watch") = None;
        *self.code_watch.lock().expect("code_watch") = None;
    }

    fn leave_sleep(self: &Arc<Self>) {
        if self.onboarded() {
            start_watch(self);
            self.request_sync();
        }
    }

    fn reload_config_file(&self) {
        match config_file::load(&self.config_path) {
            Ok(next) => {
                *self.config.lock().expect("config") = next;
            }
            Err(err) => {
                self.status.lock().expect("status").last_error =
                    Some(format!("invalid config: {err}"));
            }
        }
    }
}

fn start_watch(engine: &Arc<Engine>) {
    let cfg = engine.config.lock().expect("config").clone();
    let roots: Vec<PathBuf> = cfg.roots.iter().map(|r| expand_tilde(r)).collect();
    let code_roots: Vec<PathBuf> = cfg.code_roots.iter().map(|r| expand_tilde(r)).collect();
    let poll: Vec<PathBuf> = roots
        .iter()
        .chain(code_roots.iter())
        .filter(|p| fs_kind::classify(p) != RootKind::Local)
        .cloned()
        .collect();
    let local: Vec<PathBuf> = roots
        .iter()
        .filter(|p| fs_kind::classify(p) == RootKind::Local)
        .cloned()
        .collect();
    if local.is_empty() {
        *engine.watch.lock().expect("watch") = None;
    } else {
        let eng = Arc::clone(engine);
        let walks = crawl_walks(&cfg.excluded_folders, false);
        match FolderWatch::start_scoped(local, DEBOUNCE, walks, move |_| {
            if !eng.paused.load(Ordering::SeqCst) && !eng.sleeping.load(Ordering::SeqCst) {
                eng.request_sync();
            }
        }) {
            Ok(watch) => *engine.watch.lock().expect("watch") = Some(watch),
            Err(err) => {
                eprintln!("troweld watcher: {err}");
                engine.overflow.store(true, Ordering::SeqCst);
            }
        }
    }
    let local_code: Vec<PathBuf> = code_roots
        .iter()
        .filter(|p| fs_kind::classify(p) == RootKind::Local)
        .cloned()
        .collect();
    if local_code.is_empty() {
        *engine.code_watch.lock().expect("code_watch") = None;
    } else {
        let eng = Arc::clone(engine);
        let walks = crawl_walks(&cfg.code_excluded, true);
        match FolderWatch::start_code(
            local_code,
            CODE_QUIET,
            CODE_LONGEST_WAIT,
            walks,
            move |_| {
                if !eng.paused.load(Ordering::SeqCst) && !eng.sleeping.load(Ordering::SeqCst) {
                    eng.request_code_sync();
                }
            },
        ) {
            Ok(watch) => *engine.code_watch.lock().expect("code_watch") = Some(watch),
            Err(err) => eprintln!("troweld code watcher: {err}"),
        }
    }
    if !poll.is_empty() {
        let eng = Arc::clone(engine);
        thread::spawn(move || loop {
            thread::sleep(Duration::from_secs(30 * 60));
            if eng.onboarded() && !eng.paused.load(Ordering::SeqCst) {
                eng.request_sync();
            }
        });
    }
}

/// The folders the crawl walks, for the watcher (`Rules::walks_into`). Skipped folders are
/// matched as written and as real paths, as upstream `startWatching` does.
fn crawl_walks(excluded: &[String], code: bool) -> crate::watcher::Walks {
    let home = PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".into()));
    let mut skips: Vec<String> = Vec::new();
    for folder in excluded {
        let path = expand_tilde(folder).to_string_lossy().into_owned();
        let real = trowel_core::folder_selection::real_path(&path);
        skips.push(path);
        if !skips.contains(&real) {
            skips.push(real);
        }
    }
    Arc::new(move |folder: &Path| Rules::walks_into(folder, &home, &skips, code))
}

fn start_config_watch(engine: &Arc<Engine>) {
    let path = engine.config_path.clone();
    let Some(parent) = path.parent().map(PathBuf::from) else {
        return;
    };
    if !parent.is_dir() {
        return;
    }
    let name = path.file_name().map(|s| s.to_os_string());
    let eng = Arc::clone(engine);
    // Only the config folder itself, not what is below it.
    let only_root: crate::watcher::Walks = Arc::new(|_| false);
    match FolderWatch::start_scoped(
        vec![parent],
        Duration::from_millis(400),
        only_root,
        move |paths| {
            if paths.iter().any(|p| {
                name.as_ref()
                    .is_some_and(|n| p.file_name() == Some(n.as_os_str()))
            }) {
                eng.reload_config_file();
            }
        },
    ) {
        Ok(watch) => *engine.config_watch.lock().expect("config_watch") = Some(watch),
        Err(err) => eprintln!("troweld config watch: {err}"),
    }
}

fn spawn_idle_reaper(engine: Arc<Engine>) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_millis(200));
        if engine.idle_for() >= engine.encoder_idle() {
            engine.release_idle_memory();
        }
        let overflowed = engine.overflow.swap(false, Ordering::SeqCst)
            || engine
                .watch
                .lock()
                .ok()
                .and_then(|g| g.as_ref().map(FolderWatch::take_overflow))
                .unwrap_or(false)
            || engine
                .code_watch
                .lock()
                .ok()
                .and_then(|g| g.as_ref().map(FolderWatch::take_overflow))
                .unwrap_or(false);
        if overflowed {
            engine.request_sync();
        }
        let sleep = engine.sleeping.load(Ordering::SeqCst);
        let was = engine.was_sleeping.swap(sleep, Ordering::SeqCst);
        if sleep && !was {
            engine.enter_sleep();
        }
        if !sleep && was {
            engine.leave_sleep();
        }
        let on_battery = power::on_battery();
        let was_battery = engine.was_on_battery.swap(on_battery, Ordering::SeqCst);
        if was_battery != on_battery {
            // upstream/Sources/DigUpApp/Engine.swift setBackfillBlocker
            engine.apply_backfill_policy();
        }
    });
}

fn spawn_watchdog(engine: Arc<Engine>) {
    thread::spawn(move || loop {
        thread::sleep(Duration::from_secs(10));
        sd_notify("WATCHDOG=1");
        let _ = engine.idle_for();
    });
}

fn estimate_path(path: PathBuf, code: bool) -> Value {
    let options = trowel_core::IndexOptions {
        code,
        include_code: code,
        ..Default::default()
    };
    if !path.is_dir() {
        return json!({
            "path": path.display().to_string(),
            "files": 0,
            "by_kind": {},
            "est_seconds": 0,
            "est_bytes": 0,
            "skips": ["can't read it"],
        });
    }
    let report = trowel_core::Crawler::crawl(std::slice::from_ref(&path), &options);
    let estimate = trowel_core::Estimator::estimate(
        &report.candidates,
        &options,
        &trowel_core::Costs::default(),
        report.directories,
    );
    let mut by_kind = serde_json::Map::new();
    for (kind, n) in estimate.files {
        by_kind.insert(kind.as_str().to_string(), json!(n));
    }
    json!({
        "path": path.display().to_string(),
        "files": report.candidates.len(),
        "by_kind": by_kind,
        "est_seconds": estimate.seconds,
        "est_bytes": estimate.bytes,
        "skips": report.skipped_folders.keys().cloned().collect::<Vec<String>>(),
        "directories": estimate.directories,
    })
}

fn model_status_json() -> Value {
    use trowel_core::ModelFiles;
    let present = ModelFiles::complete();
    let mut bytes = 0u64;
    if let Ok(text) = ModelFiles::locate_text() {
        bytes += std::fs::metadata(text).map(|m| m.len()).unwrap_or(0);
    }
    if let Some(media) = ModelFiles::locate_media() {
        bytes += std::fs::metadata(media).map(|m| m.len()).unwrap_or(0);
    }
    json!({
        "present": present,
        "verified": present,
        "bytes": bytes,
        "total": ModelFiles::TOTAL_BYTES,
        "downloading": false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hybrid_without_vectors_keeps_prefix() {
        assert_eq!(search_plan(SearchMode::Keyword, true, false), (false, true));
        assert_eq!(search_plan(SearchMode::Hybrid, false, false), (false, true));
        assert_eq!(search_plan(SearchMode::Hybrid, false, true), (true, false));
        assert_eq!(search_plan(SearchMode::Hybrid, true, true), (true, true));
    }

    #[test]
    fn sd_notify_sends_datagram() {
        let path = std::env::temp_dir().join(format!("trowel-notify-{}.sock", std::process::id()));
        let _ = fs::remove_file(&path);
        let sock = UnixDatagram::bind(&path).expect("bind notify sock");
        sock.set_read_timeout(Some(Duration::from_secs(2)))
            .expect("read timeout");
        let prev = std::env::var_os("NOTIFY_SOCKET");
        unsafe { std::env::set_var("NOTIFY_SOCKET", &path) };
        sd_notify("READY=1");
        match prev {
            Some(p) => unsafe { std::env::set_var("NOTIFY_SOCKET", p) },
            None => unsafe { std::env::remove_var("NOTIFY_SOCKET") },
        }
        let mut buf = [0u8; 32];
        let n = sock.recv(&mut buf).expect("recv READY");
        let _ = fs::remove_file(&path);
        assert_eq!(&buf[..n], b"READY=1");
    }
}
