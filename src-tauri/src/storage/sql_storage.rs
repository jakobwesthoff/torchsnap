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
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context, Result};
use rusqlite::Connection;
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
    /// A list of values for use with SQL `IN (?)` clauses.
    ///
    /// When `execute` or `query_map` encounters a `List` parameter, it
    /// expands the single `?` placeholder in the SQL into `?, ?, ...`
    /// (one per element) and flattens the values into the bind array.
    /// This keeps caller SQL clean and injection-safe.
    ///
    /// An empty list expands to a single `NULL` to avoid the SQL
    /// syntax error from `IN ()`. Nested `List` values are not
    /// supported — all elements must be scalar variants.
    List(Vec<SqlValue>),
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
///
/// `List` is never bound directly — it is expanded before reaching
/// rusqlite. Attempting to bind a `List` is a programming error.
impl rusqlite::types::ToSql for SqlValue {
    fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
        use rusqlite::types::{ToSqlOutput, Value};
        match self {
            SqlValue::Null => Ok(ToSqlOutput::Owned(Value::Null)),
            SqlValue::Integer(i) => Ok(ToSqlOutput::Owned(Value::Integer(*i))),
            SqlValue::Real(f) => Ok(ToSqlOutput::Owned(Value::Real(*f))),
            SqlValue::Text(s) => Ok(ToSqlOutput::Owned(Value::Text(s.clone()))),
            SqlValue::Blob(b) => Ok(ToSqlOutput::Owned(Value::Blob(b.clone()))),
            SqlValue::List(_) => panic!("List values must be expanded before binding"),
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
    /// Monotonically increasing counter for generating unique savepoint
    /// names. Each `transaction()` call gets `sp_{n}` where `n` is the
    /// value before incrementing. Wraps on overflow (safe — a prior
    /// `sp_0` from 2^64 calls ago is long gone).
    savepoint_counter: AtomicU64,
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
            savepoint_counter: AtomicU64::new(0),
        })
    }

    /// Execute a statement that modifies data (INSERT, UPDATE,
    /// DELETE). Returns the number of rows affected.
    ///
    /// Supports `SqlValue::List` parameters — see [`expand_params`]
    /// for how `IN (?)` clauses are handled.
    pub fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<usize> {
        let conn = self.conn.lock().expect("sql connection not poisoned");
        let (expanded_sql, flat_params) = expand_params(sql, params);
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = flat_params
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        conn.execute(&expanded_sql, param_refs.as_slice())
            .context("execute SQL statement")
    }

    /// Execute a query and map each result row into `T` using
    /// the provided closure.
    ///
    /// Each row is materialized into a `SqlRow` before being
    /// passed to the mapper, so the closure never touches
    /// rusqlite types.
    ///
    /// Supports `SqlValue::List` parameters — see [`expand_params`]
    /// for how `IN (?)` clauses are handled.
    pub fn query_map<T>(
        &self,
        sql: &str,
        params: &[SqlValue],
        mut f: impl FnMut(&SqlRow) -> Result<T>,
    ) -> Result<Vec<T>> {
        let conn = self.conn.lock().expect("sql connection not poisoned");
        let (expanded_sql, flat_params) = expand_params(sql, params);
        let param_refs: Vec<&dyn rusqlite::types::ToSql> = flat_params
            .iter()
            .map(|v| v as &dyn rusqlite::types::ToSql)
            .collect();
        let mut stmt = conn.prepare(&expanded_sql).context("prepare SQL query")?;

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

    // =========================================================
    // Transactions (Savepoint-based)
    //
    // Uses SQLite SAVEPOINT/RELEASE/ROLLBACK TO uniformly at all
    // nesting depths. SQLite supports top-level SAVEPOINTs without
    // an enclosing BEGIN, so no special-casing is needed. Each
    // call gets a unique savepoint name via an atomic counter.
    // =========================================================

    /// Execute a closure inside a savepoint-based transaction.
    ///
    /// On success the savepoint is released (committed). On error
    /// it is rolled back and the error propagated. Nesting is
    /// fully supported — inner `transaction()` calls create nested
    /// savepoints that can be independently rolled back.
    ///
    /// ```ignore
    /// storage.transaction(|| {
    ///     storage.execute("INSERT INTO t (v) VALUES (?)", &[val("a")])?;
    ///     storage.execute("INSERT INTO t (v) VALUES (?)", &[val("b")])?;
    ///     Ok(())
    /// })?;
    /// ```
    pub fn transaction<T>(&self, f: impl FnOnce() -> Result<T>) -> Result<T> {
        let id = self.savepoint_counter.fetch_add(1, Ordering::Relaxed);
        let name = format!("sp_{id}");

        // SAVEPOINT opens the transaction (or nested savepoint).
        self.execute_raw(&format!("SAVEPOINT {name}"))
            .with_context(|| format!("create savepoint {name}"))?;

        match f() {
            Ok(value) => {
                // RELEASE commits the savepoint.
                self.execute_raw(&format!("RELEASE {name}"))
                    .with_context(|| format!("release savepoint {name}"))?;
                Ok(value)
            }
            Err(err) => {
                // ROLLBACK TO restores state but keeps the savepoint
                // active. The subsequent RELEASE removes it cleanly.
                let _ = self.execute_raw(&format!("ROLLBACK TO {name}"));
                let _ = self.execute_raw(&format!("RELEASE {name}"));
                Err(err)
            }
        }
    }

    /// Execute a raw SQL statement with no parameters. Used
    /// internally for savepoint control statements that cannot
    /// go through the parameterized path.
    fn execute_raw(&self, sql: &str) -> Result<()> {
        let conn = self.conn.lock().expect("sql connection not poisoned");
        conn.execute_batch(sql).context("execute raw SQL")?;
        Ok(())
    }
}

// =========================================================
// List Parameter Expansion
// =========================================================

/// Expand `SqlValue::List` parameters into individual placeholders.
///
/// Walks through the SQL string looking for `?` placeholders in
/// lockstep with `params`. When a param is a `List`:
///
/// - The single `?` is replaced with `?, ?, ...` (one per element).
/// - The list elements are flattened into the output param vector.
/// - An empty list expands to a single `NULL` to avoid the SQL
///   syntax error from `IN ()`.
///
/// Non-list params pass through unchanged. If no `List` params are
/// present, the SQL string is returned as-is (no allocation).
fn expand_params(sql: &str, params: &[SqlValue]) -> (String, Vec<SqlValue>) {
    // Fast path: skip allocation when no List params exist.
    let has_list = params.iter().any(|p| matches!(p, SqlValue::List(_)));
    if !has_list {
        return (sql.to_string(), params.to_vec());
    }

    let mut expanded_sql = String::with_capacity(sql.len() + 32);
    let mut flat_params = Vec::with_capacity(params.len());
    let mut param_iter = params.iter();

    // Walk the SQL character by character, replacing each `?` with
    // the appropriate expansion for the corresponding parameter.
    let mut chars = sql.chars().peekable();
    while let Some(ch) = chars.next() {
        // Skip over string literals so we don't mistake a `?`
        // inside a quoted value for a placeholder.
        if ch == '\'' {
            expanded_sql.push(ch);
            for inner in chars.by_ref() {
                expanded_sql.push(inner);
                if inner == '\'' {
                    break;
                }
            }
            continue;
        }

        if ch == '?' {
            let param = param_iter.next().expect("more ? placeholders than params");

            match param {
                SqlValue::List(items) if items.is_empty() => {
                    expanded_sql.push_str("NULL");
                    // No values added to flat_params — NULL is literal SQL.
                }
                SqlValue::List(items) => {
                    for (i, item) in items.iter().enumerate() {
                        assert!(
                            !matches!(item, SqlValue::List(_)),
                            "nested List values are not supported"
                        );
                        if i > 0 {
                            expanded_sql.push_str(", ");
                        }
                        expanded_sql.push('?');
                        flat_params.push(item.clone());
                    }
                }
                other => {
                    expanded_sql.push('?');
                    flat_params.push(other.clone());
                }
            }
        } else {
            expanded_sql.push(ch);
        }
    }

    (expanded_sql, flat_params)
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

        // Querying for a non-existent row should yield an empty result set.
        let missing: Vec<String> = storage
            .query_map(
                "SELECT name FROM items WHERE id = ?1",
                &[SqlValue::from(999i64)],
                |row| row.get(0),
            )
            .expect("query missing row");
        assert!(missing.is_empty());
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

    #[test]
    fn expand_params_no_lists() {
        let (sql, params) = expand_params(
            "SELECT * FROM t WHERE a = ? AND b = ?",
            &[SqlValue::from(1i64), SqlValue::from("x")],
        );
        assert_eq!(sql, "SELECT * FROM t WHERE a = ? AND b = ?");
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn expand_params_with_list() {
        let (sql, params) = expand_params(
            "SELECT * FROM t WHERE a = ? AND b IN (?)",
            &[
                SqlValue::from("x"),
                SqlValue::List(vec![
                    SqlValue::from(1i64),
                    SqlValue::from(2i64),
                    SqlValue::from(3i64),
                ]),
            ],
        );
        assert_eq!(sql, "SELECT * FROM t WHERE a = ? AND b IN (?, ?, ?)");
        assert_eq!(params.len(), 4);
    }

    #[test]
    fn expand_params_empty_list() {
        let (sql, _params) =
            expand_params("SELECT * FROM t WHERE a IN (?)", &[SqlValue::List(vec![])]);
        assert_eq!(sql, "SELECT * FROM t WHERE a IN (NULL)");
    }

    #[test]
    fn list_query_map() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("test.db");

        let storage = SqlStorage::open(
            db_path,
            &["CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);"],
        )
        .expect("open database");

        for name in &["alpha", "beta", "gamma", "delta"] {
            storage
                .execute(
                    "INSERT INTO items (name) VALUES (?)",
                    &[SqlValue::from(*name)],
                )
                .expect("insert");
        }

        let names: Vec<String> = storage
            .query_map(
                "SELECT name FROM items WHERE name IN (?) ORDER BY name",
                &[SqlValue::List(vec![
                    SqlValue::from("alpha"),
                    SqlValue::from("gamma"),
                ])],
                |row| row.get(0),
            )
            .expect("query with list");

        assert_eq!(names, vec!["alpha", "gamma"]);
    }

    #[test]
    fn transaction_commit() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("test.db");

        let storage = SqlStorage::open(
            db_path,
            &["CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);"],
        )
        .expect("open database");

        storage
            .transaction(|| {
                storage.execute(
                    "INSERT INTO items (name) VALUES (?)",
                    &[SqlValue::from("one")],
                )?;
                storage.execute(
                    "INSERT INTO items (name) VALUES (?)",
                    &[SqlValue::from("two")],
                )?;
                Ok(())
            })
            .expect("transaction commit");

        let names: Vec<String> = storage
            .query_map("SELECT name FROM items ORDER BY name", &[], |row| {
                row.get(0)
            })
            .expect("query");
        assert_eq!(names, vec!["one", "two"]);
    }

    #[test]
    fn transaction_rollback() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("test.db");

        let storage = SqlStorage::open(
            db_path,
            &["CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);"],
        )
        .expect("open database");

        // Insert one row outside the transaction so we can verify
        // it survives the rollback.
        storage
            .execute(
                "INSERT INTO items (name) VALUES (?)",
                &[SqlValue::from("before")],
            )
            .expect("insert before");

        let result: anyhow::Result<()> = storage.transaction(|| {
            storage.execute(
                "INSERT INTO items (name) VALUES (?)",
                &[SqlValue::from("inside")],
            )?;
            anyhow::bail!("intentional failure");
        });
        assert!(result.is_err());

        // Only the row inserted before the transaction should remain.
        let names: Vec<String> = storage
            .query_map("SELECT name FROM items", &[], |row| row.get(0))
            .expect("query");
        assert_eq!(names, vec!["before"]);
    }

    #[test]
    fn transaction_nested() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("test.db");

        let storage = SqlStorage::open(
            db_path,
            &["CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);"],
        )
        .expect("open database");

        storage
            .transaction(|| {
                storage.execute(
                    "INSERT INTO items (name) VALUES (?)",
                    &[SqlValue::from("outer")],
                )?;

                // Inner transaction that fails — should only roll back
                // its own work, leaving the outer insert intact.
                let inner_result: anyhow::Result<()> = storage.transaction(|| {
                    storage.execute(
                        "INSERT INTO items (name) VALUES (?)",
                        &[SqlValue::from("inner")],
                    )?;
                    anyhow::bail!("inner failure");
                });
                assert!(inner_result.is_err());

                Ok(())
            })
            .expect("outer transaction commit");

        let names: Vec<String> = storage
            .query_map("SELECT name FROM items", &[], |row| row.get(0))
            .expect("query");
        assert_eq!(names, vec!["outer"]);
    }

    #[test]
    fn transaction_returns_value() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("test.db");

        let storage = SqlStorage::open(
            db_path,
            &["CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);"],
        )
        .expect("open database");

        let count = storage
            .transaction(|| {
                storage.execute(
                    "INSERT INTO items (name) VALUES (?)",
                    &[SqlValue::from("a")],
                )?;
                storage.execute(
                    "INSERT INTO items (name) VALUES (?)",
                    &[SqlValue::from("b")],
                )?;
                let rows: Vec<String> =
                    storage.query_map("SELECT name FROM items", &[], |row| row.get(0))?;
                Ok(rows.len())
            })
            .expect("transaction with return value");

        assert_eq!(count, 2);
    }

    #[test]
    fn transaction_unique_savepoint_names() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let db_path = dir.path().join("test.db");

        let storage = SqlStorage::open(
            db_path,
            &["CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);"],
        )
        .expect("open database");

        // Run multiple sequential transactions to verify the counter
        // increments and savepoint names don't collide.
        for i in 0..5 {
            storage
                .transaction(|| {
                    storage.execute(
                        "INSERT INTO items (name) VALUES (?)",
                        &[SqlValue::from(format!("item_{i}"))],
                    )?;
                    Ok(())
                })
                .expect("sequential transaction");
        }

        let names: Vec<String> = storage
            .query_map("SELECT name FROM items ORDER BY name", &[], |row| {
                row.get(0)
            })
            .expect("query");
        assert_eq!(
            names,
            vec!["item_0", "item_1", "item_2", "item_3", "item_4"]
        );
    }
}
