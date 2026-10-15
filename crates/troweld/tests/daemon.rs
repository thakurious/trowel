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
    start_with(root, extra, &[])
}

/// `envs` override the daemon's environment (e.g. a `PATH` with stub `wl-copy`).
fn start_with(root: PathBuf, extra: &[&str], envs: &[(&str, &str)]) -> Daemon {
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
        // Never the developer's session: clipboard writes and keystrokes must fail in tests.
        .env("WAYLAND_DISPLAY", "trowel-test-no-compositor")
        .arg("--state-dir")
        .arg(root.join("state"))
        .envs(envs.iter().copied())
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

fn index_code_file(root: &Path) {
    let files = root.join("src");
    fs::create_dir_all(&files).unwrap();
    fs::write(files.join("RouteValidator.rs"), "fn route_validator() {}\n").unwrap();
    let store =
        trowel_core::IndexStore::open_named(root.join("index"), trowel_core::IndexStore::CODE_FILE)
            .unwrap();
    let mut indexer = trowel_core::Indexer::new(
        store,
        trowel_core::IndexOptions {
            code: true,
            ..trowel_core::IndexOptions::default()
        },
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
        &json!({"id":1,"op":"hello","protocol":2,"client":"test","pid":1}),
    );
    send(
        &mut stream,
        &json!({"id":2,"op":"search","text":"harbour","mode":"keyword","prefix_last":false,"gen":1}),
    );
    let mut reader = BufReader::new(stream);
    let hi = recv(&mut reader);
    assert_eq!(hi["ok"], true);
    assert_eq!(hi["protocol"], 2);
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
fn protocol_v2_handshake_and_unbuilt_ops() {
    let root = temp();
    let d = start(root, &[]);

    // A v1 client (an old panel left running across an upgrade) is told to restart.
    let mut old = connect(&d);
    send(
        &mut old,
        &json!({"id":1,"op":"hello","protocol":1,"client":"test","pid":1}),
    );
    let refused = recv(&mut BufReader::new(old));
    assert_eq!(refused["ok"], false, "{refused}");
    assert_eq!(refused["error"]["code"], "bad_request");
    assert_eq!(
        refused["error"]["message"], "Trowel was updated, restart it",
        "{refused}"
    );

    let mut stream = connect(&d);
    send(
        &mut stream,
        &json!({"id":1,"op":"hello","protocol":2,"client":"test","pid":1}),
    );
    send(
        &mut stream,
        &json!({"id":2,"op":"apps_query","text":"fire","gen":1}),
    );
    send(&mut stream, &json!({"id":3,"op":"clip_get","clip_id":1}));
    let mut reader = BufReader::new(stream);
    let hi = recv(&mut reader);
    assert_eq!(hi["protocol"], 2, "{hi}");
    let caps = hi["capabilities"].as_array().unwrap();
    assert!(
        !caps.iter().any(|c| c == "apps" || c == "clipboard"),
        "not advertised before L2/L3: {hi}"
    );
    for id in [2, 3] {
        let reply = recv(&mut reader);
        assert_eq!(reply["id"], id, "{reply}");
        assert_eq!(reply["error"]["code"], "not_ready", "{reply}");
    }
}

#[test]
fn calc_copy_and_history() {
    let root = temp();
    // Stubs record what would have reached the clipboard and the focused window.
    let bin = root.join("bin");
    fs::create_dir_all(&bin).unwrap();
    for (name, body) in [
        (
            "wl-copy",
            format!("#!/bin/sh\ncat > '{}'\n", root.join("clip.txt").display()),
        ),
        (
            "wtype",
            format!(
                "#!/bin/sh\necho \"$@\" > '{}'\n",
                root.join("keys.txt").display()
            ),
        ),
    ] {
        let path = bin.join(name);
        fs::write(&path, body).unwrap();
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let d = start_with(root.clone(), &[], &[("PATH", path.as_str())]);
    let mut stream = connect(&d);
    for msg in [
        json!({"id":1,"op":"hello","protocol":2,"client":"test","pid":1}),
        json!({"id":2,"op":"calc","text":"5 ft to cm","gen":7}),
        json!({"id":3,"op":"calc","text":"firefox","gen":8}),
        json!({"id":4,"op":"calc","text":"=1 +","force":true,"gen":9}),
        json!({"id":5,"op":"calc_copy","text":"152.4","expr":"5 ft to cm","mode":"paste"}),
        json!({"id":6,"op":"calc_history","limit":5}),
    ] {
        send(&mut stream, &msg);
    }
    let mut reader = BufReader::new(stream);
    assert_eq!(recv(&mut reader)["ok"], true);
    let card = recv(&mut reader);
    assert_eq!(card["gen"], 7, "{card}");
    assert_eq!(card["result"]["copy"], "152.4", "{card}");
    assert_eq!(card["result"]["display"], "152.4 cm", "{card}");
    assert!(card["took_us"].as_u64().unwrap() < 20_000, "{card}");
    let none = recv(&mut reader);
    assert!(
        none.get("result").is_none(),
        "firefox is not a calculation: {none}"
    );
    let forced = recv(&mut reader);
    assert!(forced["error"].is_string(), "{forced}");
    let copied = recv(&mut reader);
    assert_eq!(copied["ok"], true, "{copied}");
    let history = recv(&mut reader);
    assert_eq!(history["rows"][0]["expr"], "5 ft to cm", "{history}");
    assert_eq!(history["rows"][0]["result"], "152.4", "{history}");
    assert_eq!(fs::read_to_string(root.join("clip.txt")).unwrap(), "152.4");
    // Paste mode sends Omarchy's keystroke after the delay.
    let deadline = Instant::now() + Duration::from_secs(2);
    while !root.join("keys.txt").exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        fs::read_to_string(root.join("keys.txt")).unwrap().trim(),
        "-M shift -k Insert -m shift"
    );
    assert!(root.join("state/state.sqlite").is_file());
}

#[test]
fn calc_copy_without_a_compositor_is_an_error_and_records_nothing() {
    let root = temp();
    let d = start(root.clone(), &[]);
    let mut stream = connect(&d);
    send(
        &mut stream,
        &json!({"id":1,"op":"hello","protocol":2,"client":"test","pid":1}),
    );
    send(
        &mut stream,
        &json!({"id":2,"op":"calc_copy","text":"2","expr":"1+1","mode":"copy"}),
    );
    send(&mut stream, &json!({"id":3,"op":"calc_history"}));
    let mut reader = BufReader::new(stream);
    recv(&mut reader);
    let copied = recv(&mut reader);
    assert_eq!(copied["error"]["code"], "io", "{copied}");
    let history = recv(&mut reader);
    assert_eq!(history["rows"], json!([]), "{history}");
}

#[test]
fn code_search_uses_code_index_and_preview() {
    let root = temp();
    fs::write(
        root.join("config.toml"),
        "roots=[]\nexcluded_folders=[]\nexcluded_types=[]\nindex_dir=\"index\"\nmodels_dir=\"models\"\nhotkey=\"SUPER + D\"\nindex_on_battery=false\nencoder_idle_seconds=600\nonboarded=false\ncode_roots=[]\ncode_excluded=[]\ncode_editor=\"\"\ncode_quiet_seconds=2\n[ui]\nreduce_motion=false\n",
    )
    .unwrap();
    index_note(&root);
    index_code_file(&root);
    let d = start(root, &[]);
    let mut stream = connect(&d);
    send(
        &mut stream,
        &json!({"id":1,"op":"hello","protocol":2,"client":"test","pid":1}),
    );
    send(
        &mut stream,
        &json!({"id":2,"op":"search","text":"route_validator","mode":"keyword","code":true,"prefix_last":true,"gen":1}),
    );
    let mut reader = BufReader::new(stream);
    let hi = recv(&mut reader);
    assert_eq!(hi["ok"], true);
    let hit = recv(&mut reader);
    assert_eq!(hit["ok"], true, "{hit}");
    let groups = hit["groups"].as_array().cloned().unwrap_or_default();
    let mut segment_id = None;
    let mut saw_code = false;
    for g in &groups {
        for row in g["rows"].as_array().cloned().unwrap_or_default() {
            let name = row["name"].as_str().unwrap_or("");
            let path = row["id"].as_str().unwrap_or("");
            assert!(
                !name.ends_with("notes.txt") && !path.ends_with("notes.txt"),
                "code search leaked the document index: {hit}"
            );
            if name == "RouteValidator.rs" || path.ends_with("RouteValidator.rs") {
                saw_code = true;
                segment_id = row["segment_id"].as_i64();
            }
        }
    }
    assert!(saw_code, "expected RouteValidator.rs in code search: {hit}");
    let sid = segment_id.expect("segment_id");
    send(
        reader.get_mut(),
        &json!({"id":3,"op":"preview","segment_id":sid,"width":450,"code":true}),
    );
    let preview = recv(&mut reader);
    assert_eq!(preview["ok"], true, "{preview}");
    let ptype = preview["type"].as_str().unwrap_or("");
    assert!(
        ptype == "code" || ptype == "text",
        "preview type {ptype}: {preview}"
    );
    let has_body = preview["text"]
        .as_str()
        .is_some_and(|t| t.contains("route_validator"))
        || preview["lines"].as_array().is_some_and(|lines| {
            lines.iter().any(|line| {
                line["text"]
                    .as_str()
                    .is_some_and(|t| t.contains("route_validator"))
            })
        });
    assert!(has_body, "{preview}");
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
        &json!({"id":1,"op":"hello","protocol":2,"client":"test","pid":1}),
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
        &json!({"id":1,"op":"hello","protocol":2,"client":"test","pid":1}),
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
        &json!({"id":1,"op":"hello","protocol":2,"client":"test","pid":1}),
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
        &json!({"id":1,"op":"hello","protocol":2,"client":"test","pid":1}),
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
        &json!({"id":1,"op":"hello","protocol":2,"client":"test","pid":1}),
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
        &json!({"id":1,"op":"hello","protocol":2,"client":"test","pid":1}),
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
        &json!({"id":1,"op":"hello","protocol":2,"client":"test","pid":1}),
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

/// Asks until a keyword search for `word` finds a file, or `secs` pass.
fn finds(
    stream: &mut UnixStream,
    reader: &mut BufReader<UnixStream>,
    word: &str,
    secs: u64,
    id: &mut u64,
) -> bool {
    let deadline = Instant::now() + Duration::from_secs(secs);
    while Instant::now() < deadline {
        *id += 1;
        let n = *id;
        send(
            stream,
            &json!({"id":n,"op":"search","text":word,"mode":"keyword","prefix_last":false,"gen":n}),
        );
        let reply = loop {
            let v = recv(reader);
            if v.get("id") == Some(&json!(n)) {
                break v;
            }
        };
        let rows = reply["groups"]
            .as_array()
            .map(|g| {
                g.iter()
                    .map(|g| g["rows"].as_array().map_or(0, Vec::len))
                    .sum::<usize>()
            })
            .unwrap_or(0);
        if rows > 0 {
            return true;
        }
        thread::sleep(Duration::from_millis(150));
    }
    false
}

/// Pause stops the helper; Resume must start a new one that indexes what changed meanwhile
/// (live: after pause → helper exit → resume, no helper ever came back).
#[test]
fn resume_indexes_again_after_the_helper_stopped() {
    let root = temp();
    let files = root.join("files");
    fs::create_dir_all(&files).unwrap();
    fs::write(files.join("a.txt"), "the harbour meeting notes\n").unwrap();
    write_cfg(&root, true, std::slice::from_ref(&files));
    let d = start(root, &[]);
    let mut stream = connect(&d);
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    send(
        &mut stream,
        &json!({"id":1,"op":"hello","protocol":2,"client":"test","pid":1}),
    );
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    assert_eq!(recv(&mut reader)["ok"], true);
    let mut id = 10;
    assert!(
        finds(&mut stream, &mut reader, "harbour", 20, &mut id),
        "first pass"
    );
    send(&mut stream, &json!({"id":2,"op":"pause"}));
    // The helper stops between files and exits.
    thread::sleep(Duration::from_millis(2500));
    fs::write(files.join("b.txt"), "a zebra crossing near the station\n").unwrap();
    thread::sleep(Duration::from_millis(1200));
    send(&mut stream, &json!({"id":3,"op":"resume"}));
    assert!(
        finds(&mut stream, &mut reader, "zebra", 20, &mut id),
        "a helper must index again after resume"
    );
}

/// The helper must outlive the connection that asked for it: `PR_SET_PDEATHSIG` is per thread,
/// and a helper forked on a client's thread died when that client hung up.
#[test]
fn helper_outlives_the_connection_that_resumed_it() {
    let root = temp();
    let files = root.join("files");
    fs::create_dir_all(&files).unwrap();
    fs::write(files.join("a.txt"), "the harbour meeting notes\n").unwrap();
    write_cfg(&root, true, std::slice::from_ref(&files));
    let d = start(root, &[]);
    {
        let mut stream = connect(&d);
        stream
            .set_read_timeout(Some(Duration::from_secs(8)))
            .unwrap();
        send(
            &mut stream,
            &json!({"id":1,"op":"hello","protocol":2,"client":"test","pid":1}),
        );
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        assert_eq!(recv(&mut reader)["ok"], true);
        let mut id = 10;
        assert!(
            finds(&mut stream, &mut reader, "harbour", 20, &mut id),
            "first pass"
        );
        send(&mut stream, &json!({"id":2,"op":"pause"}));
        assert_eq!(recv(&mut reader)["ok"], true);
    }
    thread::sleep(Duration::from_millis(2500));
    // Nothing changed meanwhile: no watcher sync can start a helper from a lasting thread.
    {
        // A one-shot client, like `trowel resume`: it hangs up right after the reply.
        let mut stream = connect(&d);
        stream
            .set_read_timeout(Some(Duration::from_secs(8)))
            .unwrap();
        send(
            &mut stream,
            &json!({"id":1,"op":"hello","protocol":2,"client":"cli","pid":1}),
        );
        send(&mut stream, &json!({"id":2,"op":"resume"}));
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        assert_eq!(recv(&mut reader)["ok"], true);
        assert_eq!(recv(&mut reader)["ok"], true);
    }
    // A helper forked on the client's thread got SIGTERM when that thread ended.
    thread::sleep(Duration::from_millis(1500));
    let mut d = d;
    let _ = d.child.kill();
    let _ = d.child.wait();
    let mut err = String::new();
    if let Some(mut stderr) = d.child.stderr.take() {
        let _ = stderr.read_to_string(&mut err);
    }
    assert!(
        !err.contains("helper ended by signal 15"),
        "the helper must outlive the connection that resumed it:\n{err}"
    );
}

/// A kind tab filters the list, but the tab counts are the query's unfiltered strong results.
/// `upstream/Sources/DigUpApp/SearchWindow.swift:311`.
#[test]
fn kind_filter_keeps_unfiltered_tab_counts() {
    let root = temp();
    let files = root.join("files");
    fs::create_dir_all(&files).unwrap();
    fs::write(files.join("a.txt"), "the harbour meeting notes\n").unwrap();
    write_cfg(&root, true, std::slice::from_ref(&files));
    let d = start(root, &[]);
    let mut stream = connect(&d);
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    send(
        &mut stream,
        &json!({"id":1,"op":"hello","protocol":2,"client":"test","pid":1}),
    );
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    assert_eq!(recv(&mut reader)["ok"], true);
    let mut id = 10;
    assert!(finds(&mut stream, &mut reader, "harbour", 20, &mut id));
    let ask = |stream: &mut UnixStream,
               reader: &mut BufReader<UnixStream>,
               n: u64,
               kinds: Value| {
        send(
            stream,
            &json!({"id":n,"op":"search","text":"harbour","mode":"keyword","prefix_last":false,"gen":n,"kinds":kinds}),
        );
        loop {
            let v = recv(reader);
            if v.get("id") == Some(&json!(n)) {
                return v;
            }
        }
    };
    let all = ask(&mut stream, &mut reader, 100, Value::Null);
    assert_eq!(all["counts"]["documents"], 1, "{all}");
    let images = ask(&mut stream, &mut reader, 101, json!(["image"]));
    assert_eq!(
        images["groups"].as_array().map(Vec::len),
        Some(0),
        "{images}"
    );
    assert_eq!(images["counts"]["documents"], 1, "{images}");
}

/// A code sync is the code pass's progress. It used to set `main` to 0/N, so the panel badge
/// and the bar read "Indexing 0%" for the whole code pass.
#[test]
fn code_sync_progress_is_not_main_progress() {
    let root = temp();
    let code = root.join("code");
    fs::create_dir_all(code.join("src")).unwrap();
    fs::write(
        code.join("src/route_validator.rs"),
        "pub fn route_validator(path: &str) -> bool {\n    path.starts_with('/')\n}\n",
    )
    .unwrap();
    let cfg = trowel_proto::Config {
        roots: vec![],
        code_roots: vec![code.to_string_lossy().into_owned()],
        onboarded: true,
        index_dir: root.join("index").to_string_lossy().into_owned(),
        ..Default::default()
    };
    troweld::config_file::save(&root.join("config.toml"), &cfg).unwrap();
    let d = start(root, &[]);
    let mut stream = connect(&d);
    stream
        .set_read_timeout(Some(Duration::from_secs(8)))
        .unwrap();
    send(
        &mut stream,
        &json!({"id":1,"op":"hello","protocol":2,"client":"test","pid":1}),
    );
    send(
        &mut stream,
        &json!({"id":2,"op":"subscribe","topics":["status"]}),
    );
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut statuses = 0;
    let mut n = 100u64;
    let mut found = false;
    while Instant::now() < deadline && !found {
        n += 1;
        send(
            &mut stream,
            &json!({"id":n,"op":"search","text":"route_validator","mode":"keyword","code":true,"prefix_last":false,"gen":n}),
        );
        loop {
            let v = recv(&mut reader);
            if v["event"] == "status" {
                statuses += 1;
                assert_eq!(
                    v["main"]["total"], 0,
                    "code indexing reported as main progress: {v}"
                );
                continue;
            }
            if v.get("id") == Some(&json!(n)) {
                found = v["groups"].as_array().is_some_and(|g| !g.is_empty());
                break;
            }
        }
        thread::sleep(Duration::from_millis(150));
    }
    assert!(found, "the code file must be indexed");
    assert!(statuses > 0, "no status events seen");
}
