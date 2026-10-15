//! Clipboard history (Phase 8 L3, D20/D21). Capture runs in `trowel clip-capture` under a
//! supervised `wl-paste --watch` ([`watch`]); it spools the offers and sends `clip_ingest`, and the
//! daemon builds the entry ([`ingest`]), stores it ([`store`]) and renders thumbnails ([`thumbs`]).

pub mod classify;
pub mod import;
pub mod ingest;
pub mod store;
pub mod thumbs;
pub mod watch;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use trowel_proto::{ClipContent, ClipImage, ClipKind, ClipRow, PasteMode, PathValue};

use crate::actions::{self, ClipSource};
use store::Stored;

/// `ClipRow` with its thumbnail when one has been rendered.
pub fn with_thumb(mut row: ClipRow) -> ClipRow {
    if matches!(row.kind, ClipKind::Image | ClipKind::Files) {
        row.thumb = thumbs::row_thumb(row.id);
    }
    row
}

/// `clip_get`: everything the preview column shows. Text and HTML are capped at
/// [`ingest::BODY_CAP`] (`truncated`).
pub fn content(stored: Stored) -> ClipContent {
    let truncated = stored.body.as_ref().is_some_and(|b| {
        stored.row.kind != ClipKind::Files
            && stored.row.kind != ClipKind::Html
            && (b.len() as u64) < stored.row.bytes
    }) || stored
        .html
        .as_ref()
        .is_some_and(|h| h.len() >= ingest::BODY_CAP);
    let image = (stored.row.kind == ClipKind::Image)
        .then_some(stored.blob.as_ref())
        .flatten()
        .map(|file| ClipImage {
            file: PathValue::from_path(file),
            w: stored.width,
            h: stored.height,
        });
    ClipContent {
        text: if stored.row.kind == ClipKind::Files || stored.row.kind == ClipKind::Image {
            None
        } else {
            stored.body
        },
        html: stored.html,
        files: (!stored.files.is_empty()).then_some(stored.files),
        image,
        truncated,
        row: with_thumb(stored.row),
    }
}

#[derive(Debug)]
pub enum RestoreError {
    /// The copied files are all gone, or the blob is missing.
    Gone(String),
    Io(String),
}

/// Puts an entry back on the clipboard with its stored MIME type (`LAUNCHER-PLAN.md` L3
/// "Restore"). `plain` drops HTML and turns a file list into its paths.
pub fn restore(stored: &Stored, mode: PasteMode) -> Result<(), RestoreError> {
    let io = |e: std::io::Error| RestoreError::Io(e.to_string());
    let plain = mode == PasteMode::Plain;
    match stored.row.kind {
        ClipKind::Image => {
            let blob = stored
                .blob
                .as_ref()
                .filter(|b| b.is_file())
                .ok_or_else(|| RestoreError::Gone("the image is no longer stored".into()))?;
            let mime = stored
                .row
                .mimes
                .first()
                .cloned()
                .unwrap_or_else(|| "image/png".into());
            actions::copy_as(&mime, ClipSource::File(blob.clone())).map_err(io)
        }
        ClipKind::Files => {
            let paths: Vec<PathBuf> = stored.files.iter().map(PathValue::to_path_buf).collect();
            if !paths.iter().any(|p| p.exists()) {
                return Err(RestoreError::Gone(
                    "the copied files no longer exist".into(),
                ));
            }
            if plain {
                let text: Vec<String> = paths
                    .iter()
                    .map(|p| p.to_string_lossy().into_owned())
                    .collect();
                return actions::copy_text(&text.join("\n")).map_err(io);
            }
            // L0.3: one `text/uri-list` offer pastes files into Chromium, Nautilus and Strata.
            let list: String = paths
                .iter()
                .map(|p| format!("{}\r\n", actions::file_uri(p)))
                .collect();
            actions::copy_as("text/uri-list", ClipSource::Bytes(list.into_bytes())).map_err(io)
        }
        ClipKind::Html if !plain => {
            let html = stored.html.clone().unwrap_or_default();
            actions::copy_as("text/html", ClipSource::Bytes(html.into_bytes())).map_err(io)
        }
        _ => {
            // Long text keeps its full copy in a `txt` blob; the row holds the first 256 KiB.
            match stored.blob.as_ref() {
                Some(blob) => {
                    if !blob.is_file() {
                        return Err(RestoreError::Gone("the text is no longer stored".into()));
                    }
                    actions::copy_as("text/plain;charset=utf-8", ClipSource::File(blob.clone()))
                        .map_err(io)
                }
                None => actions::copy_text(stored.body.as_deref().unwrap_or("")).map_err(io),
            }
        }
    }
}

/// Spool files must sit in the daemon's spool directory: `clip_ingest` never reads elsewhere.
pub fn spooled(spool: &Path, file: &Path) -> bool {
    let Ok(spool) = spool.canonicalize() else {
        return false;
    };
    file.canonicalize()
        .map(|f| f.parent() == Some(spool.as_path()))
        .unwrap_or(false)
}

/// `clipboard` events, at most 4 per second (PROTOCOL §4): the newest `added` row wins, and
/// `removed` ids accumulate until the next send.
#[derive(Clone)]
pub struct Events {
    inner: Arc<Mutex<Pending>>,
    send: Arc<dyn Fn(Value) + Send + Sync>,
}

#[derive(Default)]
struct Pending {
    added: Option<ClipRow>,
    removed: Vec<i64>,
    last: Option<Instant>,
    scheduled: bool,
}

const EVENT_GAP: Duration = Duration::from_millis(250);

impl Events {
    pub fn new(send: impl Fn(Value) + Send + Sync + 'static) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Pending::default())),
            send: Arc::new(send),
        }
    }

    pub fn added(&self, row: ClipRow) {
        self.push(Some(row), &[]);
    }

    pub fn removed(&self, ids: &[i64]) {
        if !ids.is_empty() {
            self.push(None, ids);
        }
    }

    fn push(&self, added: Option<ClipRow>, removed: &[i64]) {
        let mut p = self.inner.lock().expect("clip events");
        for id in removed {
            if p.added.as_ref().is_some_and(|r| r.id == *id) {
                p.added = None;
            }
            if !p.removed.contains(id) {
                p.removed.push(*id);
            }
        }
        if let Some(row) = added {
            p.removed.retain(|id| *id != row.id);
            p.added = Some(row);
        }
        if p.scheduled {
            return;
        }
        let wait = p
            .last
            .map(|t| EVENT_GAP.saturating_sub(t.elapsed()))
            .unwrap_or_default();
        if wait.is_zero() {
            Self::flush(&mut p, &self.send);
            return;
        }
        p.scheduled = true;
        let inner = Arc::clone(&self.inner);
        let send = Arc::clone(&self.send);
        thread::spawn(move || {
            thread::sleep(wait);
            let mut p = inner.lock().expect("clip events");
            p.scheduled = false;
            Self::flush(&mut p, &send);
        });
    }

    fn flush(p: &mut Pending, send: &Arc<dyn Fn(Value) + Send + Sync>) {
        if p.added.is_none() && p.removed.is_empty() {
            return;
        }
        let mut event = json!({"event": "clipboard"});
        if let Some(row) = p.added.take() {
            event["added"] = json!(row);
        }
        if !p.removed.is_empty() {
            event["removed"] = json!(std::mem::take(&mut p.removed));
        }
        p.last = Some(Instant::now());
        send(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64) -> ClipRow {
        ClipRow {
            id,
            kind: ClipKind::Text,
            preview: String::new(),
            mimes: vec![],
            bytes: 0,
            thumb: None,
            files: None,
            color: None,
            domain: None,
            source_app: None,
            created: 0,
            last_used: 0,
            uses: 1,
            pinned: false,
        }
    }

    #[test]
    fn events_coalesce_to_four_a_second() {
        let sent = Arc::new(Mutex::new(Vec::<Value>::new()));
        let sink = Arc::clone(&sent);
        let events = Events::new(move |v| sink.lock().unwrap().push(v));
        events.added(row(1));
        events.added(row(2));
        events.removed(&[5, 6]);
        events.added(row(3));
        assert_eq!(sent.lock().unwrap().len(), 1, "first one goes at once");
        thread::sleep(Duration::from_millis(400));
        let got = sent.lock().unwrap().clone();
        assert_eq!(got.len(), 2, "{got:?}");
        assert_eq!(got[0]["added"]["id"], 1);
        assert_eq!(got[1]["added"]["id"], 3, "newest added wins");
        assert_eq!(got[1]["removed"], json!([5, 6]));
        assert_eq!(got[1]["event"], "clipboard");
    }

    #[test]
    fn spool_check_rejects_other_dirs() {
        let dir = std::env::temp_dir().join(format!("trowel-spool-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("a"), b"x").unwrap();
        std::fs::write(dir.join("sub/b"), b"x").unwrap();
        assert!(spooled(&dir, &dir.join("a")));
        assert!(!spooled(&dir, &dir.join("sub/b")));
        assert!(!spooled(&dir, &dir.join("sub/../../etc/passwd")));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
