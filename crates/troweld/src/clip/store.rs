//! Clipboard rows in `state.sqlite` and blobs in `<state dir>/clipboard/` (D20). The daemon is the
//! only writer, and every call runs under the engine's state lock, so a blob write and its row
//! insert can't interleave with a GC.

use std::fs;
use std::io::Write;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

use trowel_core::search_text::is_word_character;
use trowel_core::{DbError, SearchText, SqlValue};
use trowel_proto::{ClipKind, ClipRow, PathValue};

use crate::state::StateDb;

/// What an ingest or an import hands the store.
#[derive(Debug, Clone, Default)]
pub struct NewEntry {
    /// Content identity: re-copying the same content bumps the old row.
    pub hash: String,
    pub kind: Option<ClipKind>,
    pub preview: String,
    pub mimes: Vec<String>,
    pub bytes: u64,
    /// Searchable text (capped, see `ingest::BODY_CAP`).
    pub body: Option<String>,
    pub html: Option<String>,
    pub files: Option<Vec<PathValue>>,
    pub blob: Option<NewBlob>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub color: Option<String>,
    pub domain: Option<String>,
    pub source_app: Option<String>,
    pub at: i64,
}

#[derive(Debug, Clone)]
pub struct NewBlob {
    pub sha256: String,
    pub ext: String,
    pub data: BlobData,
}

#[derive(Debug, Clone)]
pub enum BlobData {
    Bytes(Vec<u8>),
    /// Copied, never moved (Omarchy's images stay where they are).
    CopyOf(PathBuf),
}

/// Everything `clip_get` / `clip_paste` need.
#[derive(Debug, Clone)]
pub struct Stored {
    pub row: ClipRow,
    pub body: Option<String>,
    pub html: Option<String>,
    pub files: Vec<PathValue>,
    pub blob: Option<PathBuf>,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Limits {
    pub max_entries: u32,
    pub max_age_days: u32,
    pub max_blob_bytes: u64,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct GcReport {
    pub blob_rows: u64,
    pub files: u64,
}

const COLUMNS: &str = "id, kind, preview, mimes, bytes, files, color, domain, source_app, \
                       created, last_used, uses, pinned, body, html, blob, width, height";

fn kind_of(text: &str) -> ClipKind {
    ClipKind::parse(text).unwrap_or(ClipKind::Text)
}

fn read_stored(row: &trowel_core::database::QueryRow<'_>, blob_dir: &Path) -> Stored {
    let mimes: Vec<String> = row
        .text(3)
        .and_then(|m| serde_json::from_str(&m).ok())
        .unwrap_or_default();
    let files: Vec<PathValue> = row
        .text(5)
        .and_then(|f| serde_json::from_str(&f).ok())
        .unwrap_or_default();
    let kind = kind_of(&row.text(1).unwrap_or_default());
    let blob = row.text(15).map(|sha| {
        let ext = blob_ext(&mimes, kind);
        blob_dir.join(format!("{sha}.{ext}"))
    });
    Stored {
        row: ClipRow {
            id: row.int(0),
            kind,
            preview: row.text(2).unwrap_or_default(),
            mimes,
            bytes: row.int(4).max(0) as u64,
            thumb: None,
            files: (!files.is_empty()).then_some(files.len() as u32),
            color: row.text(6),
            domain: row.text(7),
            source_app: row.text(8),
            created: row.int(9),
            last_used: row.int(10),
            uses: row.int(11).max(0) as u32,
            pinned: row.int(12) != 0,
        },
        body: row.text(13),
        html: row.text(14),
        files,
        blob,
        width: row.int(16).max(0) as u32,
        height: row.int(17).max(0) as u32,
    }
}

/// The blob's extension follows from what the entry is: an image's MIME, or `txt` for long text.
pub fn blob_ext(mimes: &[String], kind: ClipKind) -> String {
    if kind == ClipKind::Image {
        let mime = mimes.first().map(String::as_str).unwrap_or("image/png");
        return ext_for_mime(mime).to_string();
    }
    "txt".into()
}

pub fn ext_for_mime(mime: &str) -> &'static str {
    match mime {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        _ => "png",
    }
}

/// Every word as an FTS prefix, all required. Same normalization as file search.
pub fn fts_match(text: &str) -> Option<String> {
    let lowered = SearchText::normalized(text).to_lowercase();
    let words: Vec<String> = lowered
        .split(|c: char| !is_word_character(c))
        .filter(|w| !w.is_empty())
        .map(|w| format!("\"{w}\"*"))
        .collect();
    (!words.is_empty()).then(|| words.join(" AND "))
}

impl StateDb {
    pub fn blob_dir(&self) -> PathBuf {
        self.file
            .parent()
            .unwrap_or(Path::new("."))
            .join("clipboard")
    }

    /// Inserts a new entry or bumps the existing one with the same content. Returns the row and
    /// whether it is new.
    pub fn clip_upsert(&self, entry: &NewEntry) -> Result<(ClipRow, bool), DbError> {
        let blob_dir = self.blob_dir();
        self.db.transaction(|| {
            let mut existing = None;
            self.db.query(
                "SELECT id FROM clip_entries WHERE hash = ?1",
                &[SqlValue::Text(entry.hash.clone())],
                |row| {
                    existing = Some(row.int(0));
                    Ok(())
                },
            )?;
            if let Some(id) = existing {
                self.db.run(
                    "UPDATE clip_entries SET last_used = max(last_used, ?2), uses = uses + 1,
                        source_app = coalesce(?3, source_app),
                        seq = (SELECT coalesce(max(seq), 0) + 1 FROM clip_entries) WHERE id = ?1",
                    &[
                        SqlValue::Int(id),
                        SqlValue::Int(entry.at),
                        SqlValue::optional_text(entry.source_app.clone()),
                    ],
                )?;
                let row = self.clip_stored_in(id, &blob_dir)?.map(|s| s.row);
                return row
                    .map(|r| (r, false))
                    .ok_or_else(|| DbError::new("entry vanished"));
            }
            let blob_sha = match &entry.blob {
                Some(blob) => {
                    write_blob(&blob_dir, blob).map_err(|e| DbError::new(e.to_string()))?;
                    let bytes = match &blob.data {
                        BlobData::Bytes(b) => b.len() as u64,
                        BlobData::CopyOf(p) => fs::metadata(p).map(|m| m.len()).unwrap_or(0),
                    };
                    self.db.run(
                        "INSERT INTO clip_blobs(sha256, ext, bytes, refcount) VALUES (?1, ?2, ?3, 1)
                         ON CONFLICT(sha256) DO UPDATE SET refcount = refcount + 1",
                        &[
                            SqlValue::Text(blob.sha256.clone()),
                            SqlValue::Text(blob.ext.clone()),
                            SqlValue::Int(bytes as i64),
                        ],
                    )?;
                    Some(blob.sha256.clone())
                }
                None => None,
            };
            let kind = entry.kind.unwrap_or(ClipKind::Text);
            let files = entry
                .files
                .as_ref()
                .map(|f| serde_json::to_string(f).unwrap_or_else(|_| "[]".into()));
            self.db.run(
                "INSERT INTO clip_entries(hash, kind, preview, mimes, bytes, body, html, files, blob,
                    width, height, color, domain, source_app, created, last_used, uses, pinned, seq)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?15, 1, 0,
                    (SELECT coalesce(max(seq), 0) + 1 FROM clip_entries))",
                &[
                    SqlValue::Text(entry.hash.clone()),
                    SqlValue::Text(kind.as_str().into()),
                    SqlValue::Text(entry.preview.clone()),
                    SqlValue::Text(serde_json::to_string(&entry.mimes).unwrap_or_default()),
                    SqlValue::Int(entry.bytes as i64),
                    SqlValue::optional_text(entry.body.clone()),
                    SqlValue::optional_text(entry.html.clone()),
                    SqlValue::optional_text(files),
                    SqlValue::optional_text(blob_sha),
                    entry
                        .width
                        .map_or(SqlValue::Null, |w| SqlValue::Int(i64::from(w))),
                    entry
                        .height
                        .map_or(SqlValue::Null, |h| SqlValue::Int(i64::from(h))),
                    SqlValue::optional_text(entry.color.clone()),
                    SqlValue::optional_text(entry.domain.clone()),
                    SqlValue::optional_text(entry.source_app.clone()),
                    SqlValue::Int(entry.at),
                ],
            )?;
            let id = self.db.last_insert_id();
            let row = self.clip_stored_in(id, &blob_dir)?.map(|s| s.row);
            row.map(|r| (r, true))
                .ok_or_else(|| DbError::new("entry vanished"))
        })
    }

    fn clip_stored_in(&self, id: i64, blob_dir: &Path) -> Result<Option<Stored>, DbError> {
        let mut out = None;
        self.db.query(
            &format!("SELECT {COLUMNS} FROM clip_entries WHERE id = ?1"),
            &[SqlValue::Int(id)],
            |row| {
                out = Some(read_stored(row, blob_dir));
                Ok(())
            },
        )?;
        Ok(out)
    }

    pub fn clip_has(&self, hash: &str) -> Result<bool, DbError> {
        Ok(!self
            .ids(
                "SELECT id FROM clip_entries WHERE hash = ?1",
                &[SqlValue::Text(hash.into())],
            )?
            .is_empty())
    }

    pub fn clip_stored(&self, id: i64) -> Result<Option<Stored>, DbError> {
        self.clip_stored_in(id, &self.blob_dir())
    }

    /// Pinned first, then most recently used. `text` matches words (FTS prefix, AND) in the
    /// preview or body, or a literal substring of the preview (`#F2A`, `://`).
    pub fn clip_query(
        &self,
        text: &str,
        kinds: Option<&[ClipKind]>,
        limit: u32,
        offset: u32,
    ) -> Result<(Vec<ClipRow>, u64), DbError> {
        let mut filters = Vec::new();
        let mut values = Vec::new();
        if let Some(kinds) = kinds.filter(|k| !k.is_empty()) {
            let list: Vec<String> = kinds.iter().map(|k| format!("'{}'", k.as_str())).collect();
            filters.push(format!("kind IN ({})", list.join(",")));
        }
        let needle = text.trim();
        if !needle.is_empty() {
            values.push(SqlValue::Text(needle.to_lowercase()));
            let literal = "instr(lower(preview), ?1) > 0";
            match fts_match(needle) {
                Some(fts) => {
                    values.push(SqlValue::Text(fts));
                    filters.push(format!(
                        "({literal} OR id IN (SELECT rowid FROM clip_fts WHERE clip_fts MATCH ?2))"
                    ));
                }
                None => filters.push(literal.into()),
            }
        }
        let filter = if filters.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", filters.join(" AND "))
        };
        let mut total = 0u64;
        self.db.query(
            &format!("SELECT COUNT(*) FROM clip_entries {filter}"),
            &values,
            |row| {
                total = row.int(0).max(0) as u64;
                Ok(())
            },
        )?;
        let n = values.len();
        values.push(SqlValue::Int(i64::from(limit)));
        values.push(SqlValue::Int(i64::from(offset)));
        let blob_dir = self.blob_dir();
        let mut rows = Vec::new();
        self.db.query(
            &format!(
                "SELECT {COLUMNS} FROM clip_entries {filter}
                 ORDER BY pinned DESC, seq DESC LIMIT ?{} OFFSET ?{}",
                n + 1,
                n + 2
            ),
            &values,
            |row| {
                rows.push(read_stored(row, &blob_dir).row);
                Ok(())
            },
        )?;
        Ok((rows, total))
    }

    /// A paste or copy from the history moves the entry to the top.
    pub fn clip_touch(&self, id: i64, at: i64) -> Result<Option<ClipRow>, DbError> {
        self.db.run(
            "UPDATE clip_entries SET last_used = max(last_used, ?2), uses = uses + 1,
                seq = (SELECT coalesce(max(seq), 0) + 1 FROM clip_entries) WHERE id = ?1",
            &[SqlValue::Int(id), SqlValue::Int(at)],
        )?;
        Ok(self.clip_stored(id)?.map(|s| s.row))
    }

    pub fn clip_pin(&self, id: i64, pinned: bool) -> Result<Option<ClipRow>, DbError> {
        self.db.run(
            "UPDATE clip_entries SET pinned = ?2 WHERE id = ?1",
            &[SqlValue::Int(id), SqlValue::Int(i64::from(pinned))],
        )?;
        Ok(self.clip_stored(id)?.map(|s| s.row))
    }

    /// Deletes rows and releases their blobs; returns the ids that existed.
    fn clip_remove(&self, ids: &[i64]) -> Result<Vec<i64>, DbError> {
        let mut removed = Vec::new();
        let mut dead_files = Vec::new();
        let blob_dir = self.blob_dir();
        self.db.transaction(|| {
            for &id in ids {
                let mut found = None;
                self.db.query(
                    "SELECT blob FROM clip_entries WHERE id = ?1",
                    &[SqlValue::Int(id)],
                    |row| {
                        found = Some(row.text(0));
                        Ok(())
                    },
                )?;
                let Some(blob) = found else { continue };
                self.db.run(
                    "DELETE FROM clip_entries WHERE id = ?1",
                    &[SqlValue::Int(id)],
                )?;
                if let Some(sha) = blob {
                    self.db.run(
                        "UPDATE clip_blobs SET refcount = refcount - 1 WHERE sha256 = ?1",
                        &[SqlValue::Text(sha.clone())],
                    )?;
                    let mut gone = None;
                    self.db.query(
                        "SELECT ext FROM clip_blobs WHERE sha256 = ?1 AND refcount <= 0",
                        &[SqlValue::Text(sha.clone())],
                        |row| {
                            gone = row.text(0);
                            Ok(())
                        },
                    )?;
                    if let Some(ext) = gone {
                        self.db.run(
                            "DELETE FROM clip_blobs WHERE sha256 = ?1",
                            &[SqlValue::Text(sha.clone())],
                        )?;
                        dead_files.push(blob_dir.join(format!("{sha}.{ext}")));
                    }
                }
                removed.push(id);
            }
            Ok(())
        })?;
        // After the commit: a rollback must never leave a row whose file is gone.
        for file in dead_files {
            let _ = fs::remove_file(file);
        }
        Ok(removed)
    }

    pub fn clip_delete(&self, id: i64) -> Result<bool, DbError> {
        Ok(!self.clip_remove(&[id])?.is_empty())
    }

    pub fn clip_clear(&self, keep_pinned: bool) -> Result<Vec<i64>, DbError> {
        let sql = if keep_pinned {
            "SELECT id FROM clip_entries WHERE pinned = 0"
        } else {
            "SELECT id FROM clip_entries"
        };
        let ids = self.ids(sql, &[])?;
        self.clip_remove(&ids)
    }

    fn ids(&self, sql: &str, values: &[SqlValue]) -> Result<Vec<i64>, DbError> {
        let mut ids = Vec::new();
        self.db.query(sql, values, |row| {
            ids.push(row.int(0));
            Ok(())
        })?;
        Ok(ids)
    }

    /// Age, count and blob-size caps. Pinned entries are never evicted. Returns removed ids.
    pub fn clip_enforce(&self, limits: Limits, now: i64) -> Result<Vec<i64>, DbError> {
        let cutoff = now - i64::from(limits.max_age_days) * 86_400;
        let mut doomed = self.ids(
            "SELECT id FROM clip_entries WHERE pinned = 0 AND last_used < ?1",
            &[SqlValue::Int(cutoff)],
        )?;
        let mut pinned = 0i64;
        self.db.query(
            "SELECT COUNT(*) FROM clip_entries WHERE pinned = 1",
            &[],
            |row| {
                pinned = row.int(0);
                Ok(())
            },
        )?;
        let keep = (i64::from(limits.max_entries) - pinned).max(0);
        for id in self.ids(
            "SELECT id FROM clip_entries WHERE pinned = 0 AND last_used >= ?1
             ORDER BY seq DESC LIMIT -1 OFFSET ?2",
            &[SqlValue::Int(cutoff), SqlValue::Int(keep)],
        )? {
            if !doomed.contains(&id) {
                doomed.push(id);
            }
        }
        let mut removed = self.clip_remove(&doomed)?;
        // Blob bytes: drop the oldest unpinned entries that hold a blob until under the cap.
        let mut total = 0i64;
        self.db.query(
            "SELECT coalesce(sum(bytes), 0) FROM clip_blobs",
            &[],
            |row| {
                total = row.int(0);
                Ok(())
            },
        )?;
        if total as u64 > limits.max_blob_bytes {
            let mut over = total - limits.max_blob_bytes as i64;
            let mut victims = Vec::new();
            self.db.query(
                "SELECT e.id, b.bytes FROM clip_entries e JOIN clip_blobs b ON b.sha256 = e.blob
                 WHERE e.pinned = 0 ORDER BY e.seq ASC",
                &[],
                |row| {
                    if over > 0 {
                        victims.push(row.int(0));
                        over -= row.int(1);
                    }
                    Ok(())
                },
            )?;
            removed.extend(self.clip_remove(&victims)?);
        }
        Ok(removed)
    }

    /// Heals refcounts from the rows, drops unreferenced blob rows, and deletes files in the blob
    /// directory that no row names (a crash between the blob write and the insert).
    pub fn clip_gc(&self) -> Result<GcReport, DbError> {
        let mut report = GcReport::default();
        let mut live = std::collections::HashSet::new();
        self.db.transaction(|| {
            self.db.execute(
                "UPDATE clip_blobs SET refcount =
                    (SELECT COUNT(*) FROM clip_entries e WHERE e.blob = clip_blobs.sha256)",
            )?;
            let mut dead = 0u64;
            self.db.query(
                "SELECT COUNT(*) FROM clip_blobs WHERE refcount <= 0",
                &[],
                |row| {
                    dead = row.int(0).max(0) as u64;
                    Ok(())
                },
            )?;
            report.blob_rows = dead;
            self.db
                .execute("DELETE FROM clip_blobs WHERE refcount <= 0")?;
            self.db
                .query("SELECT sha256, ext FROM clip_blobs", &[], |row| {
                    live.insert(format!(
                        "{}.{}",
                        row.text(0).unwrap_or_default(),
                        row.text(1).unwrap_or_default()
                    ));
                    Ok(())
                })?;
            Ok(())
        })?;
        if let Ok(read) = fs::read_dir(self.blob_dir()) {
            for item in read.filter_map(Result::ok) {
                let name = item.file_name().to_string_lossy().into_owned();
                if !live.contains(&name) && fs::remove_file(item.path()).is_ok() {
                    report.files += 1;
                }
            }
        }
        Ok(report)
    }

    /// `(entries, blob bytes)` for the doctor and the bench.
    pub fn clip_stats(&self) -> Result<(u64, u64), DbError> {
        let mut out = (0, 0);
        self.db.query(
            "SELECT (SELECT COUNT(*) FROM clip_entries),
                    (SELECT coalesce(sum(bytes), 0) FROM clip_blobs)",
            &[],
            |row| {
                out = (row.int(0).max(0) as u64, row.int(1).max(0) as u64);
                Ok(())
            },
        )?;
        Ok(out)
    }

    /// Every entry id, for the thumbnail sweep.
    pub fn clip_ids(&self) -> Result<Vec<i64>, DbError> {
        self.ids("SELECT id FROM clip_entries", &[])
    }
}

/// `<dir>/<sha>.<ext>`, written once (`0700` dir, `0600` file) through a temp file and a rename,
/// so a crash never leaves a half blob under its final name.
fn write_blob(dir: &Path, blob: &NewBlob) -> std::io::Result<()> {
    let dest = dir.join(format!("{}.{}", blob.sha256, blob.ext));
    if dest.is_file() {
        return Ok(());
    }
    fs::create_dir_all(dir)?;
    let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
    let tmp = dir.join(format!(".{}.{}.tmp", blob.sha256, std::process::id()));
    {
        let mut out = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        match &blob.data {
            BlobData::Bytes(bytes) => out.write_all(bytes)?,
            BlobData::CopyOf(src) => {
                std::io::copy(&mut fs::File::open(src)?, &mut out)?;
            }
        }
        out.sync_data()?;
    }
    fs::rename(&tmp, &dest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn state() -> StateDb {
        let dir = std::env::temp_dir().join(format!(
            "trowel-clipstore-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        StateDb::open(&dir).unwrap()
    }

    fn text(t: &str, at: i64) -> NewEntry {
        NewEntry {
            hash: format!("t:{t}"),
            kind: Some(ClipKind::Text),
            preview: t.into(),
            mimes: vec!["text/plain".into()],
            bytes: t.len() as u64,
            body: Some(t.into()),
            at,
            ..Default::default()
        }
    }

    fn image(seed: u8, size: usize, at: i64) -> NewEntry {
        let sha = format!("{seed:064x}");
        NewEntry {
            hash: format!("i:{sha}"),
            kind: Some(ClipKind::Image),
            preview: "Image".into(),
            mimes: vec!["image/png".into()],
            bytes: size as u64,
            blob: Some(NewBlob {
                sha256: sha,
                ext: "png".into(),
                data: BlobData::Bytes(vec![seed; size]),
            }),
            at,
            ..Default::default()
        }
    }

    fn ids(rows: &[ClipRow]) -> Vec<i64> {
        rows.iter().map(|r| r.id).collect()
    }

    #[test]
    fn recopy_bumps_instead_of_duplicating() {
        let s = state();
        // All in the same second: order must still follow the copies.
        let (a, new_a) = s.clip_upsert(&text("alpha", 10)).unwrap();
        let (b, _) = s.clip_upsert(&text("beta", 10)).unwrap();
        assert!(new_a);
        let (again, new) = s.clip_upsert(&text("alpha", 10)).unwrap();
        assert!(!new);
        assert_eq!(again.id, a.id);
        assert_eq!((again.uses, again.last_used), (2, 10));
        assert_eq!(again.created, 10);
        let (rows, total) = s.clip_query("", None, 50, 0).unwrap();
        assert_eq!(total, 2);
        assert_eq!(ids(&rows), vec![a.id, b.id], "bumped to the top");
    }

    #[test]
    fn query_orders_pins_first_and_filters() {
        let s = state();
        let (a, _) = s
            .clip_upsert(&text("firewall rules for staging", 1))
            .unwrap();
        let (b, _) = s.clip_upsert(&text("lunch menu", 2)).unwrap();
        let mut link = text("https://github.com/x", 3);
        link.kind = Some(ClipKind::Link);
        let (c, _) = s.clip_upsert(&link).unwrap();
        s.clip_pin(a.id, true).unwrap();
        let (rows, _) = s.clip_query("", None, 50, 0).unwrap();
        assert_eq!(ids(&rows), vec![a.id, c.id, b.id]);
        let (rows, total) = s.clip_query("fire rul", None, 50, 0).unwrap();
        assert_eq!((ids(&rows), total), (vec![a.id], 1), "prefix words, AND");
        let (rows, _) = s.clip_query("fire lunch", None, 50, 0).unwrap();
        assert!(rows.is_empty());
        let (rows, _) = s.clip_query("://", None, 50, 0).unwrap();
        assert_eq!(ids(&rows), vec![c.id], "literal substring of the preview");
        let (rows, _) = s.clip_query("", Some(&[ClipKind::Link]), 50, 0).unwrap();
        assert_eq!(ids(&rows), vec![c.id]);
        let (rows, total) = s.clip_query("", None, 1, 1).unwrap();
        assert_eq!((ids(&rows), total), (vec![c.id], 3), "paged");
    }

    #[test]
    fn retention_by_count_and_age_spares_pins() {
        let s = state();
        let day = 86_400;
        let (old, _) = s.clip_upsert(&text("old", 0)).unwrap();
        let (pinned_old, _) = s.clip_upsert(&text("pinned old", 1)).unwrap();
        s.clip_pin(pinned_old.id, true).unwrap();
        for i in 0..5 {
            s.clip_upsert(&text(&format!("n{i}"), 40 * day + i))
                .unwrap();
        }
        let limits = Limits {
            max_entries: 4,
            max_age_days: 30,
            max_blob_bytes: u64::MAX,
        };
        let removed = s.clip_enforce(limits, 40 * day + 10).unwrap();
        assert!(removed.contains(&old.id), "too old");
        let (rows, total) = s.clip_query("", None, 50, 0).unwrap();
        assert_eq!(total, 4, "pinned + the three newest");
        assert_eq!(rows[0].id, pinned_old.id);
        assert_eq!(rows[1].preview, "n4");
        assert_eq!(rows[3].preview, "n2");
    }

    #[test]
    fn blob_cap_and_refcounts() {
        let s = state();
        let (a, _) = s.clip_upsert(&image(1, 600, 1)).unwrap();
        let (b, _) = s.clip_upsert(&image(2, 600, 2)).unwrap();
        let (c, _) = s.clip_upsert(&image(3, 600, 3)).unwrap();
        s.clip_pin(a.id, true).unwrap();
        assert_eq!(s.clip_stats().unwrap(), (3, 1800));
        let limits = Limits {
            max_entries: 100,
            max_age_days: 3650,
            max_blob_bytes: 1300,
        };
        let removed = s.clip_enforce(limits, 4).unwrap();
        assert_eq!(removed, vec![b.id], "oldest unpinned blob goes first");
        assert_eq!(s.clip_stats().unwrap(), (2, 1200));
        let files: Vec<_> = fs::read_dir(s.blob_dir()).unwrap().collect();
        assert_eq!(files.len(), 2, "its file went with it");
        assert!(s.clip_delete(c.id).unwrap());
        assert!(!s.clip_delete(c.id).unwrap());
        let stored = s.clip_stored(a.id).unwrap().unwrap();
        assert_eq!(fs::read(stored.blob.unwrap()).unwrap(), vec![1u8; 600]);
    }

    #[test]
    fn crash_between_blob_write_and_insert_leaves_no_orphan_after_gc() {
        let s = state();
        s.clip_upsert(&image(7, 100, 1)).unwrap();
        // A blob file written just before a crash: no row, no clip_blobs entry.
        let orphan = s.blob_dir().join(format!("{:064x}.png", 9));
        fs::write(&orphan, b"x").unwrap();
        fs::write(s.blob_dir().join(".dead.tmp"), b"x").unwrap();
        // A blob row whose entry was lost.
        s.db.execute(
            "INSERT INTO clip_blobs(sha256, ext, bytes, refcount) VALUES ('zz', 'png', 5, 3)",
        )
        .unwrap();
        let report = s.clip_gc().unwrap();
        assert_eq!(
            report,
            GcReport {
                blob_rows: 1,
                files: 2
            }
        );
        assert!(!orphan.exists());
        assert_eq!(fs::read_dir(s.blob_dir()).unwrap().count(), 1);
        assert_eq!(s.clip_stats().unwrap(), (1, 100));
    }

    #[test]
    fn clear_keeps_pins_unless_asked() {
        let s = state();
        let (a, _) = s.clip_upsert(&text("a", 1)).unwrap();
        s.clip_upsert(&image(4, 10, 2)).unwrap();
        s.clip_pin(a.id, true).unwrap();
        assert_eq!(s.clip_clear(true).unwrap().len(), 1);
        assert_eq!(s.clip_stats().unwrap(), (1, 0));
        assert_eq!(s.clip_clear(false).unwrap(), vec![a.id]);
        assert_eq!(fs::read_dir(s.blob_dir()).unwrap().count(), 0);
    }

    #[test]
    fn fts_query_shape() {
        assert_eq!(
            fts_match("Fire  rules!").as_deref(),
            Some("\"fire\"* AND \"rules\"*")
        );
        assert_eq!(fts_match("#!"), None);
    }
}
