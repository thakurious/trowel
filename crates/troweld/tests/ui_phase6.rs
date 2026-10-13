//! Phase 6 PROTOCOL checks against `--mock`: two-phase search, weaker fold,
//! place badges, F18 stall. Never opens a desktop window.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

static SEQ: AtomicU64 = AtomicU64::new(0);

struct Mock {
    child: Child,
    socket: PathBuf,
    dir: PathBuf,
}

impl Drop for Mock {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.socket);
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn start() -> Mock {
    let dir = std::env::temp_dir().join(format!(
        "trowel-ui6-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&dir).unwrap();
    let socket = dir.join("troweld.sock");
    let child = Command::new(env!("CARGO_BIN_EXE_troweld"))
        .args(["--mock", "--socket"])
        .arg(&socket)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn mock");
    let mock = Mock { child, socket, dir };
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if UnixStream::connect(&mock.socket).is_ok() {
            return mock;
        }
        std::thread::sleep(Duration::from_millis(15));
    }
    panic!("mock socket never appeared");
}

fn connect(m: &Mock) -> UnixStream {
    let s = UnixStream::connect(&m.socket).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    s.set_write_timeout(Some(Duration::from_millis(400)))
        .unwrap();
    s
}

fn send(s: &mut UnixStream, v: &Value) {
    writeln!(s, "{v}").unwrap();
    s.flush().unwrap();
}

fn recv(r: &mut BufReader<UnixStream>) -> Value {
    let mut line = String::new();
    r.read_line(&mut line).unwrap();
    serde_json::from_str(line.trim()).unwrap()
}

fn rpc(s: &mut UnixStream, r: &mut BufReader<UnixStream>, v: Value) -> Value {
    send(s, &v);
    loop {
        let msg = recv(r);
        if msg.get("event").is_none() {
            return msg;
        }
    }
}

#[test]
fn two_phase_keyword_then_hybrid_has_groups_and_place() {
    let m = start();
    let mut s = connect(&m);
    let mut r = BufReader::new(s.try_clone().unwrap());
    let hello = rpc(
        &mut s,
        &mut r,
        json!({"id":1,"op":"hello","protocol":1,"client":"test","pid":1}),
    );
    assert_eq!(hello["ok"], true);
    rpc(&mut s, &mut r, json!({"id":2,"op":"panel_opened"}));
    let kw = rpc(
        &mut s,
        &mut r,
        json!({"id":3,"op":"search","text":"refund","mode":"keyword","prefix_last":true,"gen":1}),
    );
    assert_eq!(kw["ok"], true, "{kw}");
    assert_eq!(kw["gen"], 1);
    assert_eq!(kw["groups"][0]["rows"][0]["place"], "p. 3");
    assert_eq!(kw["groups"][0]["rows"][0]["reason"], "Similar meaning");
    let hy = rpc(
        &mut s,
        &mut r,
        json!({"id":4,"op":"search","text":"zebra","mode":"hybrid","prefix_last":false,"gen":2}),
    );
    assert_eq!(hy["ok"], true, "{hy}");
    assert_eq!(hy["groups"][0]["rows"][0]["place"], "0:06");
    assert_eq!(hy["weaker"].as_array().unwrap().len(), 1);
    let code = rpc(
        &mut s,
        &mut r,
        json!({"id":5,"op":"search","text":"retry","mode":"hybrid","code":true,"gen":3}),
    );
    assert_eq!(code["groups"][0]["rows"][0]["place"], "L12–40");
}

#[test]
fn f18_stop_then_cont_leaves_client_able_to_search() {
    let mut m = start();
    let pid = m.child.id();
    let mut s = connect(&m);
    let mut r = BufReader::new(s.try_clone().unwrap());
    rpc(
        &mut s,
        &mut r,
        json!({"id":1,"op":"hello","protocol":1,"client":"test","pid":1}),
    );
    Command::new("kill")
        .args(["-STOP", &pid.to_string()])
        .status()
        .unwrap();
    s.set_read_timeout(Some(Duration::from_millis(400)))
        .unwrap();
    send(
        &mut s,
        &json!({"id":2,"op":"search","text":"refund","mode":"keyword","gen":1}),
    );
    let mut line = String::new();
    let stalled = r.read_line(&mut line).is_err() || line.is_empty();
    assert!(stalled, "SIGSTOP should stall the reply, got {line:?}");
    Command::new("kill")
        .args(["-CONT", &pid.to_string()])
        .status()
        .unwrap();
    // Drop the blocked connection and open a fresh one after CONT.
    drop(r);
    drop(s);
    std::thread::sleep(Duration::from_millis(50));
    let mut s = connect(&m);
    s.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    let mut r = BufReader::new(s.try_clone().unwrap());
    let hello = rpc(
        &mut s,
        &mut r,
        json!({"id":1,"op":"hello","protocol":1,"client":"test","pid":1}),
    );
    assert_eq!(hello["ok"], true);
    let kw = rpc(
        &mut s,
        &mut r,
        json!({"id":2,"op":"search","text":"refund","mode":"keyword","gen":1}),
    );
    assert_eq!(kw["ok"], true, "{kw}");
    let _ = m.child.kill();
}

#[test]
fn kill_mock_connect_fails_within_two_seconds() {
    let mut m = start();
    let sock = m.socket.clone();
    m.child.kill().unwrap();
    m.child.wait().unwrap();
    let t0 = Instant::now();
    let err = UnixStream::connect(&sock);
    assert!(err.is_err());
    assert!(t0.elapsed() < Duration::from_secs(2));
}
