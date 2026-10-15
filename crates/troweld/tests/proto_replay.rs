//! Replay `tests/proto/*.jsonl` against `troweld --mock`.

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::Value;
use trowel_proto::MAX_LINE_BYTES;

static SOCK_SEQ: AtomicU64 = AtomicU64::new(0);

struct Mock {
    child: Child,
    socket: PathBuf,
}

impl Drop for Mock {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = fs::remove_file(&self.socket);
    }
}

fn proto_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/proto")
}

fn start_mock(latency_ms: u64, fixture: Option<&Path>) -> Mock {
    let socket = std::env::temp_dir().join(format!(
        "trowel-mock-{}-{}.sock",
        std::process::id(),
        SOCK_SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_file(&socket);
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_troweld"));
    cmd.arg("--mock")
        .arg("--socket")
        .arg(&socket)
        .arg("--latency-ms")
        .arg(latency_ms.to_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    if let Some(path) = fixture {
        cmd.arg("--fixture").arg(path);
    }
    let child = cmd.spawn().expect("spawn troweld --mock");
    let mut mock = Mock { child, socket };
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if mock.socket.exists() {
            return mock;
        }
        thread::sleep(Duration::from_millis(5));
    }
    let mut err = String::new();
    if let Some(mut stderr) = mock.child.stderr.take() {
        let _ = stderr.read_to_string(&mut err);
    }
    panic!(
        "troweld --mock did not bind {} stderr={err}",
        mock.socket.display()
    );
}

fn connect(mock: &Mock) -> UnixStream {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match UnixStream::connect(&mock.socket) {
            Ok(s) => {
                s.set_read_timeout(Some(Duration::from_secs(5)))
                    .expect("read timeout");
                s.set_write_timeout(Some(Duration::from_secs(5)))
                    .expect("write timeout");
                return s;
            }
            Err(_) if Instant::now() < deadline => thread::sleep(Duration::from_millis(5)),
            Err(err) => panic!("connect {}: {err}", mock.socket.display()),
        }
    }
}

fn send(stream: &mut UnixStream, value: &Value) {
    writeln!(stream, "{value}").expect("send");
    stream.flush().expect("flush");
}

fn recv(reader: &mut BufReader<UnixStream>) -> Value {
    let mut line = String::new();
    let n = reader.read_line(&mut line).expect("recv");
    assert!(n > 0, "daemon closed the connection");
    serde_json::from_str(line.trim()).unwrap_or_else(|err| panic!("invalid JSON {line:?}: {err}"))
}

fn json_match(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::String(s), _) if s == "*" => true,
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::Object(e), Value::Object(a)) => {
            e.len() == a.len()
                && e.iter()
                    .all(|(k, v)| a.get(k).is_some_and(|got| json_match(v, got)))
        }
        (Value::Array(e), Value::Array(a)) => {
            e.len() == a.len() && e.iter().zip(a).all(|(l, r)| json_match(l, r))
        }
        (l, r) => l == r,
    }
}

fn replay_file(path: &Path) {
    let raw = fs::read_to_string(path).unwrap();
    let mut latency_ms = 0u64;
    let mut fixture: Option<PathBuf> = None;
    let steps: Vec<Value> = raw
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{}: {e}", path.display())))
        .collect();
    if let Some(Value::Object(obj)) = steps.first() {
        if let Some(server) = obj.get("server") {
            if let Some(ms) = server.get("latency_ms").and_then(Value::as_u64) {
                latency_ms = ms;
            }
            if let Some(f) = server.get("fixture").and_then(Value::as_str) {
                fixture = Some(proto_dir().join(f));
            }
        }
    }
    let mock = start_mock(latency_ms, fixture.as_deref());
    let stream = connect(&mock);
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut writer = stream;
    for (i, step) in steps.iter().enumerate() {
        if step.get("server").is_some() {
            continue;
        }
        if let Some(ms) = step.get("wait_ms").and_then(Value::as_u64) {
            thread::sleep(Duration::from_millis(ms));
            continue;
        }
        if let Some(msg) = step.get("send") {
            send(&mut writer, msg);
            continue;
        }
        if let Some(expected) = step.get("recv") {
            let actual = recv(&mut reader);
            assert!(
                json_match(expected, &actual),
                "{} step {i}: expected {expected} got {actual}",
                path.display()
            );
            continue;
        }
        panic!("{} step {i}: unknown directive {step}", path.display());
    }
}

#[test]
fn golden_transcripts() {
    let dir = proto_dir();
    let mut files: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("jsonl"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no transcripts in {}", dir.display());
    for path in files {
        replay_file(&path);
    }
}

#[test]
fn too_large_closes_the_connection() {
    let mock = start_mock(0, None);
    let mut stream = connect(&mock);
    let mut oversized = vec![b'x'; MAX_LINE_BYTES + 1];
    oversized.push(b'\n');
    stream.write_all(&oversized).expect("write oversized");
    stream.flush().unwrap();
    let mut reader = BufReader::new(stream);
    let actual = recv(&mut reader);
    assert_eq!(actual, serde_json::json!({"error": "too_large"}));
}

#[test]
fn slow_reader_keeps_responses() {
    let fixture = proto_dir().join("fixtures/flood.json");
    let mock = start_mock(0, Some(&fixture));
    let mut stream = connect(&mock);
    send(
        &mut stream,
        &serde_json::json!({"id":1,"op":"hello","protocol":2,"client":"test","pid":1}),
    );
    send(
        &mut stream,
        &serde_json::json!({"id":2,"op":"subscribe","topics":["status"]}),
    );
    send(&mut stream, &serde_json::json!({"id":3,"op":"status"}));
    thread::sleep(Duration::from_millis(150));
    let mut buf = Vec::new();
    stream
        .set_read_timeout(Some(Duration::from_millis(400)))
        .unwrap();
    loop {
        let mut chunk = [0u8; 256];
        match stream.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(err)
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.kind() == std::io::ErrorKind::TimedOut =>
            {
                break;
            }
            Err(err) => panic!("{err}"),
        }
    }
    let text = String::from_utf8_lossy(&buf);
    let lines: Vec<Value> = text
        .lines()
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_str(l).expect(l))
        .collect();
    let responses: Vec<_> = lines.iter().filter(|v| v.get("id").is_some()).collect();
    let events: Vec<_> = lines
        .iter()
        .filter(|v| v.get("event") == Some(&Value::String("status".into())))
        .collect();
    assert!(
        responses.iter().any(|v| v["id"] == 1 && v["ok"] == true),
        "hello dropped: {lines:?}"
    );
    assert!(
        responses.iter().any(|v| v["id"] == 2 && v["ok"] == true),
        "subscribe dropped: {lines:?}"
    );
    assert!(
        responses.iter().any(|v| v["id"] == 3 && v["ok"] == true),
        "status response dropped: {lines:?}"
    );
    assert!(
        events.len() < 300,
        "status queue should drop stale events, got {}",
        events.len()
    );
}
