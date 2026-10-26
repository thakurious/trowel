//! `trowel clip-capture` (run by the daemon's `wl-paste --watch`) and `trowel clip …`
//! (`LAUNCHER-PLAN.md` L3).

use std::fs;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use troweld::clip::ingest::{self, Offer};
use troweld::clip::store::{Limits, NewEntry};
use troweld::clip::watch::{ENV_EXCLUDE, ENV_MAX_BYTES, ENV_SOCKET, ENV_SPOOL};
use troweld::state::StateDb;

const FETCH_TIMEOUT: Duration = Duration::from_secs(2);

/// Runs `argv`, keeping at most `cap` bytes of stdout in `dest` (or memory when `dest` is
/// `None`). `None` on timeout, failure, or output over the cap.
fn run_capped(argv: &[&str], cap: u64, dest: Option<&Path>) -> Option<Vec<u8>> {
    let mut child = Command::new(argv[0])
        .args(&argv[1..])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let stdout = child.stdout.take()?;
    let dest = dest.map(Path::to_path_buf);
    let reader = thread::spawn(move || -> io::Result<(u64, Vec<u8>)> {
        let mut limited = stdout.take(cap + 1);
        match dest {
            Some(path) => {
                let mut out = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(path)?;
                Ok((io::copy(&mut limited, &mut out)?, Vec::new()))
            }
            None => {
                let mut buf = Vec::new();
                limited.read_to_end(&mut buf)?;
                Ok((buf.len() as u64, buf))
            }
        }
    });
    let deadline = Instant::now() + FETCH_TIMEOUT;
    let ok = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status.success(),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break false;
            }
        }
    };
    let (n, buf) = reader.join().ok()?.ok()?;
    (ok && n <= cap).then_some(buf)
}

fn source_app() -> Option<String> {
    let out = run_capped(&["hyprctl", "-j", "activewindow"], 1 << 20, None)?;
    let v: Value = serde_json::from_slice(&out).ok()?;
    v["class"]
        .as_str()
        .filter(|c| !c.is_empty())
        .map(str::to_string)
}

/// `wl-paste --watch` runs this once per selection change. Never prints clipboard content.
pub fn capture() -> i32 {
    // The watcher pipes the selection to stdin; it is fetched per type below instead.
    thread::spawn(|| {
        let _ = io::copy(&mut io::stdin(), &mut io::sink());
    });
    // `sensitive` is a password manager's copy; `clear`/`nil` means the selection went away.
    if let Ok(state) = std::env::var("CLIPBOARD_STATE") {
        if matches!(state.as_str(), "sensitive" | "clear" | "nil") {
            return 0;
        }
    }
    let (Some(socket), Some(spool)) = (std::env::var_os(ENV_SOCKET), std::env::var_os(ENV_SPOOL))
    else {
        eprintln!("trowel clip-capture: run by troweld's clipboard watcher only");
        return 2;
    };
    let max_bytes: u64 = std::env::var(ENV_MAX_BYTES)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(64 << 20);
    let Some(list) = run_capped(&["wl-paste", "--list-types"], 1 << 16, None) else {
        return 0;
    };
    let types: Vec<String> = String::from_utf8_lossy(&list)
        .lines()
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect();
    if types.iter().any(|t| t == ingest::SENSITIVE_MIME) {
        return 0;
    }
    let app = source_app();
    let excluded = std::env::var(ENV_EXCLUDE).unwrap_or_default();
    if let Some(app) = &app {
        if excluded
            .lines()
            .any(|x| !x.trim().is_empty() && x.trim().eq_ignore_ascii_case(app))
        {
            return 0;
        }
    }
    let spool = PathBuf::from(spool);
    let mut offers = Vec::new();
    for (i, mime) in ingest::wanted(&types).iter().enumerate() {
        let file = spool.join(format!("{}-{i}", std::process::id()));
        let _ = fs::remove_file(&file);
        let cap = if mime == ingest::SOURCE_URL_MIME {
            4096
        } else {
            max_bytes
        };
        let got = run_capped(
            &["wl-paste", "--no-newline", "--type", mime],
            cap,
            Some(&file),
        );
        if got.is_some() && fs::metadata(&file).map(|m| m.len() > 0).unwrap_or(false) {
            offers.push(json!({"mime": mime, "file": file}));
        } else {
            let _ = fs::remove_file(&file);
        }
    }
    if offers.is_empty() {
        return 0;
    }
    let files: Vec<PathBuf> = offers
        .iter()
        .filter_map(|o| o["file"].as_str().map(PathBuf::from))
        .collect();
    let req = json!({"op": "clip_ingest", "types": types, "offers": offers, "source_app": app});
    let result = request_at(Path::new(&socket), req, Duration::from_secs(10));
    // The daemon deletes the spool files it read; these only remain when it never answered.
    for file in files {
        let _ = fs::remove_file(file);
    }
    match result {
        Ok(_) => 0,
        Err(err) => {
            eprintln!("trowel clip-capture: {err}");
            1
        }
    }
}

pub(crate) fn socket_path() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("trowel/troweld.sock")
}

/// One request after `hello`. Errors carry the daemon's message.
pub(crate) fn request_at(socket: &Path, req: Value, timeout: Duration) -> Result<Value, String> {
    let stream = UnixStream::connect(socket)
        .map_err(|e| format!("no daemon at {} ({e})", socket.display()))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|e| e.to_string())?;
    let mut writer = stream.try_clone().map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream);
    let mut req = req;
    req["id"] = json!(2);
    for msg in [
        json!({"id":1,"op":"hello","protocol":trowel_proto::PROTOCOL_VERSION,"client":"cli","pid":std::process::id()}),
        req,
    ] {
        writeln!(writer, "{msg}").map_err(|e| e.to_string())?;
    }
    let mut line = String::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).map_err(|e| e.to_string())? == 0 {
            return Err("the daemon closed the connection".into());
        }
        let v: Value = serde_json::from_str(line.trim()).map_err(|e| e.to_string())?;
        if v.get("event").is_some() {
            continue;
        }
        if v["ok"] == false {
            return Err(v["error"]["message"]
                .as_str()
                .unwrap_or("failed")
                .to_string());
        }
        if v["id"] == 2 {
            return Ok(v);
        }
    }
}

fn request(req: Value) -> Result<Value, String> {
    request_at(&socket_path(), req, Duration::from_secs(30))
}

pub enum ClipAction {
    List {
        query: Option<String>,
        kinds: Vec<String>,
        limit: u32,
        json: bool,
    },
    Get {
        id: i64,
        json: bool,
    },
    Paste {
        id: i64,
        mode: String,
    },
    Pin {
        id: i64,
        off: bool,
    },
    Delete {
        id: i64,
    },
    Clear {
        all: bool,
    },
    Pause {
        minutes: Option<u32>,
    },
    Resume,
    ImportOmarchy,
    Bench {
        entries: u32,
        image: Option<PathBuf>,
    },
}

fn ago(at: i64) -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let s = (now - at).max(0);
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m", s / 60),
        3600..=86_399 => format!("{}h", s / 3600),
        _ => format!("{}d", s / 86_400),
    }
}

pub fn run(action: ClipAction) -> i32 {
    let result = match action {
        ClipAction::List {
            query,
            kinds,
            limit,
            json,
        } => {
            let mut req =
                json!({"op":"clip_query","text":query.unwrap_or_default(),"limit":limit,"gen":1});
            if !kinds.is_empty() {
                req["kinds"] = json!(kinds);
            }
            request(req).map(|v| {
                if json {
                    println!("{v}");
                } else {
                    for row in v["rows"].as_array().into_iter().flatten() {
                        println!(
                            "{:>6} {}{:<6} {:>4} {}",
                            row["id"],
                            if row["pinned"] == true { "*" } else { " " },
                            row["kind"].as_str().unwrap_or(""),
                            ago(row["last_used"].as_i64().unwrap_or(0)),
                            row["preview"].as_str().unwrap_or("")
                        );
                    }
                }
            })
        }
        ClipAction::Get { id, json } => request(json!({"op":"clip_get","clip_id":id})).map(|v| {
            if json {
                println!("{v}");
            } else if let Some(text) = v["text"].as_str() {
                print!("{text}");
            } else if let Some(files) = v["files"].as_array() {
                for f in files {
                    println!(
                        "{}",
                        f.as_str()
                            .unwrap_or_else(|| f["display"].as_str().unwrap_or(""))
                    );
                }
            } else if let Some(file) = v["image"]["file"].as_str() {
                println!("{file}");
            }
        }),
        ClipAction::Paste { id, mode } => {
            request(json!({"op":"clip_paste","clip_id":id,"mode":mode})).map(|_| ())
        }
        ClipAction::Pin { id, off } => {
            request(json!({"op":"clip_pin","clip_id":id,"pinned":!off})).map(|_| ())
        }
        ClipAction::Delete { id } => request(json!({"op":"clip_delete","clip_id":id})).map(|_| ()),
        ClipAction::Clear { all } => request(json!({"op":"clip_clear","keep_pinned":!all}))
            .map(|v| println!("removed {}", v["removed"])),
        ClipAction::Pause { minutes } => {
            let mut req = json!({"op":"clip_pause"});
            if let Some(m) = minutes {
                req["minutes"] = json!(m);
            }
            request(req).map(|v| match v["paused_until"].as_i64() {
                Some(-1) => println!("clipboard capture paused until resumed"),
                Some(0) => println!("clipboard capture on"),
                Some(t) => println!(
                    "clipboard capture paused for {} more minutes",
                    (t - now() + 59) / 60
                ),
                None => {}
            })
        }
        ClipAction::Resume => request(json!({"op":"clip_pause","minutes":0}))
            .map(|_| println!("clipboard capture on")),
        ClipAction::ImportOmarchy => request(json!({"op":"clip_import","source":"omarchy"}))
            .map(|v| println!("imported {}, skipped {}", v["imported"], v["skipped"])),
        ClipAction::Bench { entries, image } => bench(entries, image.as_deref()),
    };
    match result {
        Ok(()) => 0,
        Err(err) => {
            eprintln!("trowel clip: {err}");
            1
        }
    }
}

fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn pct(sorted: &[f64], p: f64) -> f64 {
    sorted[((sorted.len() as f64 - 1.0) * p).round() as usize]
}

/// `LAUNCHER-PLAN.md` L3 acceptance numbers, in-process on a scratch `state.sqlite`: `clip_query`
/// over `entries` rows (p95 < 5 ms) and one large image ingest (< 150 ms, thumbnail excluded).
fn bench(entries: u32, image: Option<&Path>) -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("trowel-clip-bench-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    let state = StateDb::open(&dir).map_err(|e| e.to_string())?;
    let words = [
        "firewall",
        "rules",
        "staging",
        "cluster",
        "invoice",
        "refund",
        "policy",
        "meeting",
        "notes",
        "kubernetes",
        "deploy",
        "harbour",
        "receipt",
        "budget",
        "draft",
        "review",
    ];
    let t0 = now() - i64::from(entries);
    for i in 0..entries {
        let w = |k: u32| words[((i * 7 + k * 13) % words.len() as u32) as usize];
        let text = match i % 4 {
            0 => format!("https://example{i}.com/{}/{}", w(1), w(2)),
            1 => format!("fn {}_{i}() {{\n    let {} = {};\n}}\n", w(1), w(2), i),
            _ => format!(
                "{} {} {} item {i} {}",
                w(1),
                w(2),
                w(3),
                w(4).repeat(1 + i as usize % 5)
            ),
        };
        let entry = ingest::text_entry(
            &text,
            NewEntry {
                at: t0 + i64::from(i),
                ..Default::default()
            },
        );
        state.clip_upsert(&entry).map_err(|e| e.to_string())?;
    }
    let queries = [
        ("", None),
        ("fire", None),
        ("invoice refund", None),
        ("://", None),
        ("", Some(vec![trowel_proto::ClipKind::Link])),
        ("deploy", Some(vec![trowel_proto::ClipKind::Code])),
    ];
    let mut times = Vec::new();
    for _ in 0..50 {
        for (text, kinds) in &queries {
            let started = Instant::now();
            state
                .clip_query(text, kinds.as_deref(), 50, 0)
                .map_err(|e| e.to_string())?;
            times.push(started.elapsed().as_secs_f64() * 1000.0);
        }
    }
    times.sort_by(|a, b| a.total_cmp(b));
    let (q50, q95) = (pct(&times, 0.5), pct(&times, 0.95));

    // An 8 MB image: the caller's screenshot, or incompressible bytes behind a PNG header.
    let bytes = match image {
        Some(path) => fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?,
        None => {
            let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
            b.extend(3840u32.to_be_bytes());
            b.extend(2160u32.to_be_bytes());
            let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
            while b.len() < 8 << 20 {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                b.extend(x.to_le_bytes());
            }
            b
        }
    };
    let spool = dir.join("spool");
    fs::create_dir_all(&spool).map_err(|e| e.to_string())?;
    let file = spool.join("0-0");
    fs::write(&file, &bytes).map_err(|e| e.to_string())?;
    let limits = Limits {
        max_entries: 1000,
        max_age_days: 30,
        max_blob_bytes: 512 << 20,
    };
    let started = Instant::now();
    let entry = ingest::build(
        &[Offer {
            mime: "image/png".into(),
            path: file,
        }],
        None,
        now(),
        64 << 20,
    )?
    .ok_or("image skipped")?;
    state.clip_upsert(&entry).map_err(|e| e.to_string())?;
    state
        .clip_enforce(limits, now())
        .map_err(|e| e.to_string())?;
    let ingest_ms = started.elapsed().as_secs_f64() * 1000.0;
    let (n, blob) = state.clip_stats().map_err(|e| e.to_string())?;
    println!(
        "{}",
        json!({
            "entries": n, "blob_bytes": blob,
            "query_ms": {"p50": q50, "p95": q95, "runs": times.len()},
            "image_bytes": bytes.len(), "ingest_ms": ingest_ms,
            "budget": {"query_p95_ms": 5.0, "ingest_ms": 150.0},
            "pass": q95 < 5.0 && ingest_ms < 150.0,
        })
    );
    let _ = fs::remove_dir_all(&dir);
    Ok(())
}
