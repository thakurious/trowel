//! Thin SQLite wrapper. `upstream/Sources/DigUpKit/Database.swift`.

use std::path::Path;
use std::time::Duration;

use rusqlite::{params_from_iter, Connection, ToSql};
use thiserror::Error;

#[derive(Debug, Error)]
#[error("{message}")]
pub struct DbError {
    pub message: String,
}

impl DbError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl From<rusqlite::Error> for DbError {
    fn from(err: rusqlite::Error) -> Self {
        Self::new(err.to_string())
    }
}

#[derive(Debug, Clone)]
pub enum SqlValue {
    Int(i64),
    Double(f64),
    Text(String),
    Blob(Vec<u8>),
    Null,
}

impl SqlValue {
    pub fn optional_text(text: Option<String>) -> Self {
        text.map(Self::Text).unwrap_or(Self::Null)
    }

    pub fn optional_f64(number: Option<f64>) -> Self {
        number.map(Self::Double).unwrap_or(Self::Null)
    }
}

impl ToSql for SqlValue {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        match self {
            Self::Int(v) => v.to_sql(),
            Self::Double(v) => v.to_sql(),
            Self::Text(v) => v.to_sql(),
            Self::Blob(v) => v.to_sql(),
            Self::Null => Ok(rusqlite::types::ToSqlOutput::Owned(
                rusqlite::types::Value::Null,
            )),
        }
    }
}

/// One connection, used from one thread.
/// `upstream/Sources/DigUpKit/Database.swift:5`
pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, DbError> {
        let conn = Connection::open(path.as_ref())
            .map_err(|e| DbError::new(format!("cannot open {}: {e}", path.as_ref().display())))?;
        conn.busy_timeout(Duration::from_millis(5000))?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL; PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;",
        )?;
        Ok(Self { conn })
    }

    pub fn execute(&self, sql: &str) -> Result<(), DbError> {
        self.conn.execute_batch(sql).map_err(|e| {
            DbError::new(format!(
                "{e} (in: {})",
                sql.chars().take(120).collect::<String>()
            ))
        })
    }

    pub fn run(&self, sql: &str, values: &[SqlValue]) -> Result<(), DbError> {
        self.conn
            .execute(sql, params_from_iter(values.iter()))
            .map(|_| ())
            .map_err(|e| {
                DbError::new(format!(
                    "{e} (in: {})",
                    sql.chars().take(160).collect::<String>()
                ))
            })
    }

    pub fn query<F>(&self, sql: &str, values: &[SqlValue], mut row: F) -> Result<(), DbError>
    where
        F: FnMut(&QueryRow<'_>) -> Result<(), DbError>,
    {
        let mut stmt = self.conn.prepare(sql).map_err(|e| {
            DbError::new(format!(
                "{e} (in: {})",
                sql.chars().take(160).collect::<String>()
            ))
        })?;
        let mut rows = stmt.query(params_from_iter(values.iter()))?;
        while let Some(r) = rows.next()? {
            row(&QueryRow(r))?;
        }
        Ok(())
    }

    pub fn transaction<T, F>(&self, body: F) -> Result<T, DbError>
    where
        F: FnOnce() -> Result<T, DbError>,
    {
        self.conn.execute("BEGIN IMMEDIATE", [])?;
        match body() {
            Ok(result) => {
                self.conn.execute("COMMIT", [])?;
                Ok(result)
            }
            Err(err) => {
                let _ = self.conn.execute("ROLLBACK", []);
                Err(err)
            }
        }
    }

    pub fn last_insert_id(&self) -> i64 {
        self.conn.last_insert_rowid()
    }

    pub fn user_version(&self) -> Result<i64, DbError> {
        self.conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .map_err(DbError::from)
    }

    pub fn set_busy_timeout(&self, ms: u64) -> Result<(), DbError> {
        self.conn
            .busy_timeout(Duration::from_millis(ms))
            .map_err(DbError::from)
    }
}

pub struct QueryRow<'a>(&'a rusqlite::Row<'a>);

impl QueryRow<'_> {
    pub fn int(&self, column: usize) -> i64 {
        self.0.get::<_, i64>(column).unwrap_or(0)
    }

    pub fn double(&self, column: usize) -> f64 {
        self.0.get::<_, f64>(column).unwrap_or(0.0)
    }

    pub fn is_null(&self, column: usize) -> bool {
        matches!(
            self.0.get_ref(column).map(|v| v.data_type()),
            Ok(rusqlite::types::Type::Null)
        )
    }

    pub fn text(&self, column: usize) -> Option<String> {
        self.0.get::<_, Option<String>>(column).ok().flatten()
    }

    pub fn blob(&self, column: usize) -> Option<Vec<u8>> {
        self.0.get::<_, Option<Vec<u8>>>(column).ok().flatten()
    }
}
