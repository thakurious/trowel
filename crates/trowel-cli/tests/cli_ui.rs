use std::fs;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

struct Mock {
    child: std::process::Child,
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

fn spawn_mock() -> Mock {
    let dir = std::env::temp_dir().join(format!("trowel-cli-ui-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let socket = dir.join("troweld.sock");
    let trowel = PathBuf::from(env!("CARGO_BIN_EXE_trowel"));
    let troweld = trowel.with_file_name("troweld");
    assert!(
        troweld.exists(),
        "troweld missing at {}; run cargo test --workspace",
        troweld.display()
    );
    let child = Command::new(&troweld)
        .args(["--mock", "--socket"])
        .arg(&socket)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn troweld --mock");
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

#[test]
fn portal_pick_folder_is_a_subcommand() {
    let bin = env!("CARGO_BIN_EXE_trowel");
    let out = Command::new(bin)
        .args(["portal-pick-folder", "--help"])
        .output()
        .expect("run trowel");
    assert!(out.status.success(), "{:?}", out);
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(text.contains("folder") || text.contains("portal"), "{text}");
}

#[test]
fn debug_and_autostart_are_documented() {
    let bin = env!("CARGO_BIN_EXE_trowel");
    let help = Command::new(bin).arg("--help").output().unwrap();
    let text = String::from_utf8_lossy(&help.stdout);
    assert!(text.contains("debug"), "{text}");
    assert!(text.contains("autostart"), "{text}");
    let trace = Command::new(bin)
        .args(["debug", "ui-trace", "--help"])
        .output()
        .unwrap();
    assert!(trace.status.success());
}

#[test]
fn autostart_status_is_readonly() {
    let bin = env!("CARGO_BIN_EXE_trowel");
    let out = Command::new(bin)
        .args(["autostart", "status"])
        .output()
        .unwrap();
    assert!(out.status.success(), "{:?}", out);
}

#[test]
fn ui_trace_against_mock_meets_ipc_budgets() {
    let mock = spawn_mock();
    let bin = env!("CARGO_BIN_EXE_trowel");
    let out = Command::new(bin)
        .args([
            "debug",
            "ui-trace",
            "--json",
            "--socket",
            mock.socket.to_str().unwrap(),
        ])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "ui-trace failed: status={:?} stdout={stdout} stderr={stderr}",
        out.status
    );
    let rows: serde_json::Value = serde_json::from_str(&stdout).expect(&stdout);
    for row in rows.as_array().unwrap() {
        assert_eq!(row["ok"], true, "{row}");
    }
}
