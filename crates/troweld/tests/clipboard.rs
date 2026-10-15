//! Real troweld clipboard history (Phase 8 L3) with stub `wl-paste`, `hyprctl`, `wl-copy` and
//! `wtype` on `PATH`: no compositor, never the developer's clipboard.

use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
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
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn trowel_bin() -> PathBuf {
    let troweld = PathBuf::from(env!("CARGO_BIN_EXE_troweld"));
    let trowel = troweld.with_file_name("trowel");
    let status = Command::new(env!("CARGO"))
        .args(["build", "-p", "trowel-cli", "--bin", "trowel", "--quiet"])
        .status()
        .expect("build trowel");
    assert!(status.success(), "cargo build -p trowel-cli failed");
    trowel
}

fn script(path: &Path, body: &str) {
    fs::write(path, body).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// The stub `wl-paste --watch` runs its command once per `stub/next` directory the test drops in
/// (renamed to `stub/current`), with `CLIPBOARD_STATE` from `current/state`, then writes the
/// count to `stub/handled`. `--list-types` and `--type MIME` read `current/`.
fn stubs(root: &Path) -> PathBuf {
    let bin = root.join("bin");
    let stub = root.join("stub");
    fs::create_dir_all(&bin).unwrap();
    fs::create_dir_all(&stub).unwrap();
    let s = stub.display();
    script(
        &bin.join("wl-paste"),
        &format!(
            r#"#!/bin/sh
S='{s}'
case "$1" in
  --watch)
    shift
    while :; do
      if [ -d "$S/next" ]; then
        rm -rf "$S/current"; mv "$S/next" "$S/current"
        n=$(( $(cat "$S/handled" 2>/dev/null || echo 0) + 1 ))
        st=$(cat "$S/current/state" 2>/dev/null || echo data)
        CLIPBOARD_STATE=$st "$@" < /dev/null
        echo "$n" > "$S/handled"
      fi
      sleep 0.02
    done ;;
  --list-types) cat "$S/current/types" ;;
  *) f=$(printf %s "$3" | tr '/;=' '___'); cat "$S/current/data/$f" ;;
esac
"#
        ),
    );
    script(
        &bin.join("hyprctl"),
        &format!("#!/bin/sh\ncat '{s}/current/window' 2>/dev/null || echo '{{}}'\n"),
    );
    script(
        &bin.join("wl-copy"),
        &format!(
            "#!/bin/sh\necho \"$2\" > '{r}/copy-type'\ncat > '{r}/copy-data'\n",
            r = root.display()
        ),
    );
    script(
        &bin.join("wtype"),
        &format!("#!/bin/sh\necho \"$@\" >> '{}/keys'\n", root.display()),
    );
    stub
}

fn start(root: PathBuf, cfg: Option<trowel_proto::Config>) -> Daemon {
    let socket = root.join("troweld.sock");
    let bin = root.join("bin");
    let cfg = cfg.unwrap_or_default();
    let cfg = trowel_proto::Config {
        index_dir: root.join("index").to_string_lossy().into_owned(),
        ..cfg
    };
    troweld::config_file::save(&root.join("config.toml"), &cfg).unwrap();
    let path = format!(
        "{}:{}",
        bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let child = Command::new(env!("CARGO_BIN_EXE_troweld"))
        .arg("--fake")
        .arg("--socket")
        .arg(&socket)
        .arg("--index")
        .arg(root.join("index"))
        .arg("--config")
        .arg(root.join("config.toml"))
        .arg("--state-dir")
        .arg(root.join("state"))
        .env("XDG_RUNTIME_DIR", &root)
        .env("XDG_CONFIG_HOME", &root)
        .env("XDG_CACHE_HOME", root.join("cache"))
        .env("XDG_DATA_HOME", root.join("data"))
        .env("XDG_DATA_DIRS", root.join("sys"))
        .env("XDG_STATE_HOME", root.join("xstate"))
        .env("OMARCHY_PATH", root.join("omarchy"))
        .env("WAYLAND_DISPLAY", "trowel-test-no-compositor")
        .env("TROWEL", trowel_bin())
        .env("PATH", path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn troweld");
    let d = Daemon {
        child,
        socket,
        root,
    };
    let deadline = Instant::now() + Duration::from_secs(4);
    while !d.socket.exists() {
        assert!(Instant::now() < deadline, "troweld did not bind");
        thread::sleep(Duration::from_millis(10));
    }
    d
}

fn temp() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "trowel-clip-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

struct Client {
    w: UnixStream,
    r: BufReader<UnixStream>,
    next: u64,
    events: Vec<Value>,
}

impl Client {
    fn new(d: &Daemon) -> Self {
        let s = UnixStream::connect(&d.socket).unwrap();
        s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut c = Self {
            w: s.try_clone().unwrap(),
            r: BufReader::new(s),
            next: 1,
            events: Vec::new(),
        };
        let hi = c.call(json!({"op":"hello","protocol":2,"client":"test","pid":1}));
        assert_eq!(hi["ok"], true, "{hi}");
        c
    }

    fn call(&mut self, mut req: Value) -> Value {
        let id = self.next;
        self.next += 1;
        req["id"] = json!(id);
        writeln!(self.w, "{req}").unwrap();
        loop {
            let mut line = String::new();
            self.r.read_line(&mut line).unwrap();
            let v: Value = serde_json::from_str(line.trim()).unwrap();
            if v.get("event").is_some() {
                self.events.push(v);
            } else if v["id"] == id {
                return v;
            }
        }
    }

    fn rows(&mut self) -> Vec<Value> {
        let v = self.call(json!({"op":"clip_query","gen":1}));
        assert_eq!(v["ok"], true, "{v}");
        v["rows"].as_array().unwrap().clone()
    }
}

/// One selection change: `types` in offer order, `data` per type, the focused window's class.
fn copy(stub: &Path, n: u32, types: &[&str], data: &[(&str, &[u8])], app: &str, state: &str) {
    let next = stub.join("next.tmp");
    let _ = fs::remove_dir_all(&next);
    fs::create_dir_all(next.join("data")).unwrap();
    fs::write(next.join("types"), types.join("\n") + "\n").unwrap();
    for (mime, bytes) in data {
        let f: String = mime
            .chars()
            .map(|c| if "/;=".contains(c) { '_' } else { c })
            .collect();
        fs::write(next.join("data").join(f), bytes).unwrap();
    }
    fs::write(next.join("window"), json!({ "class": app }).to_string()).unwrap();
    fs::write(next.join("state"), state).unwrap();
    fs::rename(&next, stub.join("next")).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let handled = fs::read_to_string(stub.join("handled"))
            .ok()
            .and_then(|s| s.trim().parse::<u32>().ok())
            .unwrap_or(0);
        if handled >= n {
            return;
        }
        assert!(Instant::now() < deadline, "stub watcher never ran copy {n}");
        thread::sleep(Duration::from_millis(10));
    }
}

fn text(stub: &Path, n: u32, body: &str, app: &str) {
    copy(
        stub,
        n,
        &["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"],
        &[("text/plain;charset=utf-8", body.as_bytes())],
        app,
        "data",
    );
}

fn wait_watcher(c: &mut Client) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let doc = c.call(json!({"op":"doctor"}));
        let row = doc["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "clipboard")
            .cloned()
            .unwrap_or_else(|| panic!("no clipboard doctor row: {doc}"));
        if row["status"] == "OK" && row["detail"].as_str().unwrap().contains("running") {
            return;
        }
        assert!(Instant::now() < deadline, "watcher never started: {row}");
        thread::sleep(Duration::from_millis(50));
    }
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut b = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
    b.extend(w.to_be_bytes());
    b.extend(h.to_be_bytes());
    b.extend([8, 6, 0, 0, 0, 1, 2, 3, 4]);
    b
}

#[test]
fn capture_classify_dedup_restore_and_privacy() {
    let root = temp();
    let stub = stubs(&root);
    let d = start(root.clone(), None);
    let mut c = Client::new(&d);
    let hi = c.call(json!({"op":"hello","protocol":2,"client":"test","pid":1}));
    let caps = hi["capabilities"].as_array().unwrap();
    assert!(caps.iter().any(|c| c == "clipboard"), "{hi}");
    assert_eq!(c.rows(), Vec::<Value>::new());
    assert!(
        !root.join("state/state.sqlite").exists(),
        "an empty query creates nothing"
    );
    c.call(json!({"op":"subscribe","topics":["clipboard"]}));
    wait_watcher(&mut c);

    // Text, from Slack.
    text(&stub, 1, "firewall rules for the staging cluster", "Slack");
    let rows = c.rows();
    assert_eq!(rows.len(), 1, "{rows:?}");
    assert_eq!(rows[0]["kind"], "text");
    assert_eq!(rows[0]["source_app"], "Slack");
    assert_eq!(rows[0]["mimes"], json!(["text/plain"]));
    let first = rows[0]["id"].as_i64().unwrap();

    // An image (Chrome "Copy Image" offers HTML too; the image wins).
    let image = png(4, 3);
    copy(
        &stub,
        2,
        &[
            "text/x-moz-url",
            "text/html",
            "image/png",
            "chromium/x-source-url",
        ],
        &[("image/png", &image), ("text/html", b"<img src=x>")],
        "google-chrome",
        "data",
    );
    // A file copy, as Strata offers it (L0.2).
    let file = root.join("my report.pdf");
    fs::write(&file, b"%PDF").unwrap();
    let uri = format!("file://{}\r\n", root.join("my%20report.pdf").display());
    copy(
        &stub,
        3,
        &[
            "text/plain;charset=utf-8",
            "text/uri-list",
            "application/vnd.portal.files",
        ],
        &[
            ("text/uri-list", uri.as_bytes()),
            ("text/plain;charset=utf-8", b"my report.pdf"),
        ],
        "io.github.lgse.Strata",
        "data",
    );
    // Formatted text from a page.
    copy(
        &stub,
        4,
        &["text/plain", "text/html", "chromium/x-source-url"],
        &[
            ("text/html", b"<p><b>Refund policy</b>: 30 days</p>"),
            ("text/plain", b"Refund policy: 30 days"),
            ("chromium/x-source-url", b"https://www.example.com/help"),
        ],
        "google-chrome",
        "data",
    );
    text(&stub, 5, "#f2a22a", "gimp");
    text(
        &stub,
        6,
        "https://github.com/thakurious/trowel/pull/7",
        "google-chrome",
    );
    let rows = c.rows();
    let kinds: Vec<&str> = rows.iter().map(|r| r["kind"].as_str().unwrap()).collect();
    assert_eq!(
        kinds,
        ["link", "color", "html", "files", "image", "text"],
        "{rows:?}"
    );
    assert_eq!(rows[0]["domain"], "github.com");
    assert_eq!(rows[1]["color"], "#F2A22A");
    assert_eq!(rows[2]["domain"], "example.com");
    assert_eq!(rows[2]["preview"], "Refund policy: 30 days");
    assert_eq!(rows[3]["files"], 1);
    assert_eq!(rows[3]["preview"], "my report.pdf");
    assert_eq!(rows[4]["preview"], "Image 4×3");
    let (image_id, files_id, html_id) = (
        rows[4]["id"].as_i64().unwrap(),
        rows[3]["id"].as_i64().unwrap(),
        rows[2]["id"].as_i64().unwrap(),
    );

    // Full content.
    let got = c.call(json!({"op":"clip_get","clip_id":image_id}));
    assert_eq!(
        (got["image"]["w"].as_u64(), got["image"]["h"].as_u64()),
        (Some(4), Some(3))
    );
    let blob = PathBuf::from(got["image"]["file"].as_str().unwrap());
    assert_eq!(fs::read(&blob).unwrap(), image, "stored byte-identical");
    assert!(blob.starts_with(root.join("state/clipboard")));
    let got = c.call(json!({"op":"clip_get","clip_id":files_id}));
    assert_eq!(got["files"], json!([file.to_string_lossy()]));
    let got = c.call(json!({"op":"clip_get","clip_id":html_id}));
    assert_eq!(got["html"], "<p><b>Refund policy</b>: 30 days</p>");
    assert_eq!(got["text"], "Refund policy: 30 days");

    // Search.
    let hits = c.call(json!({"op":"clip_query","text":"refund pol","gen":2}));
    assert_eq!(hits["total"], 1, "{hits}");
    let hits = c.call(json!({"op":"clip_query","kinds":["image","files"],"gen":3}));
    assert_eq!(hits["total"], 2, "{hits}");
    let bad = c.call(json!({"op":"clip_query","kinds":["nope"],"gen":4}));
    assert_eq!(bad["error"]["code"], "bad_request", "{bad}");

    // Re-copying bumps instead of duplicating.
    text(&stub, 7, "firewall rules for the staging cluster", "Slack");
    let rows = c.rows();
    assert_eq!(rows.len(), 6);
    assert_eq!(rows[0]["id"], first);
    assert_eq!(rows[0]["uses"], 2);

    // Privacy: a password manager's copy, the KDE hint, an excluded app.
    copy(
        &stub,
        8,
        &["text/plain"],
        &[("text/plain", b"hunter2")],
        "Chromium",
        "sensitive",
    );
    copy(
        &stub,
        9,
        &["text/plain", "x-kde-passwordManagerHint"],
        &[
            ("text/plain", b"hunter3"),
            ("x-kde-passwordManagerHint", b"secret"),
        ],
        "kitty",
        "data",
    );
    text(&stub, 10, "hunter4", "1Password");
    let hits = c.call(json!({"op":"clip_query","text":"hunter","gen":5}));
    assert_eq!(hits["total"], 0, "{hits}");

    // Only the daemon's own capture child may ingest.
    let forged = c.call(json!({"op":"clip_ingest","types":["text/plain"],
        "offers":[{"mime":"text/plain","file":"/etc/hostname"}]}));
    assert_eq!(forged["error"]["code"], "bad_request", "{forged}");

    // Restore: the image with its MIME, byte-identical.
    let pasted = c.call(json!({"op":"clip_paste","clip_id":image_id,"mode":"copy"}));
    assert_eq!(pasted["ok"], true, "{pasted}");
    assert_eq!(
        fs::read_to_string(root.join("copy-type")).unwrap().trim(),
        "image/png"
    );
    assert_eq!(fs::read(root.join("copy-data")).unwrap(), image);
    assert!(!root.join("keys").exists(), "copy mode sends no keystroke");
    // The daemon's own copy is not captured again within a second.
    text(&stub, 11, "echo of my own paste", "Slack");
    let hits = c.call(json!({"op":"clip_query","text":"echo","gen":6}));
    assert_eq!(hits["total"], 0, "{hits}");
    // Files go back as one text/uri-list offer (L0.3), and paste mode sends Shift+Insert.
    let pasted = c.call(json!({"op":"clip_paste","clip_id":files_id,"mode":"paste"}));
    assert_eq!(pasted["ok"], true, "{pasted}");
    assert_eq!(
        fs::read_to_string(root.join("copy-type")).unwrap().trim(),
        "text/uri-list"
    );
    assert_eq!(fs::read_to_string(root.join("copy-data")).unwrap(), uri);
    let deadline = Instant::now() + Duration::from_secs(2);
    while !root.join("keys").exists() {
        assert!(Instant::now() < deadline, "no paste keystroke");
        thread::sleep(Duration::from_millis(20));
    }
    assert_eq!(
        fs::read_to_string(root.join("keys")).unwrap().trim(),
        "-M shift -k Insert -m shift"
    );
    // Plain drops the HTML.
    c.call(json!({"op":"clip_paste","clip_id":html_id,"mode":"plain"}));
    assert_eq!(
        fs::read_to_string(root.join("copy-type")).unwrap().trim(),
        "text/plain;charset=utf-8"
    );
    assert_eq!(
        fs::read_to_string(root.join("copy-data")).unwrap(),
        "Refund policy: 30 days"
    );
    thread::sleep(Duration::from_millis(1100));

    // Incognito: nothing is captured while paused; capture resumes after.
    let paused = c.call(json!({"op":"clip_pause"}));
    assert_eq!(paused["paused_until"], -1, "{paused}");
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let doc = c.call(json!({"op":"doctor"}));
        let row = doc["checks"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "clipboard")
            .cloned()
            .unwrap();
        if row["detail"].as_str().unwrap().starts_with("paused") {
            break;
        }
        assert!(Instant::now() < deadline, "{row}");
        thread::sleep(Duration::from_millis(20));
    }
    let resumed = c.call(json!({"op":"clip_pause","minutes":0}));
    assert_eq!(resumed["paused_until"], 0);
    wait_watcher(&mut c);
    text(&stub, 12, "after resume", "Slack");
    let hits = c.call(json!({"op":"clip_query","text":"after resume","gen":7}));
    assert_eq!(hits["total"], 1, "{hits}");

    // Pin, delete, clear.
    assert_eq!(
        c.call(json!({"op":"clip_pin","clip_id":image_id,"pinned":true}))["ok"],
        true
    );
    assert_eq!(c.rows()[0]["id"], image_id, "pinned first");
    assert_eq!(
        c.call(json!({"op":"clip_delete","clip_id":files_id}))["ok"],
        true
    );
    let gone = c.call(json!({"op":"clip_delete","clip_id":files_id}));
    assert_eq!(gone["error"]["code"], "not_found", "{gone}");
    let cleared = c.call(json!({"op":"clip_clear"}));
    assert_eq!(cleared["removed"], 5, "{cleared}");
    let rows = c.rows();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], image_id);
    assert!(blob.is_file(), "the pinned image's blob stays");
    c.call(json!({"op":"clip_clear","keep_pinned":false}));
    assert!(!blob.exists(), "its blob goes with the last reference");
    assert_eq!(
        fs::read_dir(root.join("clip-spool")).unwrap().count(),
        0,
        "spool files are deleted after ingest"
    );

    // Events arrived for adds and removals (coalesced: the last one within 250 ms).
    thread::sleep(Duration::from_millis(300));
    c.call(json!({"op":"status"}));
    assert!(c
        .events
        .iter()
        .any(|e| e["event"] == "clipboard" && e.get("added").is_some()));
    assert!(c
        .events
        .iter()
        .any(|e| e["event"] == "clipboard" && e.get("removed").is_some()));
}

#[test]
fn capture_off_runs_no_watcher_and_import_copies_omarchy_history() {
    let root = temp();
    stubs(&root);
    let mut cfg = trowel_proto::Config::default();
    cfg.clipboard.capture = false;
    let d = start(root.clone(), Some(cfg));
    let mut c = Client::new(&d);
    let doc = c.call(json!({"op":"doctor"}));
    let row = doc["checks"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["name"] == "clipboard")
        .cloned()
        .unwrap();
    assert!(
        row["detail"].as_str().unwrap().starts_with("capture off"),
        "{row}"
    );

    let omarchy = root.join("xstate/omarchy");
    fs::create_dir_all(omarchy.join("clipboard-images")).unwrap();
    let img = omarchy.join("clipboard-images/aa.png");
    fs::write(&img, png(8, 8)).unwrap();
    fs::write(
        omarchy.join("clipboard-history.json"),
        json!([
            {"type":"text","text":"newest"},
            {"type":"image","mime":"image/png","path":img,"capturedAt":"Tuesday 14:03"},
            {"type":"text","text":"oldest"}
        ])
        .to_string(),
    )
    .unwrap();
    let out = c.call(json!({"op":"clip_import","source":"omarchy"}));
    assert_eq!(
        (out["imported"].as_u64(), out["skipped"].as_u64()),
        (Some(3), Some(0)),
        "{out}"
    );
    let previews: Vec<String> = c
        .rows()
        .iter()
        .map(|r| r["preview"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(previews, ["newest", "Image 8×8", "oldest"]);
    let again = c.call(json!({"op":"clip_import"}));
    assert_eq!(
        (again["imported"].as_u64(), again["skipped"].as_u64()),
        (Some(0), Some(3))
    );
    assert!(img.is_file(), "Omarchy's image is copied, not moved");
    let bad = c.call(json!({"op":"clip_import","source":"klipper"}));
    assert_eq!(bad["error"]["code"], "bad_request");
}
