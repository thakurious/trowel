//! `upstream/Tests/DigUpKitTests/LongFilesTests.swift` (float16 search waits for Phase 4).

use std::cell::Cell;
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use trowel_core::{
    Candidate, Costs, DocExtractor, Embedder, Estimator, FakeEmbedder, FileKind, IndexOptions,
    IndexStore, Indexer, Modality, PdfExtractor, SearchText, SegmentKind, SqlValue,
};

static N: AtomicU64 = AtomicU64::new(0);

struct Harness {
    folder: PathBuf,
    files: PathBuf,
}

impl Harness {
    fn new() -> Self {
        let n = N.fetch_add(1, Ordering::Relaxed);
        let folder =
            std::env::temp_dir().join(format!("trowel-longfiles-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        let files = folder.join("files");
        std::fs::create_dir_all(&files).unwrap();
        Self { folder, files }
    }

    fn store(&self) -> IndexStore {
        IndexStore::open(self.folder.join("index")).unwrap()
    }

    fn indexer(&self, store: IndexStore, embedder: FakeEmbedder, options: IndexOptions) -> Indexer {
        Indexer::new(
            store,
            options,
            Box::new(move || Ok(Box::new(embedder.clone()) as Box<dyn Embedder>)),
        )
        .unwrap()
    }
}

impl Drop for Harness {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.folder);
    }
}

fn prose(characters: usize, seed: i32, special: Option<(usize, &str)>) -> String {
    let words = [
        "river", "garden", "morning", "letter", "window", "harbour", "kitchen", "mountain",
        "station", "market", "evening", "bridge", "orchard", "lantern", "meadow", "village",
        "library", "summer",
    ];
    let mut pieces = Vec::new();
    let mut count = 0usize;
    let mut index = seed;
    let mut placed = special.is_none();
    while count < characters {
        index = index.wrapping_mul(1_103_515_245).wrapping_add(12_345) & 0x7fff_ffff;
        let mut piece = words[(index as usize) % words.len()].to_string();
        if index % 7 == 0 {
            piece.push('.');
        }
        if !placed {
            if let Some((at, word)) = special {
                if count >= at {
                    piece = format!("the {word} swims. {piece}");
                    placed = true;
                }
            }
        }
        count += piece.chars().count() + 1;
        pieces.push(piece);
    }
    pieces.join(" ")
}

fn pdf_escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('(', "\\(")
        .replace(')', "\\)")
}

fn make_pdf(dir: &Path, name: &str, pages: i32, special: &[(i32, &str)]) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(name);
    let special: HashMap<i32, &str> = special.iter().copied().collect();
    let mut objects: Vec<Vec<u8>> = vec![Vec::new()];
    let push = |objects: &mut Vec<Vec<u8>>, body: Vec<u8>| {
        objects.push(body);
        objects.len() as i32 - 1
    };

    let catalog_id = 1;
    let pages_id = 2;
    let font_id = 3;
    let mut page_ids = Vec::new();
    let mut content_ids = Vec::new();
    objects.resize(4, Vec::new());

    for page in 1..=pages {
        let mut text = format!("Page {page}. ") + &prose(600, page, None);
        if let Some(word) = special.get(&page) {
            text.push_str(&format!(" The {word} lives here."));
        }
        // Several short Tj lines: a single 600-char string sits off the page and
        // pdftotext -layout keeps only ~100 characters, which drops special words
        // and forces a pdftoppm fallback.
        let mut lines = Vec::new();
        let chars: Vec<char> = text.chars().collect();
        let mut at = 0;
        while at < chars.len() {
            let end = (at + 80).min(chars.len());
            lines.push(chars[at..end].iter().collect::<String>());
            at = end;
        }
        let mut stream = String::from("BT /F1 12 Tf 60 720 Td\n");
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                stream.push_str("0 -14 Td\n");
            }
            stream.push_str(&format!("({}) Tj\n", pdf_escape(line)));
        }
        stream.push_str("ET\n");
        let content = format!(
            "<< /Length {} >>\nstream\n{}endstream",
            stream.len(),
            stream
        )
        .into_bytes();
        let content_id = push(&mut objects, content);
        content_ids.push(content_id);
        let page_dict = format!(
            "<< /Type /Page /Parent {pages_id} 0 R /MediaBox [0 0 612 792] \
             /Resources << /Font << /F1 {font_id} 0 R >> >> /Contents {content_id} 0 R >>"
        )
        .into_bytes();
        page_ids.push(push(&mut objects, page_dict));
    }

    objects[catalog_id as usize] =
        format!("<< /Type /Catalog /Pages {pages_id} 0 R >>").into_bytes();
    let kids = page_ids
        .iter()
        .map(|id| format!("{id} 0 R"))
        .collect::<Vec<_>>()
        .join(" ");
    objects[pages_id as usize] =
        format!("<< /Type /Pages /Kids [{kids}] /Count {pages} >>").into_bytes();
    objects[font_id as usize] = b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec();

    let mut pdf = b"%PDF-1.4\n".to_vec();
    let mut offsets = vec![0usize; objects.len()];
    for id in 1..objects.len() {
        offsets[id] = pdf.len();
        pdf.extend_from_slice(format!("{id} 0 obj\n").as_bytes());
        pdf.extend_from_slice(&objects[id]);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref_at = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n", objects.len()).as_bytes());
    pdf.extend_from_slice(b"0000000000 65535 f \n");
    for offset in offsets.iter().skip(1) {
        pdf.extend_from_slice(format!("{:010} 00000 n \n", offset).as_bytes());
    }
    pdf.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
            objects.len()
        )
        .as_bytes(),
    );
    std::fs::write(&path, pdf).unwrap();
    let _ = content_ids;
    path
}

fn places(store: &IndexStore, path: &str, kinds: &[&str]) -> Vec<i32> {
    let list = kinds
        .iter()
        .map(|k| format!("'{k}'"))
        .collect::<Vec<_>>()
        .join(",");
    let mut out = Vec::new();
    store
        .db
        .query(
            &format!(
                "SELECT s.loc FROM segments s JOIN files f ON f.id = s.file \
                 WHERE f.path = ? AND s.kind IN ({list}) ORDER BY s.loc"
            ),
            &[SqlValue::Text(path.into())],
            |row| {
                out.push(row.double(0) as i32);
                Ok(())
            },
        )
        .unwrap();
    out
}

fn state(store: &IndexStore, path: &str) -> (String, BTreeMap<String, String>) {
    let mut result = (String::new(), BTreeMap::new());
    store
        .db
        .query(
            "SELECT id, state FROM files WHERE path = ?",
            &[SqlValue::Text(path.into())],
            |row| {
                let id = row.int(0);
                result = (row.text(1).unwrap_or_default(), store.info(id).unwrap());
                Ok(())
            },
        )
        .unwrap();
    result
}

fn found(store: &IndexStore, word: &str) -> Vec<i32> {
    let mut out = Vec::new();
    store
        .db
        .query(
            "SELECT s.loc FROM fts JOIN segments s ON s.id = fts.rowid WHERE fts MATCH ? ORDER BY s.loc",
            &[SqlValue::Text(format!("\"{word}\""))],
            |row| {
                out.push(row.double(0) as i32);
                Ok(())
            },
        )
        .unwrap();
    out
}

fn run(indexer: &mut Indexer) -> BTreeMap<String, i32> {
    indexer.run(None, None, &|| false, |_| {}).unwrap()
}

#[test]
fn make_pdf_extracts_text_pages() {
    let h = Harness::new();
    let pdf = make_pdf(&h.files, "Field guide.pdf", 5, &[(2, "quokka")]);
    let out = h.folder.join("out");
    std::fs::create_dir_all(&out).unwrap();
    let (pages, total, _) = PdfExtractor::pages(&pdf, |t| 0..t, 200, 1400, &out).unwrap();
    assert_eq!(total, 5);
    assert_eq!(pages.len(), 5);
    assert!(pages[0].text.contains("Page 1"));
    assert!(pages[1].text.contains("quokka"));
    assert!(pages.iter().all(|p| p.image_path.is_none()));
}

#[test]
fn long_pdf_is_read_in_steps() {
    let h = Harness::new();
    let pdf = make_pdf(&h.files, "Field guide.pdf", 120, &[(100, "quokka")]);
    let path = pdf.to_string_lossy().into_owned();
    let embedder = FakeEmbedder::new();
    let mut indexer = h.indexer(h.store(), embedder.clone(), IndexOptions::default());
    indexer.sync(std::slice::from_ref(&h.files)).unwrap();
    assert_eq!(run(&mut indexer).get("partial").copied(), Some(1));
    assert_eq!(
        places(&indexer.store, &path, &["page", "page_image", "chunk"]),
        (1..=30).collect::<Vec<_>>()
    );
    let (st, info) = state(&indexer.store, &path);
    assert_eq!(st, "partial");
    assert_eq!(info.get("read_to").map(String::as_str), Some("30"));
    assert_eq!(indexer.store.unfinished_count().unwrap(), (1, 90));
    assert!(found(&indexer.store, "quokka").is_empty());

    let mut steps = Vec::new();
    assert_eq!(
        indexer
            .finish_long_files(&|| false, |s| steps.push(s))
            .unwrap()
            .get("done")
            .copied(),
        Some(1)
    );
    assert_eq!(
        steps.iter().map(|s| s.done).collect::<Vec<_>>(),
        vec![30, 60, 90]
    );
    assert_eq!(
        steps.iter().map(|s| s.total).collect::<Vec<_>>(),
        vec![90, 90, 90]
    );
    assert_eq!(
        steps.iter().map(|s| s.state.as_str()).collect::<Vec<_>>(),
        vec!["partial", "partial", "done"]
    );
    assert_eq!(
        steps.iter().map(|s| s.files).collect::<Vec<_>>(),
        vec![1, 1, 0]
    );
    assert_eq!(
        places(&indexer.store, &path, &["page", "page_image", "chunk"]),
        (1..=120).collect::<Vec<_>>()
    );
    assert_eq!(found(&indexer.store, "quokka"), vec![100]);
    let (done, info) = state(&indexer.store, &path);
    assert_eq!(done, "done");
    assert!(!info.contains_key("read_to") && !info.contains_key("read_end"));
    assert_eq!(info.get("pages").map(String::as_str), Some("120"));
    assert_eq!(indexer.store.unfinished_count().unwrap(), (0, 0));
    assert!(indexer
        .finish_long_files(&|| false, |_| {})
        .unwrap()
        .is_empty());
    assert_eq!(embedder.texts(), 120);
}

#[test]
fn stopping_between_steps_loses_nothing() {
    let h = Harness::new();
    let pdf = make_pdf(&h.files, "Book.pdf", 100, &[]);
    let path = pdf.to_string_lossy().into_owned();
    let mut indexer = h.indexer(h.store(), FakeEmbedder::new(), IndexOptions::default());
    indexer.sync(std::slice::from_ref(&h.files)).unwrap();
    run(&mut indexer);
    let steps = Cell::new(0);
    indexer
        .finish_long_files(&|| steps.get() >= 1, |_| steps.set(steps.get() + 1))
        .unwrap();
    assert_eq!(
        state(&indexer.store, &path)
            .1
            .get("read_to")
            .map(String::as_str),
        Some("60")
    );
    assert_eq!(indexer.store.unfinished_count().unwrap(), (1, 40));
    indexer.finish_long_files(&|| false, |_| {}).unwrap();
    assert_eq!(
        places(&indexer.store, &path, &["page", "page_image", "chunk"]),
        (1..=100).collect::<Vec<_>>()
    );
    assert_eq!(state(&indexer.store, &path).0, "done");
}

#[test]
fn a_changed_file_starts_over() {
    let h = Harness::new();
    let pdf = make_pdf(&h.files, "Book.pdf", 80, &[]);
    let path = pdf.to_string_lossy().into_owned();
    let mut indexer = h.indexer(h.store(), FakeEmbedder::new(), IndexOptions::default());
    indexer.sync(std::slice::from_ref(&h.files)).unwrap();
    run(&mut indexer);
    make_pdf(&h.files, "Book.pdf", 90, &[]);
    assert_eq!(
        indexer
            .sync(std::slice::from_ref(&h.files))
            .unwrap()
            .changed,
        1
    );
    assert_eq!(state(&indexer.store, &path).0, "pending");
    assert!(places(&indexer.store, &path, &["page", "page_image", "chunk"]).is_empty());
    run(&mut indexer);
    indexer.finish_long_files(&|| false, |_| {}).unwrap();
    assert_eq!(
        places(&indexer.store, &path, &["page", "page_image", "chunk"]),
        (1..=90).collect::<Vec<_>>()
    );
}

#[test]
fn long_document_goes_on_where_it_stopped() {
    let h = Harness::new();
    let text = prose(150_000, 1, Some((120_000, "axolotl")));
    let url = h.files.join("Notes.txt");
    std::fs::write(&url, &text).unwrap();
    let chunks = DocExtractor::chunks(&text, 1800, 200, 10_000);
    assert!(chunks.len() > 80);
    let path = url.to_string_lossy().into_owned();
    let mut indexer = h.indexer(h.store(), FakeEmbedder::new(), IndexOptions::default());
    indexer.sync(std::slice::from_ref(&h.files)).unwrap();
    run(&mut indexer);
    assert_eq!(
        places(&indexer.store, &path, &["page", "page_image", "chunk"]),
        (1..=40).collect::<Vec<_>>()
    );
    let expected_end = chunks.len().to_string();
    assert_eq!(
        state(&indexer.store, &path)
            .1
            .get("read_end")
            .map(String::as_str),
        Some(expected_end.as_str())
    );
    assert!(found(&indexer.store, "axolotl").is_empty());
    indexer.finish_long_files(&|| false, |_| {}).unwrap();
    assert_eq!(
        places(&indexer.store, &path, &["page", "page_image", "chunk"]),
        (1..=chunks.len() as i32).collect::<Vec<_>>()
    );
    let mut stored = Vec::new();
    indexer
        .store
        .db
        .query(
            "SELECT coalesce(fts.original, fts.body) FROM fts JOIN segments s ON s.id = fts.rowid \
             WHERE s.kind = 'chunk' ORDER BY s.loc",
            &[],
            |row| {
                stored.push(row.text(0).unwrap_or_default());
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(stored, chunks);
    let places = found(&indexer.store, "axolotl");
    assert!(!places.is_empty() && places.iter().all(|p| *p > 40));
    assert_eq!(state(&indexer.store, &path).0, "done");
}

#[test]
fn digits_get_keyword_search_only() {
    let h = Harness::new();
    let mut dump = String::new();
    for row in 0..4_000 {
        dump.push_str(&format!(
            "{} {} {}\n",
            row,
            row * 7919 % 100_003,
            row * 31 % 977
        ));
    }
    dump.push_str("4815162342\n");
    std::fs::write(h.files.join("sensor log.txt"), &dump).unwrap();
    assert!(!SearchText::has_words(&dump));
    assert!(SearchText::has_words(
        "Revenue 1,234,567 · Net income 234,567 · Margin 19% in the third quarter"
    ));
    assert!(SearchText::has_words("কলকাতায় বৃষ্টি হচ্ছে"));
    let embedder = FakeEmbedder::new();
    let mut indexer = h.indexer(h.store(), embedder.clone(), IndexOptions::default());
    indexer.sync(std::slice::from_ref(&h.files)).unwrap();
    run(&mut indexer);
    indexer.finish_long_files(&|| false, |_| {}).unwrap();
    assert_eq!(embedder.texts(), 0);
    let mut vectors = -1;
    indexer
        .store
        .db
        .query("SELECT COUNT(vec) FROM segments", &[], |row| {
            vectors = row.int(0);
            Ok(())
        })
        .unwrap();
    assert_eq!(vectors, 0);
    assert_eq!(found(&indexer.store, "4815162342").len(), 1);
}

#[test]
fn machine_made_giants_stop_at_the_cap() {
    let h = Harness::new();
    let pdf = make_pdf(&h.files, "Export.pdf", 120, &[]);
    let path = pdf.to_string_lossy().into_owned();
    let options = IndexOptions {
        max_pages: 45,
        ..IndexOptions::default()
    };
    let mut indexer = h.indexer(h.store(), FakeEmbedder::new(), options);
    indexer.sync(std::slice::from_ref(&h.files)).unwrap();
    run(&mut indexer);
    assert_eq!(indexer.store.unfinished_count().unwrap(), (1, 15));
    indexer.finish_long_files(&|| false, |_| {}).unwrap();
    assert_eq!(
        places(&indexer.store, &path, &["page", "page_image", "chunk"]),
        (1..=45).collect::<Vec<_>>()
    );
    assert_eq!(state(&indexer.store, &path).0, "done");
    let mut detail = None;
    indexer
        .store
        .db
        .query("SELECT detail FROM files", &[], |row| {
            detail = row.text(0);
            Ok(())
        })
        .unwrap();
    assert_eq!(detail.as_deref(), Some("the first 45 of 120 pages"));
}

#[test]
fn old_indexes_finish_what_they_cut() {
    let h = Harness::new();
    {
        let store = h.store();
        store
            .insert(&Candidate {
                path: "/books/atlas.pdf".into(),
                kind: FileKind::Pdf,
                size: 10,
                mtime: 1.0,
                inode: 1,
                device: 1,
            })
            .unwrap();
        store
            .insert(&Candidate {
                path: "/notes/long.txt".into(),
                kind: FileKind::Doc,
                size: 10,
                mtime: 1.0,
                inode: 2,
                device: 1,
            })
            .unwrap();
        let short = store
            .insert(&Candidate {
                path: "/notes/short.txt".into(),
                kind: FileKind::Doc,
                size: 10,
                mtime: 1.0,
                inode: 3,
                device: 1,
            })
            .unwrap();
        let dump = store
            .insert(&Candidate {
                path: "/data/dump.txt".into(),
                kind: FileKind::Doc,
                size: 10,
                mtime: 1.0,
                inode: 4,
                device: 1,
            })
            .unwrap();
        store
            .add_segment(
                short,
                SegmentKind::Chunk,
                Modality::Text,
                Some(1.0),
                None,
                None,
                Some("Notes from the harbour meeting"),
                Some(&FakeEmbedder::vector_for("words")),
            )
            .unwrap();
        store
            .add_segment(
                dump,
                SegmentKind::Chunk,
                Modality::Text,
                Some(1.0),
                None,
                None,
                Some("1760000000 4 9153\n1760000060 5 17"),
                Some(&FakeEmbedder::vector_for("digits")),
            )
            .unwrap();
        store
            .db
            .execute(
                "
            UPDATE files SET state = 'partial', info = '{\"pages\":\"463\",\"scanned_pages\":\"4\"}' WHERE path = '/books/atlas.pdf';
            UPDATE files SET state = 'done', info = '{\"chars\":\"100000\",\"chunks\":\"40\"}' WHERE path = '/notes/long.txt';
            UPDATE files SET state = 'done', info = '{\"chars\":\"3000\",\"chunks\":\"2\"}' WHERE path = '/notes/short.txt';
            UPDATE files SET state = 'done', info = '{\"chars\":\"30\",\"chunks\":\"1\"}' WHERE path = '/data/dump.txt';
            PRAGMA user_version = 4;
            ",
            )
            .unwrap();
    }
    let migrated = h.store();
    let atlas = state(&migrated, "/books/atlas.pdf");
    assert_eq!(atlas.0, "partial");
    assert_eq!(atlas.1.get("pages").map(String::as_str), Some("463"));
    assert_eq!(atlas.1.get("scanned_pages").map(String::as_str), Some("4"));
    assert_eq!(atlas.1.get("read_to").map(String::as_str), Some("30"));
    assert_eq!(atlas.1.get("read_end").map(String::as_str), Some("463"));
    let long = state(&migrated, "/notes/long.txt");
    assert_eq!(long.0, "partial");
    assert_eq!(long.1.get("read_to").map(String::as_str), Some("40"));
    assert_eq!(long.1.get("read_end").map(String::as_str), Some("63"));
    assert_eq!(state(&migrated, "/notes/short.txt").0, "done");
    assert_eq!(migrated.unfinished_count().unwrap(), (2, 433 + 23));
    let mut vectors = Vec::new();
    migrated
        .db
        .query(
            "SELECT f.path FROM segments s JOIN files f ON f.id = s.file WHERE s.vec IS NOT NULL",
            &[],
            |row| {
                vectors.push(row.text(0).unwrap_or_default());
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(vectors, vec!["/notes/short.txt".to_string()]);
    assert_eq!(found(&migrated, "9153"), vec![1]);
}

#[test]
fn estimates_count_the_rest() {
    let h = Harness::new();
    let book = make_pdf(&h.files, "Book.pdf", 120, &[]);
    let short = make_pdf(&h.files, "Leaflet.pdf", 3, &[]);
    let candidates = [book, short]
        .into_iter()
        .map(|p| Candidate {
            path: p.to_string_lossy().into_owned(),
            kind: FileKind::Pdf,
            size: 10,
            mtime: 1.0,
            inode: 1,
            device: 1,
        })
        .collect::<Vec<_>>();
    let costs = Costs::default();
    let estimate = Estimator::estimate(&candidates, &IndexOptions::default(), &costs, 0);
    assert_eq!(estimate.pdf_pages, 33);
    assert_eq!(estimate.pdf_pages_total, 123);
    assert_eq!(estimate.long_files, 1);
    assert!((estimate.seconds - 33.0 * costs.pdf_page).abs() < 1e-9);
    assert!((estimate.later_seconds - 90.0 * costs.pdf_page).abs() < 1e-9);
}

#[test]
fn move_by_inode_does_not_reembed() {
    let h = Harness::new();
    std::fs::write(h.files.join("alpha.txt"), "the harbour meeting notes\n").unwrap();
    let embedder = FakeEmbedder::new();
    let mut indexer = h.indexer(h.store(), embedder.clone(), IndexOptions::default());
    indexer.sync(std::slice::from_ref(&h.files)).unwrap();
    run(&mut indexer);
    let texts = embedder.texts();
    std::fs::rename(h.files.join("alpha.txt"), h.files.join("beta.txt")).unwrap();
    let summary = indexer.sync(std::slice::from_ref(&h.files)).unwrap();
    assert_eq!(summary.moved, 1);
    assert_eq!(summary.added, 0);
    assert_eq!(summary.changed, 0);
    run(&mut indexer);
    assert_eq!(embedder.texts(), texts);
}

#[test]
fn touching_one_file_reembeds_only_it() {
    let h = Harness::new();
    std::fs::write(
        h.files.join("keep.txt"),
        "the river garden morning letter window\n",
    )
    .unwrap();
    std::fs::write(
        h.files.join("edit.txt"),
        "the harbour kitchen mountain station\n",
    )
    .unwrap();
    let embedder = FakeEmbedder::new();
    let mut indexer = h.indexer(h.store(), embedder.clone(), IndexOptions::default());
    indexer.sync(std::slice::from_ref(&h.files)).unwrap();
    run(&mut indexer);
    let texts = embedder.texts();
    std::fs::write(
        h.files.join("edit.txt"),
        "the harbour kitchen mountain station meadow extra\n",
    )
    .unwrap();
    let summary = indexer.sync(std::slice::from_ref(&h.files)).unwrap();
    assert_eq!(summary.changed, 1);
    assert_eq!(summary.added, 0);
    run(&mut indexer);
    assert_eq!(embedder.texts(), texts + 1);
}

#[test]
fn poison_file_is_skipped_on_the_second_attempt() {
    let h = Harness::new();
    std::fs::write(
        h.files.join("notes.txt"),
        "the library summer village lantern\n",
    )
    .unwrap();
    let embedder = FakeEmbedder::new();
    let mut indexer = h.indexer(h.store(), embedder.clone(), IndexOptions::default());
    indexer.sync(std::slice::from_ref(&h.files)).unwrap();
    let id = indexer.store.pending(None, None).unwrap()[0].id;
    assert_eq!(indexer.store.begin_attempt(id).unwrap(), 1);
    let states = run(&mut indexer);
    assert_eq!(states.get("failed").copied(), Some(1));
    assert_eq!(embedder.texts(), 0);
    let path = h.files.join("notes.txt");
    let (st, _) = state(&indexer.store, &path.to_string_lossy());
    assert_eq!(st, "failed");
    let mut detail = None;
    indexer
        .store
        .db
        .query("SELECT detail FROM files", &[], |row| {
            detail = row.text(0);
            Ok(())
        })
        .unwrap();
    assert_eq!(detail.as_deref(), Some("Couldn't be read safely"));
}

#[test]
fn vanished_root_is_not_deleted() {
    let h = Harness::new();
    let gone = h.folder.join("missing");
    std::fs::create_dir_all(&gone).unwrap();
    std::fs::write(gone.join("keep.txt"), "the village library summer\n").unwrap();
    let mut indexer = h.indexer(h.store(), FakeEmbedder::new(), IndexOptions::default());
    indexer.sync(std::slice::from_ref(&gone)).unwrap();
    run(&mut indexer);
    std::fs::remove_dir_all(&gone).unwrap();
    let summary = indexer.sync(&[gone]).unwrap();
    assert_eq!(summary.removed, 0);
    let mut files = 0;
    indexer
        .store
        .db
        .query("SELECT COUNT(*) FROM files", &[], |row| {
            files = row.int(0);
            Ok(())
        })
        .unwrap();
    assert_eq!(files, 1);
}
