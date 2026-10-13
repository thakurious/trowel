//! Real troweld: lock, hello, keyword search, pause. Never walks `$HOME`.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

static SEQ: AtomicU64 = AtomicU64::new(0);

struct Daemon {
    child: Child,
    socket: PathBuf,
    root: PathBuf,
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.socket);
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn temp() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "trowel-daemon-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn trowel_bin() -> PathBuf {
    let troweld = PathBuf::from(env!("CARGO_BIN_EXE_troweld"));
    let trowel = troweld.with_file_name("trowel");
    if !trowel.is_file() {
        let status = Command::new(env!("CARGO"))
            .args(["build", "-p", "trowel-cli", "--bin", "trowel", "--quiet"])
            .status()
            .expect("build trowel");
        assert!(status.success(), "cargo build -p trowel-cli failed");
    }
    assert!(
        trowel.is_file(),
        "trowel binary missing at {}",
        trowel.display()
    );
    trowel
}

fn write_cfg(root: &Path, onboarded: bool, roots: &[PathBuf]) {
    let cfg = trowel_proto::Config {
        roots: roots
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect(),
        onboarded,
        index_dir: root.join("index").to_string_lossy().into_owned(),
        ..Default::default()
    };
    troweld::config_file::save(&root.join("config.toml"), &cfg).unwrap();
}

fn start(root: PathBuf, extra: &[&str]) -> Daemon {
    let socket = root.join("troweld.sock");
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_troweld"));
    cmd.arg("--fake")
        .arg("--socket")
        .arg(&socket)
        .arg("--index")
        .arg(root.join("index"))
        .arg("--config")
        .arg(root.join("config.toml"))
        .args(extra)
        .env("XDG_RUNTIME_DIR", &root)
        .env("XDG_CONFIG_HOME", &root)
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("TROWEL", trowel_bin())
        .env("TROWEL_LINGER", "3")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let child = cmd.spawn().expect("spawn troweld");
    let mut daemon = Daemon {
        child,
        socket,
        root,
    };
    let deadline = Instant::now() + Duration::from_secs(4);
    while Instant::now() < deadline {
        if daemon.socket.exists() {
            return daemon;
        }
        thread::sleep(Duration::from_millis(10));
    }
    let mut err = String::new();
    if let Some(mut stderr) = daemon.child.stderr.take() {
        let _ = stderr.read_to_string(&mut err);
    }
    panic!(
        "troweld did not bind {} stderr={err}",
        daemon.socket.display()
    );
}

fn connect(d: &Daemon) -> UnixStream {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match UnixStream::connect(&d.socket) {
            Ok(s) => {
                s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                return s;
            }
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            Err(err) => panic!("connect: {err}"),
        }
    }
}

fn send(stream: &mut UnixStream, value: &Value) {
    writeln!(stream, "{value}").unwrap();
    stream.flush().unwrap();
}

fn recv(reader: &mut BufReader<UnixStream>) -> Value {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    serde_json::from_str(line.trim()).unwrap_or_else(|e| panic!("{line:?}: {e}"))
}

fn index_note(root: &Path) {
    let files = root.join("files");
    fs::create_dir_all(&files).unwrap();
    fs::write(files.join("notes.txt"), "the harbour meeting notes\n").unwrap();
    let store = trowel_core::IndexStore::open(root.join("index")).unwrap();
    let mut indexer = trowel_core::Indexer::new(
        store,
        trowel_core::IndexOptions::default(),
        Box::new(|| {
            Ok(Box::new(trowel_core::FakeEmbedder::new()) as Box<dyn trowel_core::Embedder>)
        }),
    )
    .unwrap();
    indexer.sync(&[files]).unwrap();
    indexer.run(None, None, &|| false, |_| {}).unwrap();
    indexer.finish();
}

#[test]
fn second_instance_exits_zero() {
    let root = temp();
    fs::write(
        root.join("config.toml"),
        "roots=[]\nexcluded_folders=[]\nexcluded_types=[]\nindex_dir=\"index\"\nmodels_dir=\"models\"\nhotkey=\"SUPER + D\"\nindex_on_battery=false\nencoder_idle_seconds=600\nonboarded=false\ncode_roots=[]\ncode_excluded=[]\ncode_editor=\"\"\ncode_quiet_seconds=2\n[ui]\nreduce_motion=false\n",
    )
    .unwrap();
    let d = start(root, &[]);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_troweld"));
    let status = cmd
        .arg("--fake")
        .arg("--socket")
        .arg(d.root.join("other.sock"))
        .arg("--index")
        .arg(d.root.join("index"))
        .arg("--config")
        .arg(d.root.join("config.toml"))
        .env("XDG_RUNTIME_DIR", &d.root)
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(0), "F11 second instance exits 0");
}

#[test]
fn hello_and_keyword_search() {
    let root = temp();
    fs::write(
        root.join("config.toml"),
        "roots=[]\nexcluded_folders=[]\nexcluded_types=[]\nindex_dir=\"index\"\nmodels_dir=\"models\"\nhotkey=\"SUPER + D\"\nindex_on_battery=false\nencoder_idle_seconds=600\nonboarded=false\ncode_roots=[]\ncode_excluded=[]\ncode_editor=\"\"\ncode_quiet_seconds=2\n[ui]\nreduce_motion=false\n",
    )
    .unwrap();
    index_note(&root);
    let d = start(root, &[]);
    let mut stream = connect(&d);
    send(
        &mut stream,
        &json!({"id":1,"op":"hello","protocol":1,"client":"test","pid":1}),
    );
    send(
        &mut stream,
        &json!({"id":2,"op":"search","text":"harbour","mode":"keyword","prefix_last":false,"gen":1}),
    );
    let mut reader = BufReader::new(stream);
    let hi = recv(&mut reader);
    assert_eq!(hi["ok"], true);
    assert_eq!(hi["protocol"], 1);
    let hit = recv(&mut reader);
    assert_eq!(hit["ok"], true, "{hit}");
    let groups = hit["groups"].as_array().cloned().unwrap_or_default();
    assert!(
        groups.iter().any(|g| {
            g["rows"].as_array().is_some_and(|rows| {
                rows.iter().any(|r| {
                    r["name"].as_str() == Some("notes.txt")
                        || r["id"].as_str().is_some_and(|p| p.ends_with("notes.txt"))
                })
            })
        }),
        "{hit}"
    );
}

#[test]
fn pause_sets_status() {
    let root = temp();
    fs::write(
        root.join("config.toml"),
        "roots=[]\nexcluded_folders=[]\nexcluded_types=[]\nindex_dir=\"index\"\nmodels_dir=\"models\"\nhotkey=\"SUPER + D\"\nindex_on_battery=false\nencoder_idle_seconds=600\nonboarded=false\ncode_roots=[]\ncode_excluded=[]\ncode_editor=\"\"\ncode_quiet_seconds=2\n[ui]\nreduce_motion=false\n",
    )
    .unwrap();
    let d = start(root, &[]);
    let mut stream = connect(&d);
    send(
        &mut stream,
        &json!({"id":1,"op":"hello","protocol":1,"client":"test","pid":1}),
    );
    send(&mut stream, &json!({"id":2,"op":"pause"}));
    send(&mut stream, &json!({"id":3,"op":"status"}));
    let mut reader = BufReader::new(stream);
    let hello = recv(&mut reader);
    assert_eq!(hello["ok"], true);
    let pause = recv(&mut reader);
    assert_eq!(pause["ok"], true);
    let status = recv(&mut reader);
    assert_eq!(status["phase"], "paused");
    assert_eq!(status["paused_reason"], "user");
}

fn groups_contain(hit: &Value, name: &str) -> bool {
    hit["groups"].as_array().is_some_and(|groups| {
        groups.iter().any(|g| {
            g["rows"].as_array().is_some_and(|rows| {
                rows.iter().any(|r| {
                    r["name"].as_str() == Some(name)
                        || r["id"].as_str().is_some_and(|p| p.ends_with(name))
                })
            })
        })
    })
}

#[test]
fn folder_change_indexes_and_search_finds_it() {
    let root = temp();
    let files = root.join("files");
    fs::create_dir_all(&files).unwrap();
    write_cfg(&root, true, std::slice::from_ref(&files));
    let d = start(root, &[]);
    let mut stream = connect(&d);
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    send(
        &mut stream,
        &json!({"id":1,"op":"hello","protocol":1,"client":"test","pid":1}),
    );
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let hello = recv(&mut reader);
    assert_eq!(hello["ok"], true, "{hello}");

    let wait_idle = Instant::now() + Duration::from_secs(10);
    let mut sid = 10u64;
    while Instant::now() < wait_idle {
        send(&mut stream, &json!({"id": sid, "op": "status"}));
        let st = loop {
            let v = recv(&mut reader);
            if v.get("id") == Some(&json!(sid)) {
                break v;
            }
        };
        if st["phase"].as_str() == Some("idle") {
            break;
        }
        sid += 1;
        thread::sleep(Duration::from_millis(50));
    }

    fs::write(files.join("notes.txt"), "the harbour meeting notes\n").unwrap();

    let deadline = Instant::now() + Duration::from_secs(20);
    let mut n = 100u64;
    let mut last = json!({});
    while Instant::now() < deadline {
        send(
            &mut stream,
            &json!({"id":n,"op":"search","text":"harbour","mode":"keyword","prefix_last":false,"gen":n}),
        );
        loop {
            last = recv(&mut reader);
            if last.get("id") == Some(&json!(n)) {
                break;
            }
        }
        if last["ok"] == true && groups_contain(&last, "notes.txt") {
            let segment_id = last["groups"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|g| g["rows"].as_array())
                .flatten()
                .find(|r| {
                    r["name"].as_str() == Some("notes.txt")
                        || r["id"].as_str().is_some_and(|p| p.ends_with("notes.txt"))
                })
                .and_then(|r| r["segment_id"].as_i64())
                .expect("segment_id");
            send(
                &mut stream,
                &json!({"id": n + 1, "op": "text_of_segment", "segment_id": segment_id}),
            );
            let text = recv(&mut reader);
            assert_eq!(text["ok"], true, "{text}");
            assert!(
                text["text"].as_str().is_some_and(|t| t.contains("harbour")),
                "{text}"
            );
            return;
        }
        n += 1;
        thread::sleep(Duration::from_millis(200));
    }
    let log = fs::read_to_string(d.root.join("index/indexer.log")).unwrap_or_default();
    panic!("new file was not searchable after folder change last={last} indexer.log={log}");
}

#[test]
fn resume_starts_helper_after_pause() {
    let root = temp();
    let files = root.join("files");
    fs::create_dir_all(&files).unwrap();
    write_cfg(&root, true, std::slice::from_ref(&files));
    let d = start(root, &[]);
    let mut stream = connect(&d);
    send(
        &mut stream,
        &json!({"id":1,"op":"hello","protocol":1,"client":"test","pid":1}),
    );
    send(&mut stream, &json!({"id":2,"op":"pause"}));
    send(&mut stream, &json!({"id":3,"op":"resume"}));
    send(&mut stream, &json!({"id":4,"op":"status"}));
    let mut reader = BufReader::new(stream);
    assert_eq!(recv(&mut reader)["ok"], true);
    assert_eq!(recv(&mut reader)["ok"], true);
    assert_eq!(recv(&mut reader)["ok"], true);
    let status = recv(&mut reader);
    assert_ne!(status["phase"], "paused", "{status}");
    assert!(status["paused_reason"].is_null(), "{status}");
}

#[test]
fn idle_releases_vectors() {
    let root = temp();
    let files = root.join("files");
    fs::create_dir_all(&files).unwrap();
    fs::write(files.join("notes.txt"), "harbour\n").unwrap();
    let cfg = trowel_proto::Config {
        roots: vec![files.to_string_lossy().into_owned()],
        onboarded: false,
        index_dir: root.join("index").to_string_lossy().into_owned(),
        encoder_idle_seconds: 1,
        ..Default::default()
    };
    troweld::config_file::save(&root.join("config.toml"), &cfg).unwrap();
    index_note(&root);
    let d = start(root, &[]);
    let mut stream = connect(&d);
    send(
        &mut stream,
        &json!({"id":1,"op":"hello","protocol":1,"client":"test","pid":1}),
    );
    send(&mut stream, &json!({"id":2,"op":"panel_opened"}));
    send(&mut stream, &json!({"id":3,"op":"status"}));
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    assert_eq!(recv(&mut reader)["ok"], true);
    assert_eq!(recv(&mut reader)["ok"], true);
    let warm = recv(&mut reader);
    assert_eq!(warm["vectors_loaded"], true, "{warm}");
    thread::sleep(Duration::from_millis(1300));
    send(&mut stream, &json!({"id":4,"op":"status"}));
    let idle = loop {
        let v = recv(&mut reader);
        if v.get("id") == Some(&json!(4)) {
            break v;
        }
    };
    assert_eq!(idle["vectors_loaded"], false, "{idle}");
    let rss_kb = proc_rss_kb(d.child.id());
    assert!(rss_kb <= 120 * 1024, "idle RSS {rss_kb} kB exceeds 120 MB");
}

fn proc_rss_kb(pid: u32) -> u64 {
    fs::read_to_string(format!("/proc/{pid}/status"))
        .unwrap_or_default()
        .lines()
        .find(|l| l.starts_with("VmRSS:"))
        .and_then(|l| l.split_whitespace().nth(1))
        .and_then(|s| s.parse().ok())
        .unwrap_or(u64::MAX)
}

fn wait_idle(stream: &mut UnixStream, reader: &mut BufReader<UnixStream>, mut sid: u64) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        send(stream, &json!({"id": sid, "op": "status"}));
        let st = loop {
            let v = recv(reader);
            if v.get("id") == Some(&json!(sid)) {
                break v;
            }
        };
        if st["phase"].as_str() == Some("idle") {
            return;
        }
        sid += 1;
        thread::sleep(Duration::from_millis(50));
    }
}

fn search_until(stream: &mut UnixStream, reader: &mut BufReader<UnixStream>, q: &str, name: &str) {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut n = 200u64;
    let mut last = json!({});
    while Instant::now() < deadline {
        send(
            stream,
            &json!({"id":n,"op":"search","text":q,"mode":"keyword","prefix_last":false,"gen":n}),
        );
        last = loop {
            let v = recv(reader);
            if v.get("id") == Some(&json!(n)) {
                break v;
            }
        };
        if last["ok"] == true && groups_contain(&last, name) {
            return;
        }
        n += 1;
        thread::sleep(Duration::from_millis(200));
    }
    panic!("search {q:?} did not find {name}: {last}");
}

#[test]
fn overflow_signal_rescans() {
    let root = temp();
    let files = root.join("files");
    fs::create_dir_all(&files).unwrap();
    write_cfg(&root, true, std::slice::from_ref(&files));
    let d = start(root, &[]);
    let pid = d.child.id();
    let mut stream = connect(&d);
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    send(
        &mut stream,
        &json!({"id":1,"op":"hello","protocol":1,"client":"test","pid":1}),
    );
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    assert_eq!(recv(&mut reader)["ok"], true);
    wait_idle(&mut stream, &mut reader, 10);
    fs::write(files.join("overflow.txt"), "the overflow harbour note\n").unwrap();
    Command::new("kill")
        .args(["-USR1", &pid.to_string()])
        .status()
        .unwrap();
    search_until(&mut stream, &mut reader, "overflow harbour", "overflow.txt");
}

#[test]
fn suspend_resume_signal_syncs() {
    let root = temp();
    let files = root.join("files");
    fs::create_dir_all(&files).unwrap();
    write_cfg(&root, true, std::slice::from_ref(&files));
    let d = start(root, &[]);
    let pid = d.child.id();
    let mut stream = connect(&d);
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    send(
        &mut stream,
        &json!({"id":1,"op":"hello","protocol":1,"client":"test","pid":1}),
    );
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    assert_eq!(recv(&mut reader)["ok"], true);
    wait_idle(&mut stream, &mut reader, 10);
    Command::new("kill")
        .args(["-USR2", &pid.to_string()])
        .status()
        .unwrap();
    thread::sleep(Duration::from_millis(400));
    fs::write(files.join("woke.txt"), "the resumed harbour dock\n").unwrap();
    Command::new("kill")
        .args(["-USR2", &pid.to_string()])
        .status()
        .unwrap();
    search_until(&mut stream, &mut reader, "resumed harbour", "woke.txt");
}

#[test]
fn estimate_and_status_subscribe() {
    let root = temp();
    let files = root.join("files");
    fs::create_dir_all(&files).unwrap();
    fs::write(files.join("notes.txt"), "harbour\n").unwrap();
    write_cfg(&root, false, &[]);
    let d = start(root, &[]);
    let mut stream = connect(&d);
    send(
        &mut stream,
        &json!({"id":1,"op":"hello","protocol":1,"client":"test","pid":1}),
    );
    send(
        &mut stream,
        &json!({"id":2,"op":"subscribe","topics":["status"]}),
    );
    send(
        &mut stream,
        &json!({"id":3,"op":"estimate","paths":[files], "code": false}),
    );
    send(&mut stream, &json!({"id":4,"op":"model_status"}));
    send(&mut stream, &json!({"id":5,"op":"doctor"}));
    let mut reader = BufReader::new(stream);
    assert_eq!(recv(&mut reader)["ok"], true);
    let mut saw_status = false;
    let mut estimate = None;
    let mut model = None;
    let mut doctor = None;
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && (estimate.is_none() || model.is_none() || doctor.is_none()) {
        let v = recv(&mut reader);
        if v.get("event") == Some(&json!("status")) {
            saw_status = true;
        }
        match v.get("id").and_then(Value::as_u64) {
            Some(3) => estimate = Some(v),
            Some(4) => model = Some(v),
            Some(5) => doctor = Some(v),
            _ => {}
        }
    }
    assert!(saw_status, "subscribe status snapshot");
    let estimate = estimate.expect("estimate");
    assert_eq!(estimate["ok"], true, "{estimate}");
    assert!(
        estimate["per_path"][0]["files"].as_u64().unwrap_or(0) >= 1,
        "{estimate}"
    );
    assert_eq!(model.expect("model")["ok"], true);
    let doctor = doctor.expect("doctor");
    assert_eq!(doctor["ok"], true, "{doctor}");
    let checks = doctor["checks"].as_array().cloned().unwrap_or_default();
    // Daemon/tools/disk/index must be healthy in this fixture. model/gpu/worker
    // FAIL or WARN on CI (no GGUFs, no vulkan-tools, no trowel-worker).
    for name in ["daemon", "tools", "disk", "index"] {
        let check = checks
            .iter()
            .find(|c| c["name"] == name)
            .unwrap_or_else(|| panic!("missing {name} check in {checks:?}"));
        assert_ne!(check["status"], "FAIL", "{check}");
    }
}
