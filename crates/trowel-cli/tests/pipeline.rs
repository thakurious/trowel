use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn trowel() -> Command {
    Command::new(env!("CARGO_BIN_EXE_trowel"))
}

fn temp() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "trowel-cli-pipe-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("files")).unwrap();
    dir
}

#[test]
fn index_then_keyword_search() {
    let root = temp();
    let files = root.join("files");
    let index = root.join("index");
    std::fs::write(
        files.join("notes.txt"),
        "the harbour meeting notes from the village library\n",
    )
    .unwrap();
    let out = trowel()
        .args([
            "index",
            files.to_str().unwrap(),
            "--index",
            index.to_str().unwrap(),
            "--fake",
        ])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "index: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let status = trowel()
        .args(["status", "--index", index.to_str().unwrap(), "--json"])
        .output()
        .unwrap();
    assert!(status.status.success());
    let body: serde_json::Value = serde_json::from_slice(&status.stdout).unwrap();
    assert!(body["vectors"].as_i64().unwrap() >= 1);
    let search = trowel()
        .args([
            "search",
            "harbour",
            "--index",
            index.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(search.status.success());
    let hits: serde_json::Value = serde_json::from_slice(&search.stdout).unwrap();
    assert!(
        hits.as_array()
            .unwrap()
            .iter()
            .any(|h| { h["path"].as_str().unwrap_or("").ends_with("notes.txt") }),
        "{hits}"
    );
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn index_service_sync_and_idle() {
    let root = temp();
    let files = root.join("files");
    let index = root.join("index");
    std::fs::write(files.join("a.txt"), "the river garden morning letter\n").unwrap();
    let mut child = trowel()
        .args([
            "index-service",
            files.to_str().unwrap(),
            "--index",
            index.to_str().unwrap(),
            "--fake",
            "--backfill",
            "--linger",
            "0.2",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let stdin = child.stdin.take().unwrap();
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("\"event\":\"sync\""), "{stdout}");
    assert!(stdout.contains("\"event\":\"exit\""), "{stdout}");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn touch_one_file_reports_changed() {
    let root = temp();
    let files = root.join("files");
    let index = root.join("index");
    std::fs::write(
        files.join("keep.txt"),
        "the library summer village lantern\n",
    )
    .unwrap();
    std::fs::write(
        files.join("edit.txt"),
        "the harbour kitchen mountain station\n",
    )
    .unwrap();
    assert!(trowel()
        .args([
            "index",
            files.to_str().unwrap(),
            "--index",
            index.to_str().unwrap(),
            "--fake",
        ])
        .status()
        .unwrap()
        .success());
    std::fs::write(
        files.join("edit.txt"),
        "the harbour kitchen mountain station meadow extra\n",
    )
    .unwrap();
    let mut child = trowel()
        .args([
            "index-service",
            files.to_str().unwrap(),
            "--index",
            index.to_str().unwrap(),
            "--fake",
            "--linger",
            "0.1",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let mut stdin = child.stdin.take().unwrap();
    let _ = writeln!(stdin, "sync");
    drop(stdin);
    let out = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("\"changed\":1") || stdout.contains("\"changed\": 1"),
        "{stdout}"
    );
    let _ = std::fs::remove_dir_all(root);
}
