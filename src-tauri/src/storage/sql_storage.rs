// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// SQL Storage
//
// Lightweight wrapper around a single SQLite database with
// WAL-mode pragmas and schema migration support. Each plugin
// gets its own database file; this struct manages the
// connection, configuration, and migration lifecycle.
//
// The public API uses `SqlValue` and `SqlRow` instead of
// rusqlite types, so callers never depend on the underlying
// database driver. This abstraction is designed to be
// serde-serializable for a future WASM plugin boundary.
// =========================================================

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension};
use rusqlite_migration::{M, Migrations};

// =========================================================
// SqlValue — query parameters
// =========================================================

/// A database value that can be used as a query parameter or
/// read from a result row. Maps to SQLite's five storage
/// classes.
///
/// All variants use owned types so the value can be serialized
/// across a WASM boundary without lifetime concerns.
#[derive(Debug, Clone, PartialEq)]
pub enum SqlValue {
    Null,
    Integer(i64),
    Real(f64),
    Text(String),
    Blob(Vec<u8>),
}

// Convenience conversions so callers can write:
//   &[val("hello"), val(42)]
// instead of:
//   &[SqlValue::Text("hello".into()), SqlValue::Integer(42)]

impl From<&str> for SqlValue {
    fn from(s: &str) -> Self {
        SqlValue::Text(s.to_string())
    }
}

impl From<String> for SqlValue {
    fn from(s: String) -> Self {
        SqlValue::Text(s)
    }
}

impl From<i64> for SqlValue {
    fn from(v: i64) -> Self {
        SqlValue::Integer(v)
    }
}

impl From<f64> for SqlValue {
    fn from(v: f64) -> Self {
        SqlValue::Real(v)
    }
}

impl From<Vec<u8>> for SqlValue {
    fn from(v: Vec<u8>) -> Self {
        SqlValue::Blob(v)
    }
}

impl From<bool> for SqlValue {
    fn from(v: bool) -> Self {
        SqlValue::Integer(v as i64)
    }
}

impl<T: Into<SqlValue>> From<Option<T>> for SqlValue {
    fn from(v: Option<T>) -> Self {
        match v {
            Some(inner) => inner.into(),
            None => SqlValue::Null,
        }
    }
}

/// Convert `SqlValue` to a rusqlite-compatible parameter.
impl rusqlite::types::ToSql for SqlValue {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        use rusqlite::types::{ToSqlOutput, Value};
        match self {
            SqlValue::Null => Ok(ToSqlOutput::Owned(Value::Null)),
            SqlValue::Integer(i) => Ok(ToSqlOutput::Owned(Value::Integer(*i))),
            SqlValue::Real(f) => Ok(ToSqlOutput::Owned(Value::Real(*f))),
            SqlValue::Text(s) => Ok(ToSqlOutput::Owned(Value::Text(s.clone()))),
            SqlValue::Blob(b) => Ok(ToSqlOutput::Owned(Value::Blob(b.clone()))),
        }
    }
}

// =========================================================
// SqlRow — result row access
// =========================================================

/// A result row with type-safe column access.
///
/// Wraps a materialized row of `SqlValue` columns. The row
/// mapper closure receives this instead of a rusqlite `Row`,
/// keeping the database driver out of the public API.
pub struct SqlRow {
    columns: Vec<SqlValue>,
}

impl SqlRow {
    /// Read a column by index, converting to the target type.
    ///
    /// Returns an error if the index is out of bounds or the
    /// value cannot be converted to `T`.
    pub fn get<T: FromSqlValue>(&self, index: usize) -> Result<T> {
        let value = self
            .columns
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("column index {index} out of bounds"))?;
        T::from_sql_value(value).ok_or_else(|| anyhow::anyhow!("column {index}: type mismatch"))
    }
}

/// Materialize a rusqlite `Row` into a `SqlRow` by reading all
/// columns as generic `Value`s.
fn materialize_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<SqlRow> {
    let count = row.as_ref().column_count();
    let mut columns = Vec::with_capacity(count);

    for i in 0..count {
        let value: rusqlite::types::Value = row.get(i)?;
        columns.push(match value {
            rusqlite::types::Value::Null => SqlValue::Null,
            rusqlite::types::Value::Integer(i) => SqlValue::Integer(i),
            rusqlite::types::Value::Real(f) => SqlValue::Real(f),
            rusqlite::types::Value::Text(s) => SqlValue::Text(s),
            rusqlite::types::Value::Blob(b) => SqlValue::Blob(b),
        });
    }

    Ok(SqlRow { columns })
}

// =========================================================
// FromSqlValue — type extraction from SqlValue
// =========================================================

/// Extract a typed value from a `SqlValue`. Implemented for
/// common Rust types that map to SQLite storage classes.
pub trait FromSqlValue: Sized {
    fn from_sql_value(value: &SqlValue) -> Option<Self>;
}

impl FromSqlValue for String {
    fn from_sql_value(value: &SqlValue) -> Option<Self> {
        match value {
            SqlValue::Text(s) => Some(s.clone()),
            _ => None,
        }
    }
}

impl FromSqlValue for i64 {
    fn from_sql_value(value: &SqlValue) -> Option<Self> {
        match value {
            SqlValue::Integer(i) => Some(*i),
            _ => None,
        }
    }
}

impl FromSqlValue for f64 {
    fn from_sql_value(value: &SqlValue) -> Option<Self> {
        match value {
            SqlValue::Real(f) => Some(*f),
            SqlValue::Integer(i) => Some(*i as f64),
            _ => None,
        }
    }
}

impl FromSqlValue for bool {
    fn from_sql_value(value: &SqlValue) -> Option<Self> {
        match value {
            SqlValue::Integer(i) => Some(*i != 0),
            _ => None,
        }
    }
}

impl FromSqlValue for Vec<u8> {
    fn from_sql_value(value: &SqlValue) -> Option<Self> {
        match value {
            SqlValue::Blob(b) => Some(b.clone()),
            _ => None,
        }
    }
}

impl<T: FromSqlValue> FromSqlValue for Option<T> {
    fn from_sql_value(value: &SqlValue) -> Option<Self> {
        match value {
            SqlValue::Null => Some(None),
            _ => Some(T::from_sql_value(value)),
        }
    }
}

// =========================================================
// SqlStorage
// =========================================================

/// Per-plugin SQLite database with WAL mode and migrations.
///
/// The connection is protected by a `Mutex` so the struct can
/// be shared across threads via `Arc`. All methods lock the
/// mutex, prepare the statement, execute, and wrap errors with
/// `anyhow::Context`.
pub struct SqlStorage {
    conn: Mutex<Connection>,
}

impl SqlStorage {
    /// Open (or create) a database at `db_path`, configure
    /// pragmas for performance and correctness, and run any
    /// pending schema migrations.
    ///
    /// `migrations` is a slice of SQL strings, each representing
    /// one forward migration step. They are applied in order and
    /// tracked by `rusqlite_migration` so each runs at most once.
    pub fn open(db_path: PathBuf, migrations: &[&str]) -> Result<Self> {
        if let Some(parent) = db_path.parent() {
            std::fs::create_dir_all(parent).context("create database directory")?;
        }

        let mut conn =
            Connection::open(&db_path).with_context(|| format!("open {}", db_path.display()))?;

        configure_connection(&conn).context("configure database connection")?;

        let migration_set: Vec<M<'_>> = migrations.iter().map(|sql| M::up(sql)).collect();
        let migrations = Migrations::new(migration_set);

        migrations
            .to_latest(&mut conn)
            .context("run database migrations")?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Execute a statement that modifies data (INSERT, UPDATE,
    /// DELETE). Returns the number of rows affected.
    pub fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<usize> {
        let conn = self.conn.lock().expect("sql connection not poisoned");
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        conn.execute(sql, param_refs.as_slice())
            .context("execute SQL statement")
    }

    /// Execute a query and map each result row into `T` using
    /// the provided closure.
    ///
    /// Each row is materialized into a `SqlRow` before being
    /// passed to the mapper, so the closure never touches
    /// rusqlite types.
    pub fn query_map<T>(
        &self,
        sql: &str,
        params: &[SqlValue],
        mut f: impl FnMut(&SqlRow) -> Result<T>,
    ) -> Result<Vec<T>> {
        let conn = self.conn.lock().expect("sql connection not poisoned");
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        let mut stmt = conn.prepare(sql).context("prepare SQL query")?;

        let rows = stmt
            .query_map(param_refs.as_slice(), materialize_row)
            .context("execute SQL query")?;

        let mut result = Vec::new();
        for row in rows {
            let sql_row = row.context("read result row")?;
            result.push(f(&sql_row)?);
        }
        Ok(result)
    }

    /// Execute a query expected to return zero or one row.
    /// Returns `Ok(None)` if the query produces no results.
    pub fn query_optional<T>(
        &self,
        sql: &str,
        params: &[SqlValue],
        f: impl FnOnce(&SqlRow) -> Result<T>,
    ) -> Result<Option<T>> {
        let conn = self.conn.lock().expect("sql connection not poisoned");
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = params
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        let mut stmt = conn.prepare(sql).context("prepare SQL query")?;

        let maybe_row = stmt
            .query_row(param_refs.as_slice(), materialize_row)
            .optional()
            .context("execute optional query")?;

        match maybe_row {
            Some(sql_row) => Ok(Some(f(&sql_row)?)),
            None => Ok(None),
        }
    }
}

// =========================================================
// Connection Configuration
// =========================================================

/// Apply performance and correctness pragmas to a fresh
/// connection.
///
/// - `journal_mode = wal`: concurrent readers don't block
///   the writer.
/// - `busy_timeout = 5000`: wait up to 5s on lock contention
///   instead of failing immediately.
/// - `synchronous = normal`: safe with WAL, faster than `full`.
/// - `foreign_keys = on`: enforce REFERENCES constraints
///   (off by default in SQLite).
/// - `cache_size = -2000`: 2 MB page cache (negative =
///   kibibytes).
fn configure_connection(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "wal")
        .context("set journal_mode")?;
    conn.pragma_update(None, "busy_timeout", 5000)
        .context("set busy_timeout")?;
    conn.pragma_update(None, "synchronous", "normal")
        .context("set synchronous")?;
    conn.pragma_update(None, "foreign_keys", "on")
        .context("set foreign_keys")?;
    conn.pragma_update(None, "cache_size", -2000)
        .context("set cache_size")?;
    Ok(())
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_migrate_insert_query() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("test.db");

        let storage = SqlStorage::open(
            db_path,
            &["CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);"],
        )
        .expect("open database");

        let affected = storage
            .execute(
                "INSERT INTO items (name) VALUES (?1)",
                &[SqlValue::from("hello")],
            )
            .expect("insert row");
        assert_eq!(affected, 1);

        let names: Vec<String> = storage
            .query_map("SELECT name FROM items", &[], |row| row.get(0))
            .expect("query rows");
        assert_eq!(names, vec!["hello".to_string()]);

        let found: Option<String> = storage
            .query_optional(
                "SELECT name FROM items WHERE id = ?1",
                &[SqlValue::from(1i64)],
                |row| row.get(0),
            )
            .expect("optional query");
        assert_eq!(found, Some("hello".to_string()));

        let missing: Option<String> = storage
            .query_optional(
                "SELECT name FROM items WHERE id = ?1",
                &[SqlValue::from(999i64)],
                |row| row.get(0),
            )
            .expect("optional query for missing row");
        assert_eq!(missing, None);
    }

    #[test]
    fn optional_columns() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("test.db");

        let storage = SqlStorage::open(
            db_path,
            &["CREATE TABLE data (id INTEGER PRIMARY KEY, value TEXT);"],
        )
        .expect("open database");

        storage
            .execute(
                "INSERT INTO data (id, value) VALUES (?1, ?2)",
                &[SqlValue::from(1i64), SqlValue::from("present")],
            )
            .expect("insert with value");

        storage
            .execute(
                "INSERT INTO data (id, value) VALUES (?1, ?2)",
                &[SqlValue::from(2i64), SqlValue::Null],
            )
            .expect("insert with null");

        let results: Vec<(i64, Option<String>)> = storage
            .query_map("SELECT id, value FROM data ORDER BY id", &[], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .expect("query");

        assert_eq!(results, vec![(1, Some("present".to_string())), (2, None),]);
    }
}
