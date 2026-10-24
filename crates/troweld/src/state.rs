//! `state.sqlite`: launcher and clipboard state (D20). Separate from `index.sqlite`, so D16 schema
//! parity and the index's F10 handling are untouched. The daemon is its only writer.
//! `LAUNCHER-PLAN.md` L1; clipboard rows are written from L3.

use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use trowel_core::{Database, DbError, IndexStore, SqlValue};
use trowel_proto::CalcHistoryRow;

use crate::apps::rank::decayed;
use crate::apps::Launches;
use crate::learn::{self, Learned, Term};

pub const STATE_FILE: &str = "state.sqlite";
/// 1: L1. 2: `apps_launches.score` (decayed frecency sum, L2). 3: `clip_entries.seq`, the
/// recency order (`last_used` is whole seconds, so copies within a second would tie; L3).
/// 4: `learned`, Raycast-style frecency and search terms for apps and commands (D19 amended).
const SCHEMA_VERSION: i64 = 4;

pub struct StateDb {
    pub(crate) db: Database,
    pub file: PathBuf,
}

/// `$XDG_DATA_HOME/trowel` (default `~/.local/share/trowel`): next to `index/` and `models/`.
pub fn default_state_dir() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("trowel")
}

impl StateDb {
    /// Opens or creates `dir/state.sqlite` (`0700` dir, `0600` file). A file that fails
    /// `quick_check` is moved to `state.sqlite.corrupt-<ts>` and a fresh one is started
    /// (ROBUSTNESS F10 parity; F-C4 adds pin salvage in L5).
    pub fn open(dir: impl AsRef<Path>) -> Result<Self, DbError> {
        let dir = dir.as_ref();
        fs::create_dir_all(dir)
            .map_err(|e| DbError::new(format!("mkdir {}: {e}", dir.display())))?;
        let _ = fs::set_permissions(dir, fs::Permissions::from_mode(0o700));
        let file = dir.join(STATE_FILE);
        let existed = fs::metadata(&file).map(|m| m.len() > 0).unwrap_or(false);
        match Self::open_file(&file) {
            Ok(state) => Ok(state),
            Err(err) if existed => {
                let ts = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                for suffix in ["", "-wal", "-shm"] {
                    let from = PathBuf::from(format!("{}{suffix}", file.display()));
                    if from.exists() {
                        let _ = fs::rename(
                            &from,
                            dir.join(format!("{STATE_FILE}.corrupt-{ts}{suffix}")),
                        );
                    }
                }
                eprintln!("troweld: state.sqlite was damaged, started fresh ({err})");
                Self::open_file(&file)
            }
            Err(err) => Err(err),
        }
    }

    fn open_file(file: &Path) -> Result<Self, DbError> {
        let db = Database::open(file)?;
        let _ = fs::set_permissions(file, fs::Permissions::from_mode(0o600));
        if !db.quick_check()? {
            return Err(DbError::new("PRAGMA quick_check failed"));
        }
        let state = Self {
            db,
            file: file.to_path_buf(),
        };
        state.migrate()?;
        Ok(state)
    }

    fn migrate(&self) -> Result<(), DbError> {
        let version = self.db.user_version()?;
        if version > SCHEMA_VERSION {
            return Err(DbError::new(format!(
                "state.sqlite schema {version} is newer than this troweld ({SCHEMA_VERSION})"
            )));
        }
        if version == SCHEMA_VERSION {
            return Ok(());
        }
        if version == 1 {
            // Old rows only know their count; treat each as launched at `last`.
            // Then fall through: the creates below are idempotent and add anything missing.
            self.db.execute(
                "BEGIN;
                ALTER TABLE apps_launches ADD COLUMN score REAL NOT NULL DEFAULT 0;
                UPDATE apps_launches SET score = count;
                COMMIT;",
            )?;
        }
        // Same tokenizer string as the index, so clipboard keyword search treats Unicode
        // (marks, Bengali, Arabic) exactly as file search does.
        let tokenizer = IndexStore::TOKENIZER.replace('\'', "''");
        self.db.execute(&format!(
            "BEGIN;
            CREATE TABLE IF NOT EXISTS apps_launches(
                id TEXT PRIMARY KEY,
                count INTEGER NOT NULL,
                last INTEGER NOT NULL,
                score REAL NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS clip_blobs(
                sha256 TEXT PRIMARY KEY,
                ext TEXT NOT NULL,
                bytes INTEGER NOT NULL,
                refcount INTEGER NOT NULL DEFAULT 0);
            CREATE TABLE IF NOT EXISTS clip_entries(
                id INTEGER PRIMARY KEY,
                hash TEXT NOT NULL UNIQUE,
                kind TEXT NOT NULL,
                preview TEXT NOT NULL,
                mimes TEXT NOT NULL,
                bytes INTEGER NOT NULL,
                body TEXT,
                html TEXT,
                files TEXT,
                blob TEXT REFERENCES clip_blobs(sha256),
                width INTEGER,
                height INTEGER,
                color TEXT,
                domain TEXT,
                source_app TEXT,
                created INTEGER NOT NULL,
                last_used INTEGER NOT NULL,
                uses INTEGER NOT NULL DEFAULT 1,
                pinned INTEGER NOT NULL DEFAULT 0,
                seq INTEGER NOT NULL DEFAULT 0);
            CREATE VIRTUAL TABLE IF NOT EXISTS clip_fts USING fts5(
                preview, body, content = 'clip_entries', content_rowid = 'id',
                tokenize = '{tokenizer}');
            CREATE TRIGGER IF NOT EXISTS clip_fts_ai AFTER INSERT ON clip_entries BEGIN
                INSERT INTO clip_fts(rowid, preview, body) VALUES (new.id, new.preview, new.body);
            END;
            CREATE TRIGGER IF NOT EXISTS clip_fts_ad AFTER DELETE ON clip_entries BEGIN
                INSERT INTO clip_fts(clip_fts, rowid, preview, body)
                    VALUES ('delete', old.id, old.preview, old.body);
            END;
            CREATE TRIGGER IF NOT EXISTS clip_fts_au AFTER UPDATE OF preview, body ON clip_entries BEGIN
                INSERT INTO clip_fts(clip_fts, rowid, preview, body)
                    VALUES ('delete', old.id, old.preview, old.body);
                INSERT INTO clip_fts(rowid, preview, body) VALUES (new.id, new.preview, new.body);
            END;
            CREATE TABLE IF NOT EXISTS learned(
                item TEXT PRIMARY KEY,
                anchor REAL NOT NULL,
                opened_at INTEGER NOT NULL,
                terms TEXT NOT NULL DEFAULT '[]');
            CREATE TABLE IF NOT EXISTS calc_history(
                id INTEGER PRIMARY KEY,
                expr TEXT NOT NULL,
                result TEXT NOT NULL,
                at INTEGER NOT NULL);
            COMMIT;"
        ))?;
        let mut has_seq = false;
        self.db.query(
            "SELECT 1 FROM pragma_table_info('clip_entries') WHERE name = 'seq'",
            &[],
            |_| {
                has_seq = true;
                Ok(())
            },
        )?;
        if version < 4 {
            self.seed_learned()?;
        }
        let add_seq = if has_seq {
            ""
        } else {
            "ALTER TABLE clip_entries ADD COLUMN seq INTEGER NOT NULL DEFAULT 0;
             UPDATE clip_entries SET seq = id;"
        };
        self.db.execute(&format!(
            "BEGIN;
            {add_seq}
            DROP INDEX IF EXISTS clip_entries_order;
            CREATE INDEX IF NOT EXISTS clip_entries_recent ON clip_entries(pinned DESC, seq DESC);
            PRAGMA user_version = {SCHEMA_VERSION};
            COMMIT;"
        ))
    }

    /// Counts a launch (`app_launch`). `score` stays Σ 0.5^(age/14 d) over every launch: the old
    /// sum decays to `at`, plus one. Returns the new row.
    pub fn record_launch(&self, id: &str, at: i64) -> Result<Launches, DbError> {
        self.db.transaction(|| {
            let old = self.launch(id)?.unwrap_or_default();
            let last = old.last.max(at);
            let row = Launches {
                count: old.count + 1,
                last,
                score: decayed(old.score, old.last, last) + decayed(1.0, at, last),
            };
            self.db.run(
                "INSERT INTO apps_launches(id, count, last, score) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(id) DO UPDATE SET count = ?2, last = ?3, score = ?4",
                &[
                    SqlValue::Text(id.into()),
                    SqlValue::Int(i64::from(row.count)),
                    SqlValue::Int(row.last),
                    SqlValue::Double(row.score),
                ],
            )?;
            Ok(row)
        })
    }

    fn launch(&self, id: &str) -> Result<Option<Launches>, DbError> {
        let mut out = None;
        self.db.query(
            "SELECT count, last, score FROM apps_launches WHERE id = ?1",
            &[SqlValue::Text(id.into())],
            |row| {
                out = Some(Launches {
                    count: row.int(0).max(0) as u32,
                    last: row.int(1),
                    score: row.double(2),
                });
                Ok(())
            },
        )?;
        Ok(out)
    }

    /// Every app's launches, for `apps_query`.
    pub fn launches(&self) -> Result<HashMap<String, Launches>, DbError> {
        let mut out = HashMap::new();
        self.db.query(
            "SELECT id, count, last, score FROM apps_launches",
            &[],
            |row| {
                if let Some(id) = row.text(0) {
                    out.insert(
                        id,
                        Launches {
                            count: row.int(1).max(0) as u32,
                            last: row.int(2),
                            score: row.double(3),
                        },
                    );
                }
                Ok(())
            },
        )?;
        Ok(out)
    }

    /// v4: each app's 14-day launch sum becomes a Raycast anchor. n launches at once sum to n;
    /// Raycast scores them 1 + 100·n, so that is the seeded score at `last`. No search terms.
    fn seed_learned(&self) -> Result<(), DbError> {
        let launches = self.launches()?;
        self.db.transaction(|| {
            for (id, l) in &launches {
                self.db.run(
                    "INSERT OR IGNORE INTO learned(item, anchor, opened_at) VALUES (?1, ?2, ?3)",
                    &[
                        SqlValue::Text(format!("app:{id}")),
                        SqlValue::Double(learn::anchor_for(1.0 + 100.0 * l.score, l.last)),
                        SqlValue::Int(l.last),
                    ],
                )?;
            }
            Ok(())
        })
    }

    /// One visit of `item` (`app:<id>` / `cmd:<id>`) picked with `query`. Returns the new row.
    pub fn record_learned(&self, item: &str, query: &str, at: i64) -> Result<Learned, DbError> {
        self.db.transaction(|| {
            let mut prior = None;
            self.db.query(
                "SELECT anchor, opened_at, terms FROM learned WHERE item = ?1",
                &[SqlValue::Text(item.into())],
                |row| {
                    prior = Some(learned_row(row.double(0), row.int(1), row.text(2)));
                    Ok(())
                },
            )?;
            let next = learn::record(prior.as_ref(), at, query);
            self.db.run(
                "INSERT INTO learned(item, anchor, opened_at, terms) VALUES (?1, ?2, ?3, ?4)
                 ON CONFLICT(item) DO UPDATE SET anchor = ?2, opened_at = ?3, terms = ?4",
                &[
                    SqlValue::Text(item.into()),
                    SqlValue::Double(next.anchor),
                    SqlValue::Int(next.opened_at),
                    SqlValue::Text(
                        serde_json::to_string(&next.terms).unwrap_or_else(|_| "[]".into()),
                    ),
                ],
            )?;
            Ok(next)
        })
    }

    /// Every learned item, for `apps_query`.
    pub fn learned(&self) -> Result<HashMap<String, Learned>, DbError> {
        let mut out = HashMap::new();
        self.db.query(
            "SELECT item, anchor, opened_at, terms FROM learned",
            &[],
            |row| {
                if let Some(item) = row.text(0) {
                    out.insert(item, learned_row(row.double(1), row.int(2), row.text(3)));
                }
                Ok(())
            },
        )?;
        Ok(out)
    }

    /// Recorded when a result is copied or pasted, never per keystroke.
    pub fn record_calc(&self, expr: &str, result: &str, at: i64) -> Result<(), DbError> {
        self.db.run(
            "INSERT INTO calc_history(expr, result, at) VALUES (?1, ?2, ?3)",
            &[
                SqlValue::Text(expr.into()),
                SqlValue::Text(result.into()),
                SqlValue::Int(at),
            ],
        )
    }

    pub fn calc_history(&self, limit: u32) -> Result<Vec<CalcHistoryRow>, DbError> {
        let mut rows = Vec::new();
        self.db.query(
            "SELECT expr, result, at FROM calc_history ORDER BY at DESC, id DESC LIMIT ?1",
            &[SqlValue::Int(i64::from(limit))],
            |row| {
                rows.push(CalcHistoryRow {
                    expr: row.text(0).unwrap_or_default(),
                    result: row.text(1).unwrap_or_default(),
                    at: row.int(2),
                });
                Ok(())
            },
        )?;
        Ok(rows)
    }
}

/// A damaged `terms` value reads as no terms; the frecency still counts.
fn learned_row(anchor: f64, opened_at: i64, terms: Option<String>) -> Learned {
    Learned {
        anchor,
        opened_at,
        terms: terms
            .and_then(|t| serde_json::from_str::<Vec<Term>>(&t).ok())
            .unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQ: AtomicU64 = AtomicU64::new(0);

    fn temp() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "trowel-state-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn creates_private_files_and_is_idempotent() {
        let dir = temp();
        let state = StateDb::open(&dir).unwrap();
        assert_eq!(mode(&dir), 0o700);
        assert_eq!(mode(&state.file), 0o600);
        drop(state);
        StateDb::open(&dir).unwrap();
    }

    #[test]
    fn clip_fts_uses_the_index_tokenizer() {
        let state = StateDb::open(temp()).unwrap();
        let mut sql = String::new();
        state
            .db
            .query(
                "SELECT sql FROM sqlite_master WHERE name = 'clip_fts'",
                &[],
                |row| {
                    sql = row.text(0).unwrap_or_default();
                    Ok(())
                },
            )
            .unwrap();
        assert!(
            sql.contains(&IndexStore::TOKENIZER.replace('\'', "''")),
            "{sql}"
        );
    }

    #[test]
    fn fts_follows_inserts_updates_and_deletes() {
        let state = StateDb::open(temp()).unwrap();
        let db = &state.db;
        db.execute(
            "INSERT INTO clip_entries(id, hash, kind, preview, mimes, bytes, body, created, last_used)
             VALUES (1, 'h1', 'text', 'firewall rules', '[]', 14, 'কুমির harbour notes', 1, 1)",
        )
        .unwrap();
        let hits = |q: &str| {
            let mut n = 0;
            db.query(
                "SELECT rowid FROM clip_fts WHERE clip_fts MATCH ?1",
                &[SqlValue::Text(q.into())],
                |_| {
                    n += 1;
                    Ok(())
                },
            )
            .unwrap();
            n
        };
        assert_eq!(hits("harbour"), 1);
        assert_eq!(hits("কুমির*"), 1, "marks count as letters");
        db.execute("UPDATE clip_entries SET body = 'refund' WHERE id = 1")
            .unwrap();
        assert_eq!(hits("harbour"), 0);
        assert_eq!(hits("refund"), 1);
        db.execute("DELETE FROM clip_entries WHERE id = 1").unwrap();
        assert_eq!(hits("refund"), 0);
    }

    #[test]
    fn launches_accumulate() {
        let state = StateDb::open(temp()).unwrap();
        state.record_launch("firefox", 100).unwrap();
        state.record_launch("firefox", 50).unwrap();
        state.record_launch("nvim", 70).unwrap();
        let all = state.launches().unwrap();
        assert_eq!(
            (all["firefox"].count, all["firefox"].last),
            (2, 100),
            "last never goes back"
        );
        assert_eq!((all["nvim"].count, all["nvim"].last), (1, 70));
    }

    #[test]
    fn frecency_is_a_decayed_sum() {
        let state = StateDb::open(temp()).unwrap();
        let day = 86_400;
        state.record_launch("a", 0).unwrap();
        let row = state.record_launch("a", 14 * day).unwrap();
        // 0.5 (two weeks old) + 1.
        assert!((row.score - 1.5).abs() < 1e-9, "{row:?}");
        // A launch recorded out of order counts with its own age.
        let row = state.record_launch("a", 0).unwrap();
        assert!((row.score - 2.0).abs() < 1e-9, "{row:?}");
    }

    #[test]
    fn v1_files_migrate_and_keep_counts() {
        let dir = temp();
        fs::create_dir_all(&dir).unwrap();
        {
            let db = Database::open(dir.join(STATE_FILE)).unwrap();
            db.execute(
                "CREATE TABLE apps_launches(id TEXT PRIMARY KEY, count INTEGER NOT NULL, last INTEGER NOT NULL);
                 INSERT INTO apps_launches VALUES ('nvim', 3, 50);
                 PRAGMA user_version = 1;",
            )
            .unwrap();
        }
        let state = StateDb::open(&dir).unwrap();
        let all = state.launches().unwrap();
        state.record_calc("1+1", "2", 1).unwrap();
        assert_eq!(all["nvim"].count, 3);
        assert!((all["nvim"].score - 3.0).abs() < 1e-9);
        // v4 seeds the learned score: three launches at 50 → 301 at 50.
        let learned = state.learned().unwrap();
        let nvim = &learned["app:nvim"];
        assert!(
            (learn::score(nvim.anchor, 50) - 301.0).abs() < 1e-6,
            "{nvim:?}"
        );
        assert!(nvim.terms.is_empty());
    }

    #[test]
    fn learned_rows_round_trip() {
        let state = StateDb::open(temp()).unwrap();
        state.record_learned("app:chromium", "Ch", 1_000).unwrap();
        let row = state.record_learned("app:chromium", "chr", 2_000).unwrap();
        // 101 decayed over 1000 s, plus 100.
        let want = 101.0 * 0.5f64.powf(1_000.0 / 864_000.0) + 100.0;
        assert!((learn::score(row.anchor, 2_000) - want).abs() < 1e-6);
        state.record_learned("cmd:clipboard", "", 2_000).unwrap();
        let all = state.learned().unwrap();
        let terms: Vec<&str> = all["app:chromium"]
            .terms
            .iter()
            .map(|t| t.term.as_str())
            .collect();
        assert_eq!(terms, ["ch", "chr"]);
        assert_eq!(all["app:chromium"].opened_at, 2_000);
        assert!(all["cmd:clipboard"].terms.is_empty());
        // A damaged terms column keeps the score.
        state
            .db
            .execute("UPDATE learned SET terms = 'nope' WHERE item = 'app:chromium'")
            .unwrap();
        assert!(state.learned().unwrap()["app:chromium"].terms.is_empty());
    }

    #[test]
    fn v2_clip_rows_get_a_recency_seq() {
        let dir = temp();
        let state = StateDb::open(&dir).unwrap();
        // A v2 file: clip_entries without `seq`.
        state
            .db
            .execute(
                "DROP INDEX clip_entries_recent;
                 ALTER TABLE clip_entries DROP COLUMN seq;
                 INSERT INTO clip_entries(id, hash, kind, preview, mimes, bytes, created, last_used)
                    VALUES (7, 'h', 'text', 'p', '[]', 1, 1, 1);
                 PRAGMA user_version = 2;",
            )
            .unwrap();
        drop(state);
        let state = StateDb::open(&dir).unwrap();
        assert_eq!(state.db.user_version().unwrap(), SCHEMA_VERSION);
        let mut seq = 0;
        state
            .db
            .query("SELECT seq FROM clip_entries WHERE id = 7", &[], |row| {
                seq = row.int(0);
                Ok(())
            })
            .unwrap();
        assert_eq!(seq, 7);
    }

    #[test]
    fn calc_history_is_newest_first() {
        let state = StateDb::open(temp()).unwrap();
        state.record_calc("1+1", "2", 10).unwrap();
        state.record_calc("5 ft to cm", "152.4", 20).unwrap();
        let rows = state.calc_history(1).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].expr, "5 ft to cm");
    }

    #[test]
    fn corrupt_state_is_quarantined() {
        let dir = temp();
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(STATE_FILE), vec![0x5au8; 8192]).unwrap();
        let state = StateDb::open(&dir).unwrap();
        state.record_launch("firefox", 1).unwrap();
        let quarantined = fs::read_dir(&dir).unwrap().filter_map(|e| e.ok()).any(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("state.sqlite.corrupt-")
        });
        assert!(quarantined, "damaged file moved aside");
    }

    #[test]
    fn newer_schema_is_moved_aside_not_downgraded() {
        let dir = temp();
        let state = StateDb::open(&dir).unwrap();
        state.db.execute("PRAGMA user_version = 99").unwrap();
        drop(state);
        // A newer troweld's file is moved aside (kept), never downgraded in place.
        let reopened = StateDb::open(&dir).unwrap();
        assert_eq!(reopened.db.user_version().unwrap(), SCHEMA_VERSION);
    }
}
