//! Searcher tests that need a large index (float16 across blocks).
//! `upstream/Tests/DigUpKitTests/LongFilesTests.swift` `float16VectorsScoreAcrossBlocks`

use trowel_core::{
    Candidate, FakeEmbedder, FileKind, IndexStore, Modality, Searcher, SegmentKind, VectorStorage,
};

fn temp() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "trowel-searcher-blocks-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn float16_vectors_score_across_blocks() {
    let dir = temp();
    let store = IndexStore::open(&dir).unwrap();
    let rows = VectorStorage::BLOCK_ROWS * 2 + 300;
    let mut target = Vec::new();
    store
        .db
        .transaction(|| {
            for index in 0..rows {
                let id = store.insert(&Candidate {
                    path: format!("/v/{index}.png"),
                    kind: FileKind::Image,
                    size: 1,
                    mtime: 1.0,
                    inode: (index + 1) as u64,
                    device: 1,
                })?;
                let vector = FakeEmbedder::vector_for(&index.to_string());
                if index == rows - 5 {
                    target = vector.clone();
                }
                store.add_segment(
                    id,
                    SegmentKind::Image,
                    Modality::Image,
                    None,
                    None,
                    None,
                    None,
                    Some(&vector),
                )?;
            }
            Ok(())
        })
        .unwrap();
    let mut searcher = Searcher::new(store, true).unwrap();
    assert_eq!(searcher.vector_count(), rows);
    let hits = searcher
        .search("", Some(&target), None, 3, 2.0, None, false)
        .unwrap();
    let want = format!("/v/{}.png", rows - 5);
    assert_eq!(hits.first().map(|h| h.path.as_str()), Some(want.as_str()));
    assert!((hits.first().and_then(|h| h.cosine).unwrap_or(0.0) - 1.0).abs() < 0.002);
    let _ = std::fs::remove_dir_all(dir);
}
