//! Phase 7 fault injection that we can run without a GPU soak.

use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use trowel_core::IndexStore;

fn tmp() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "trowel-fault-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn corrupt_index_is_quarantined_and_reopened() {
    let dir = tmp();
    let path = dir.join("index.sqlite");
    fs::write(&path, vec![0xFFu8; 4096]).unwrap();
    let store = IndexStore::open(&dir).expect("open after quarantine");
    drop(store);
    let names: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect();
    assert!(
        names.iter().any(|n| n.contains("corrupt")),
        "expected a quarantined copy, got {names:?}"
    );
    assert!(dir.join("index.sqlite").is_file());
    let _ = fs::remove_dir_all(&dir);
}
