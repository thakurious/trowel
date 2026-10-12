//! On-disk index. `upstream/Sources/DigUpKit/IndexStore.swift`

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::database::{Database, DbError, SqlValue};
use crate::folder_selection::is_inside;
use crate::model::{Candidate, FileKind, IndexOptions, Modality, SegmentKind};
use crate::search_text::{PageTextProblem, SearchText};
use crate::vectors::Vectors;

/// A file as the index knows it.
/// `upstream/Sources/DigUpKit/IndexStore.swift:4`
#[derive(Debug, Clone)]
pub struct FileRecord {
    pub id: i64,
    pub path: String,
    pub kind: FileKind,
    pub size: i64,
    pub mtime: f64,
    pub inode: u64,
    pub device: u64,
    pub state: String,
}

/// `upstream/Sources/DigUpKit/IndexStore.swift:20`
pub struct IndexStore {
    pub directory: PathBuf,
    pub db: Database,
    pub file: PathBuf,
    scope_sql: String,
    scope_values: Vec<SqlValue>,
    scope: Option<Vec<String>>,
}

impl IndexStore {
    pub const SCHEMA_VERSION: i64 = 5;
    /// Keyword search's tokenizer. Marks (M*) count as letters.
    /// `upstream/Sources/DigUpKit/IndexStore.swift:27`
    pub const TOKENIZER: &'static str =
        "porter unicode61 remove_diacritics 2 categories 'L* N* Co M*'";
    pub const MAIN_FILE: &'static str = "index.sqlite";
    pub const CODE_FILE: &'static str = "code.sqlite";
    const FILE_COLUMNS: &'static str = "id, path, kind, size, mtime, inode, device, state";

    pub fn open(directory: impl AsRef<Path>) -> Result<Self, DbError> {
        Self::open_named(directory, Self::MAIN_FILE)
    }

    pub fn open_named(directory: impl AsRef<Path>, name: &str) -> Result<Self, DbError> {
        let directory = directory.as_ref().to_path_buf();
        std::fs::create_dir_all(&directory)
            .map_err(|e| DbError::new(format!("mkdir {}: {e}", directory.display())))?;
        let file = directory.join(name);
        let db = Database::open(&file)?;
        let store = Self {
            directory,
            db,
            file,
            scope_sql: String::new(),
            scope_values: Vec::new(),
            scope: None,
        };
        store.migrate()?;
        store.ensure_linux_tables()?;
        Ok(store)
    }

    pub fn set_scope(&mut self, scope: Option<Vec<String>>) {
        self.scope = scope.clone();
        match scope {
            None => {
                self.scope_sql.clear();
                self.scope_values.clear();
            }
            Some(scope) if scope.is_empty() => {
                self.scope_sql = " AND 0".into();
                self.scope_values.clear();
            }
            Some(scope) => {
                self.scope_sql = format!(
                    " AND ({})",
                    scope
                        .iter()
                        .map(|_| "(path >= ? AND path < ?)")
                        .collect::<Vec<_>>()
                        .join(" OR ")
                );
                self.scope_values = scope
                    .iter()
                    .flat_map(|root| {
                        let prefix = if root.ends_with('/') {
                            root.clone()
                        } else {
                            format!("{root}/")
                        };
                        [
                            SqlValue::Text(prefix.clone()),
                            SqlValue::Text(format!("{prefix}\u{10FFFF}")),
                        ]
                    })
                    .collect();
            }
        }
    }

    fn migrate(&self) -> Result<(), DbError> {
        let mut version = self.db.user_version()?;
        if version > Self::SCHEMA_VERSION {
            return Err(DbError::new(format!(
                "this index was made by a newer DigUp (schema {version})"
            )));
        }
        if version >= Self::SCHEMA_VERSION {
            return Ok(());
        }
        self.db.set_busy_timeout(60_000)?;
        let result = self.db.transaction(|| {
            version = self.db.user_version()?;
            if version == 0 {
                self.create()?;
            }
            if version == 1 {
                self.migrate_keyword_search()?;
            }
            if (1..=2).contains(&version) {
                self.migrate_screenshot_text()?;
            }
            if (1..=3).contains(&version) {
                self.migrate_vector_names()?;
            }
            if (1..=4).contains(&version) {
                self.migrate_long_files()?;
            }
            Ok(())
        });
        self.db.set_busy_timeout(5_000)?;
        result
    }

    fn create(&self) -> Result<(), DbError> {
        let fts = Self::fts_columns();
        self.db.execute(&format!(
            "
            CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE files(
                id INTEGER PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                kind TEXT NOT NULL,
                size INTEGER NOT NULL,
                mtime REAL NOT NULL,
                inode INTEGER NOT NULL,
                device INTEGER NOT NULL,
                state TEXT NOT NULL DEFAULT 'pending',
                detail TEXT,
                info TEXT,
                indexed_at REAL
            );
            CREATE INDEX files_inode ON files(device, inode);
            CREATE INDEX files_state ON files(state, kind);
            CREATE TABLE segments(
                id INTEGER PRIMARY KEY,
                file INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
                kind TEXT NOT NULL,
                modality TEXT NOT NULL,
                loc REAL,
                loc_end REAL,
                excerpt TEXT,
                vec BLOB
            );
            CREATE INDEX segments_file ON segments(file);
            CREATE VIRTUAL TABLE fts USING fts5({fts});
            PRAGMA user_version = {};
            ",
            Self::SCHEMA_VERSION
        ))
    }

    fn fts_columns() -> String {
        format!(
            "name, body, original UNINDEXED, tokenize = \"{}\"",
            Self::TOKENIZER
        )
    }

    fn migrate_keyword_search(&self) -> Result<(), DbError> {
        let fts = Self::fts_columns();
        self.db
            .execute(&format!("CREATE VIRTUAL TABLE fts_new USING fts5({fts})"))?;
        let mut rows: Vec<(i64, String, String)> = Vec::new();
        self.db
            .query("SELECT rowid, name, body FROM fts", &[], |row| {
                rows.push((
                    row.int(0),
                    row.text(1).unwrap_or_default(),
                    row.text(2).unwrap_or_default(),
                ));
                Ok(())
            })?;
        for (id, name, body) in rows {
            let mut values = vec![
                SqlValue::Int(id),
                SqlValue::Text(SearchText::normalized(&name)),
            ];
            values.extend(Self::keyword_text(&body));
            self.db.run(
                "INSERT INTO fts_new(rowid, name, body, original) VALUES(?, ?, ?, ?)",
                &values,
            )?;
        }
        let mut garbled = HashSet::new();
        self.db.query(
            "
            SELECT s.file, fts.body FROM fts JOIN segments s ON s.id = fts.rowid JOIN files f ON f.id = s.file
            WHERE f.kind = 'pdf' AND s.kind IN ('page', 'page_image')
            ",
            &[],
            |row| {
                if PageTextProblem::check(&row.text(1).unwrap_or_default()).is_some() {
                    garbled.insert(row.int(0));
                }
                Ok(())
            },
        )?;
        self.db.execute(
            "DROP TABLE fts; ALTER TABLE fts_new RENAME TO fts; PRAGMA user_version = 2;",
        )?;
        for file in garbled {
            self.requeue(file)?;
        }
        Ok(())
    }

    fn migrate_screenshot_text(&self) -> Result<(), DbError> {
        self.db.execute(
            "UPDATE segments SET vec = NULL, modality = 'none' WHERE kind = 'ocr'; PRAGMA user_version = 3;",
        )
    }

    fn migrate_vector_names(&self) -> Result<(), DbError> {
        self.db.execute(
            "
            UPDATE meta SET value = replace(value, ' llama.cpp-b11461-metal ', ' llama.cpp-vectors1 ')
            WHERE key IN ('fingerprint', 'vector_source');
            PRAGMA user_version = 4;
            ",
        )
    }

    fn migrate_long_files(&self) -> Result<(), DbError> {
        let mut digits = Vec::new();
        self.db.query(
            "
            SELECT s.id, coalesce(fts.original, fts.body) FROM segments s JOIN fts ON fts.rowid = s.id
            WHERE s.kind IN ('page', 'chunk') AND s.vec IS NOT NULL
            ",
            &[],
            |row| {
                if !SearchText::has_words(&row.text(1).unwrap_or_default()) {
                    digits.push(row.int(0));
                }
                Ok(())
            },
        )?;
        for id in digits {
            self.db.run(
                "UPDATE segments SET vec = NULL, modality = 'none' WHERE id = ?",
                &[SqlValue::Int(id)],
            )?;
        }
        let limits = IndexOptions::default();
        self.db.execute(&format!(
            "
            UPDATE files SET detail = 'read to page 30', info = json_set(info, '$.read_to', '30', '$.read_end',
                CAST(MIN(CAST(json_extract(info, '$.pages') AS INTEGER), {}) AS TEXT))
            WHERE kind = 'pdf' AND state = 'partial' AND info IS NOT NULL;
            UPDATE files SET state = 'partial', detail = 'read to chunk 40', info = json_set(info, '$.read_to', '40',
                '$.read_end', CAST(MIN(MAX(41, (CAST(json_extract(info, '$.chars') AS INTEGER) + 1599) / 1600),
                                       {}) AS TEXT))
            WHERE kind = 'doc' AND state = 'done' AND json_extract(info, '$.chunks') = '40';
            PRAGMA user_version = 5;
            ",
            limits.max_pages, limits.max_chunks
        ))
    }

    /// Linux-only tables; must not change `user_version` (DECISIONS D16).
    fn ensure_linux_tables(&self) -> Result<(), DbError> {
        self.db.execute(
            "
            CREATE TABLE IF NOT EXISTS linux_raw_paths(
                file_id INTEGER PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
                raw BLOB NOT NULL
            );
            CREATE TABLE IF NOT EXISTS linux_attempts(
                file_id INTEGER PRIMARY KEY REFERENCES files(id) ON DELETE CASCADE,
                n INTEGER NOT NULL,
                last_at REAL NOT NULL
            );
            ",
        )
    }

    pub fn meta(&self, key: &str) -> Result<Option<String>, DbError> {
        let mut value = None;
        self.db.query(
            "SELECT value FROM meta WHERE key = ?",
            &[SqlValue::Text(key.into())],
            |row| {
                value = row.text(0);
                Ok(())
            },
        )?;
        Ok(value)
    }

    pub fn set_meta(&self, key: &str, value: Option<&str>) -> Result<(), DbError> {
        if let Some(value) = value {
            self.db.run(
                "INSERT INTO meta(key, value) VALUES(?, ?) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                &[SqlValue::Text(key.into()), SqlValue::Text(value.into())],
            )
        } else {
            self.db.run(
                "DELETE FROM meta WHERE key = ?",
                &[SqlValue::Text(key.into())],
            )
        }
    }

    fn record(row: &crate::database::QueryRow<'_>) -> FileRecord {
        FileRecord {
            id: row.int(0),
            path: row.text(1).unwrap_or_default(),
            kind: row
                .text(2)
                .as_deref()
                .and_then(|s| s.parse().ok())
                .unwrap_or(FileKind::Doc),
            size: row.int(3),
            mtime: row.double(4),
            inode: row.int(5) as u64,
            device: row.int(6) as u64,
            state: row.text(7).unwrap_or_else(|| "pending".into()),
        }
    }

    pub fn files_under(&self, roots: &[String]) -> Result<Vec<FileRecord>, DbError> {
        let mut out = Vec::new();
        for root in roots {
            if roots
                .iter()
                .any(|other| other != root && is_inside(root, other))
            {
                continue;
            }
            let prefix = if root.ends_with('/') {
                root.clone()
            } else {
                format!("{root}/")
            };
            self.db.query(
                &format!(
                    "SELECT {} FROM files WHERE path >= ? AND path < ?",
                    Self::FILE_COLUMNS
                ),
                &[
                    SqlValue::Text(prefix.clone()),
                    SqlValue::Text(format!("{prefix}\u{10FFFF}")),
                ],
                |row| {
                    out.push(Self::record(row));
                    Ok(())
                },
            )?;
        }
        Ok(out)
    }

    pub fn insert(&self, file: &Candidate) -> Result<i64, DbError> {
        self.db.run(
            "INSERT INTO files(path, kind, size, mtime, inode, device) VALUES(?, ?, ?, ?, ?, ?)",
            &[
                SqlValue::Text(file.path.clone()),
                SqlValue::Text(file.kind.as_str().into()),
                SqlValue::Int(file.size),
                SqlValue::Double(file.mtime),
                SqlValue::Int(file.inode as i64),
                SqlValue::Int(file.device as i64),
            ],
        )?;
        let id = self.db.last_insert_id();
        self.add_name_row(id, &file.path)?;
        Ok(id)
    }

    pub fn requeue_as(&self, id: i64, file: &Candidate) -> Result<(), DbError> {
        self.delete_segments(id, true)?;
        self.db.run(
            "
            UPDATE files SET kind = ?, size = ?, mtime = ?, inode = ?, device = ?, state = 'pending', detail = NULL,
                info = NULL, indexed_at = NULL WHERE id = ?
            ",
            &[
                SqlValue::Text(file.kind.as_str().into()),
                SqlValue::Int(file.size),
                SqlValue::Double(file.mtime),
                SqlValue::Int(file.inode as i64),
                SqlValue::Int(file.device as i64),
                SqlValue::Int(id),
            ],
        )
    }

    fn requeue(&self, id: i64) -> Result<(), DbError> {
        self.delete_segments(id, true)?;
        self.db.run(
            "UPDATE files SET state = 'pending', detail = NULL, info = NULL, indexed_at = NULL WHERE id = ?",
            &[SqlValue::Int(id)],
        )
    }

    pub fn move_to(&self, id: i64, path: &str) -> Result<(), DbError> {
        self.db.run(
            "UPDATE files SET path = ? WHERE id = ?",
            &[SqlValue::Text(path.into()), SqlValue::Int(id)],
        )?;
        self.db.run(
            "
            UPDATE fts SET name = ?, body = ? WHERE rowid IN (SELECT id FROM segments WHERE file = ? AND kind = 'name')
            ",
            &[
                SqlValue::Text(SearchText::normalized(&Self::name_text(path))),
                SqlValue::Text(SearchText::normalized(&Self::folder_text(path))),
                SqlValue::Int(id),
            ],
        )
    }

    pub fn delete(&self, id: i64) -> Result<(), DbError> {
        self.db.run(
            "DELETE FROM fts WHERE rowid IN (SELECT id FROM segments WHERE file = ?)",
            &[SqlValue::Int(id)],
        )?;
        self.db
            .run("DELETE FROM files WHERE id = ?", &[SqlValue::Int(id)])
    }

    pub fn prune(&self, keeping_under: &[String]) -> Result<usize, DbError> {
        let prefixes: Vec<String> = keeping_under
            .iter()
            .map(|r| {
                if r.ends_with('/') {
                    r.clone()
                } else {
                    format!("{r}/")
                }
            })
            .collect();
        let mut gone = Vec::new();
        self.db.query("SELECT id, path FROM files", &[], |row| {
            let path = row.text(1).unwrap_or_default();
            if !prefixes.iter().any(|p| path.starts_with(p)) {
                gone.push(row.int(0));
            }
            Ok(())
        })?;
        if gone.is_empty() {
            return Ok(0);
        }
        let n = gone.len();
        self.db.transaction(|| {
            for id in gone {
                self.delete(id)?;
            }
            Ok(())
        })?;
        Ok(n)
    }

    pub fn pending(
        &self,
        limit: Option<i64>,
        only: Option<&HashSet<i64>>,
    ) -> Result<Vec<FileRecord>, DbError> {
        if matches!(only, Some(ids) if ids.is_empty()) {
            return Ok(Vec::new());
        }
        let filter = only
            .map(|ids| {
                format!(
                    " AND id IN ({})",
                    ids.iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join(",")
                )
            })
            .unwrap_or_default();
        let sql = format!(
            "SELECT {} FROM files WHERE state = 'pending'{}{} ORDER BY {}, mtime DESC LIMIT ?",
            Self::FILE_COLUMNS,
            filter,
            self.scope_sql,
            FileKind::INDEXING_ORDER_SQL
        );
        let mut values = self.scope_values.clone();
        values.push(SqlValue::Int(limit.unwrap_or(-1)));
        let mut out = Vec::new();
        self.db.query(&sql, &values, |row| {
            out.push(Self::record(row));
            Ok(())
        })?;
        Ok(out)
    }

    pub fn pending_count(&self) -> Result<i64, DbError> {
        let mut count = 0;
        self.db.query(
            &format!(
                "SELECT COUNT(*) FROM files WHERE state = 'pending'{}",
                self.scope_sql
            ),
            &self.scope_values,
            |row| {
                count = row.int(0);
                Ok(())
            },
        )?;
        Ok(count)
    }

    pub fn unfinished_count(&self) -> Result<(i64, i64), DbError> {
        let mut files = 0;
        let mut left = 0;
        self.db.query(
            &format!(
                "
                SELECT COUNT(*), SUM(MAX(0, CAST(json_extract(info, '$.read_end') AS INTEGER)
                                          - CAST(json_extract(info, '$.read_to') AS INTEGER)))
                FROM files WHERE state = 'partial'{}
                ",
                self.scope_sql
            ),
            &self.scope_values,
            |row| {
                files = row.int(0);
                left = if row.is_null(1) { 0 } else { row.int(1) };
                Ok(())
            },
        )?;
        Ok((files, left))
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_segment(
        &self,
        file: i64,
        kind: SegmentKind,
        modality: Modality,
        loc: Option<f64>,
        loc_end: Option<f64>,
        excerpt: Option<&str>,
        text: Option<&str>,
        vector: Option<&[f32]>,
    ) -> Result<(), DbError> {
        let stored_modality = if vector.is_none() {
            Modality::None
        } else {
            modality
        };
        self.db.run(
            "INSERT INTO segments(file, kind, modality, loc, loc_end, excerpt, vec) VALUES(?, ?, ?, ?, ?, ?, ?)",
            &[
                SqlValue::Int(file),
                SqlValue::Text(kind.as_str().into()),
                SqlValue::Text(stored_modality.as_str().into()),
                SqlValue::optional_f64(loc),
                SqlValue::optional_f64(loc_end),
                SqlValue::optional_text(excerpt.map(str::to_string)),
                vector
                    .map(|v| SqlValue::Blob(Vectors::half(v)))
                    .unwrap_or(SqlValue::Null),
            ],
        )?;
        if let Some(text) = text {
            if !text.is_empty() {
                let mut values = vec![
                    SqlValue::Int(self.db.last_insert_id()),
                    SqlValue::Text(String::new()),
                ];
                values.extend(Self::keyword_text(text));
                self.db.run(
                    "INSERT INTO fts(rowid, name, body, original) VALUES(?, ?, ?, ?)",
                    &values,
                )?;
            }
        }
        Ok(())
    }

    pub fn keyword_text(text: &str) -> Vec<SqlValue> {
        let normalized = SearchText::normalized(text);
        vec![
            SqlValue::Text(normalized.clone()),
            if normalized == text {
                SqlValue::Null
            } else {
                SqlValue::Text(text.to_string())
            },
        ]
    }

    fn add_name_row(&self, file: i64, path: &str) -> Result<(), DbError> {
        self.db.run(
            "INSERT INTO segments(file, kind, modality) VALUES(?, 'name', 'none')",
            &[SqlValue::Int(file)],
        )?;
        self.db.run(
            "INSERT INTO fts(rowid, name, body) VALUES(?, ?, ?)",
            &[
                SqlValue::Int(self.db.last_insert_id()),
                SqlValue::Text(SearchText::normalized(&Self::name_text(path))),
                SqlValue::Text(SearchText::normalized(&Self::folder_text(path))),
            ],
        )
    }

    fn delete_segments(&self, file: i64, keeping_name: bool) -> Result<(), DbError> {
        let filter = if keeping_name {
            "AND kind != 'name'"
        } else {
            ""
        };
        self.db.run(
            &format!(
                "DELETE FROM fts WHERE rowid IN (SELECT id FROM segments WHERE file = ? {filter})"
            ),
            &[SqlValue::Int(file)],
        )?;
        self.db.run(
            &format!("DELETE FROM segments WHERE file = ? {filter}"),
            &[SqlValue::Int(file)],
        )
    }

    fn name_text(path: &str) -> String {
        path.rsplit('/').next().unwrap_or(path).to_string()
    }

    fn folder_text(path: &str) -> String {
        let home = std::env::var("HOME").unwrap_or_default();
        let mut folder = match path.rsplit_once('/') {
            Some((head, _)) => head.to_string(),
            None => String::new(),
        };
        if !home.is_empty() && folder.starts_with(&home) {
            folder = folder[home.len()..].to_string();
        }
        folder
            .split('/')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    }

    pub fn finish(
        &self,
        id: i64,
        state: &str,
        detail: Option<&str>,
        info: &BTreeMap<String, String>,
    ) -> Result<(), DbError> {
        let json = if info.is_empty() {
            None
        } else {
            Some(serde_json::to_string(info).unwrap_or_else(|_| "{}".into()))
        };
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        self.db.run(
            "UPDATE files SET state = ?, detail = ?, info = ?, indexed_at = ? WHERE id = ?",
            &[
                SqlValue::Text(state.into()),
                SqlValue::optional_text(detail.map(str::to_string)),
                SqlValue::optional_text(json),
                SqlValue::Double(now),
                SqlValue::Int(id),
            ],
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static N: AtomicU64 = AtomicU64::new(0);

    fn temp() -> PathBuf {
        let n = N.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("trowel-index-{}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn file(path: &str, inode: u64) -> Candidate {
        Candidate {
            path: path.into(),
            kind: FileKind::Image,
            size: 10,
            mtime: 1.0,
            inode,
            device: 1,
        }
    }

    #[test]
    fn prune_keeps_only_chosen_folders() {
        let dir = temp();
        let store = IndexStore::open(&dir).unwrap();
        store.insert(&file("/a/x.png", 1)).unwrap();
        store.insert(&file("/a/sub/y.png", 2)).unwrap();
        store.insert(&file("/ab/z.png", 3)).unwrap();
        store.insert(&file("/b/w.png", 4)).unwrap();
        assert_eq!(store.prune(&["/a".into()]).unwrap(), 2);
        assert_eq!(store.files_under(&["/a".into()]).unwrap().len(), 2);
        assert!(store
            .files_under(&["/ab".into(), "/b".into()])
            .unwrap()
            .is_empty());
        let mut keyword_rows = 0;
        store
            .db
            .query("SELECT COUNT(*) FROM fts", &[], |row| {
                keyword_rows = row.int(0);
                Ok(())
            })
            .unwrap();
        assert_eq!(keyword_rows, 2);
        assert_eq!(store.prune(&[]).unwrap(), 2);
    }

    #[test]
    fn scope_limits_what_is_waiting() {
        let dir = temp();
        let mut store = IndexStore::open(&dir).unwrap();
        store.insert(&file("/a/x.png", 1)).unwrap();
        store.insert(&file("/drive/y.png", 2)).unwrap();
        store.set_scope(Some(vec!["/a".into()]));
        assert_eq!(store.pending_count().unwrap(), 1);
        assert_eq!(
            store
                .pending(None, None)
                .unwrap()
                .iter()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>(),
            ["/a/x.png"]
        );
        store.set_scope(Some(vec![]));
        assert_eq!(store.pending_count().unwrap(), 0);
        store.set_scope(None);
        assert_eq!(store.pending_count().unwrap(), 2);
    }

    #[test]
    fn files_under_nested_folders_come_once() {
        let dir = temp();
        let store = IndexStore::open(&dir).unwrap();
        store
            .insert(&Candidate {
                path: "/D/Media/Courses/a.mp4".into(),
                kind: FileKind::Video,
                size: 1,
                mtime: 1.0,
                inode: 1,
                device: 1,
            })
            .unwrap();
        assert_eq!(
            store
                .files_under(&["/D".into(), "/D/Media/Courses".into()])
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn schema_one_migrates_to_current() {
        let dir = temp();
        let built = "google/embeddinggemma-2@bfcd298 llama.cpp-b11461-metal text-q8_0+mmproj-q8_0";
        let db = Database::open(dir.join("index.sqlite")).unwrap();
        db.execute(&format!(
            "
            CREATE TABLE meta(key TEXT PRIMARY KEY, value TEXT NOT NULL);
            CREATE TABLE files(id INTEGER PRIMARY KEY, path TEXT NOT NULL UNIQUE, kind TEXT NOT NULL,
                size INTEGER NOT NULL, mtime REAL NOT NULL, inode INTEGER NOT NULL, device INTEGER NOT NULL,
                state TEXT NOT NULL DEFAULT 'pending', detail TEXT, info TEXT, indexed_at REAL);
            CREATE TABLE segments(id INTEGER PRIMARY KEY, file INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
                kind TEXT NOT NULL, modality TEXT NOT NULL, loc REAL, loc_end REAL, excerpt TEXT, vec BLOB);
            CREATE VIRTUAL TABLE fts USING fts5(name, body, tokenize = 'porter unicode61 remove_diacritics 2');
            INSERT INTO files VALUES(1, '/d/notes.txt', 'doc', 1, 1, 1, 1, 'done', NULL, NULL, 1);
            INSERT INTO segments VALUES(1, 1, 'chunk', 'text', 1, NULL, NULL, NULL);
            INSERT INTO fts(rowid, name, body) VALUES(1, '', 'قَالَ إِنَّ الْمَكْتَبَةَ تُغْلَقُ');
            INSERT INTO files VALUES(2, '/d/rent.pdf', 'pdf', 1, 1, 2, 1, 'done', NULL, NULL, 1);
            INSERT INTO segments VALUES(2, 2, 'page', 'text', 1, NULL, NULL, NULL);
            INSERT INTO fts(rowid, name, body) VALUES(2, '', 'বািড় ভাড়ার চিপ ধানমি ৭ নর রােডর িতন বডরেমর াট মািসক ২৫,০০০ টাকা ভাড়ায় দওয়া হেলা।');
            INSERT INTO files VALUES(3, '/d/shot.png', 'screenshot', 1, 1, 3, 1, 'done', NULL, NULL, 1);
            INSERT INTO segments VALUES(3, 3, 'ocr', 'text', NULL, NULL, 'Booking K7Q2LM', zeroblob(1536));
            INSERT INTO fts(rowid, name, body) VALUES(3, '', 'Booking K7Q2LM');
            INSERT INTO meta VALUES('fingerprint', '{built} | photo140 shot280 page280 frame70/3.0s audio30.0/25.0s');
            INSERT INTO meta VALUES('vector_source', '{built}');
            PRAGMA user_version = 1;
            "
        ))
        .unwrap();
        drop(db);
        let store = IndexStore::open(&dir).unwrap();
        assert_eq!(store.db.user_version().unwrap(), IndexStore::SCHEMA_VERSION);
        assert_eq!(
            store
                .pending(None, None)
                .unwrap()
                .iter()
                .map(|f| f.path.as_str())
                .collect::<Vec<_>>(),
            ["/d/rent.pdf"]
        );
        let mut vectors = -1;
        store
            .db
            .query(
                "SELECT COUNT(vec) FROM segments WHERE kind = 'ocr'",
                &[],
                |row| {
                    vectors = row.int(0);
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(vectors, 0);
        let named = "google/embeddinggemma-2@bfcd298 llama.cpp-vectors1 text-q8_0+mmproj-q8_0";
        assert_eq!(store.meta("vector_source").unwrap().as_deref(), Some(named));
        assert!(store
            .meta("fingerprint")
            .unwrap()
            .unwrap()
            .starts_with(&format!("{named} | ")));
        assert_eq!(store.unfinished_count().unwrap(), (0, 0));
        // FTS tokenizer: Arabic word stays whole after normalize+migrate.
        let mut hits = 0;
        store
            .db
            .query(
                "SELECT COUNT(*) FROM fts WHERE fts MATCH '\"المكتبة\"'",
                &[],
                |row| {
                    hits = row.int(0);
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(hits, 1);
    }

    #[test]
    fn bengali_and_arabic_words_stay_whole() {
        let dir = temp();
        let store = IndexStore::open(&dir).unwrap();
        let id = store
            .insert(&Candidate {
                path: "/notes/trip.txt".into(),
                kind: FileKind::Doc,
                size: 1,
                mtime: 1.0,
                inode: 1,
                device: 1,
            })
            .unwrap();
        store
            .add_segment(
                id,
                SegmentKind::Chunk,
                Modality::Text,
                Some(1.0),
                None,
                None,
                Some("সুন্দরবনে আমরা কুমিরও দেখেছি। قَالَ إِنَّ الْمَكْتَبَةَ تُغْلَقُ"),
                None,
            )
            .unwrap();
        let mut bengali = 0;
        store
            .db
            .query(
                "SELECT COUNT(*) FROM fts WHERE fts MATCH '\"কুমির\"*'",
                &[],
                |row| {
                    bengali = row.int(0);
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(bengali, 1);
        let mut arabic = 0;
        store
            .db
            .query(
                "SELECT COUNT(*) FROM fts WHERE fts MATCH '\"المكتبة\"'",
                &[],
                |row| {
                    arabic = row.int(0);
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(arabic, 1);
        let mut miss = 0;
        store
            .db
            .query(
                "SELECT COUNT(*) FROM fts WHERE fts MATCH '\"কলকাতা\"'",
                &[],
                |row| {
                    miss = row.int(0);
                    Ok(())
                },
            )
            .unwrap();
        assert_eq!(miss, 0);
    }

    #[test]
    fn linux_tables_exist_without_bumping_schema() {
        let dir = temp();
        let store = IndexStore::open(&dir).unwrap();
        assert_eq!(store.db.user_version().unwrap(), 5);
        store
            .db
            .query("SELECT COUNT(*) FROM linux_raw_paths", &[], |_| Ok(()))
            .unwrap();
        store
            .db
            .query("SELECT COUNT(*) FROM linux_attempts", &[], |_| Ok(()))
            .unwrap();
    }
}
