// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// SQL Storage
//
// Lightweight wrapper around a single SQLite database with
// WAL-mode pragmas and schema migration support. Each gadget
// gets its own database file; this struct manages the
// connection, configuration, and migration lifecycle.
//
// The public API uses `SqlValue` and `SqlRow` instead of
// rusqlite types, so callers never depend on the underlying
// database driver. This abstraction is designed to be
// serde-serializable for a future WASM gadget boundary.
//
// Gadgets send SQL verbatim, so every connection is confined
// to its own database file (see `confine_connection`).
// =========================================================

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, PoisonError};

use anyhow::{Context, Result};
use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::limits::Limit;
use rusqlite::{Connection, Transaction, TransactionBehavior, ffi};
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

    /// Borrow the materialized column slice. Used by the
    /// WASM SQL bridge to forward result rows across the
    /// WIT boundary without going through the typed
    /// `FromSqlValue` accessor.
    ///
    /// Crate-private on purpose: any caller outside the
    /// crate should use the typed `get<T>` accessor instead
    /// — bypassing it should be a deliberate decision the
    /// boundary code makes, not a casual choice. Promote to
    /// `pub` only if a future external caller has a
    /// genuine reason and acknowledges the typed-accessor
    /// trade-off.
    pub(crate) fn columns(&self) -> &[SqlValue] {
        &self.columns
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

/// Per-gadget SQLite database with WAL mode and migrations.
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
    /// pragmas for performance and correctness, confine the
    /// connection to this one file, and run any pending schema
    /// migrations.
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

        // Gadget databases and the host's own stores share this
        // constructor, and no host store attaches databases or
        // vacuums, so every connection is confined. It happens
        // before the migrations because a gadget's migrations come
        // from its manifest and are gadget SQL too.
        confine_connection(&conn).context("confine database connection")?;

        let migration_set: Vec<M<'_>> = migrations.iter().map(|sql| M::up(sql)).collect();
        let migrations = Migrations::new(migration_set);

        migrations
            .to_latest(&mut conn)
            .context("run database migrations")?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Lock the connection. A panic inside a transaction closure
    /// poisons the mutex while the rusqlite `Transaction` rolls back
    /// on unwind, so the connection behind a poisoned lock is
    /// consistent and stays usable.
    fn connection(&self) -> MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Execute a statement that modifies data (INSERT, UPDATE,
    /// DELETE). Returns the number of rows affected.
    ///
    /// Supports `SqlValue::List` parameters — see [`expand_params`]
    /// for how `IN (?)` clauses are handled.
    pub fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<usize> {
        execute_on(&self.connection(), sql, params)
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
        f: impl FnMut(&SqlRow) -> Result<T>,
    ) -> Result<Vec<T>> {
        query_map_on(&self.connection(), sql, params, f)
    }

    // =========================================================
    // Transactions
    //
    // The storage is shared between threads through one
    // connection, and SQLite transactions belong to a connection,
    // not to a thread. A transaction therefore holds the
    // connection lock from BEGIN to COMMIT: statements from other
    // threads wait until it ends instead of joining it and being
    // rolled back with it.
    // =========================================================

    /// Execute `f` inside a transaction. `f` issues its statements
    /// through the `SqlTransaction` it receives. On `Ok` the
    /// transaction commits; on `Err`, or when `f` panics, it rolls
    /// back. Transactions do not nest, and `f` must not call this
    /// `SqlStorage` itself: the connection is locked, so that call
    /// deadlocks or panics.
    ///
    /// The transaction starts with `BEGIN IMMEDIATE`, taking the
    /// database write lock up front, so a second connection to the
    /// same file waits for it under `busy_timeout`.
    ///
    /// ```ignore
    /// storage.transaction(|tx| {
    ///     tx.execute("INSERT INTO t (v) VALUES (?)", &[val("a")])?;
    ///     tx.execute("INSERT INTO t (v) VALUES (?)", &[val("b")])?;
    ///     Ok(())
    /// })?;
    /// ```
    pub fn transaction<T>(&self, f: impl FnOnce(&SqlTransaction<'_>) -> Result<T>) -> Result<T> {
        let mut conn = self.connection();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .context("begin transaction")?;
        let handle = SqlTransaction { tx };

        // Dropping the handle on `Err` rolls the transaction back.
        let value = f(&handle)?;
        handle.tx.commit().context("commit transaction")?;
        Ok(value)
    }
}

/// The statements of one transaction, handed to the closure of
/// [`SqlStorage::transaction`].
pub struct SqlTransaction<'conn> {
    tx: Transaction<'conn>,
}

impl SqlTransaction<'_> {
    /// Same as [`SqlStorage::execute`], inside the transaction.
    pub fn execute(&self, sql: &str, params: &[SqlValue]) -> Result<usize> {
        execute_on(&self.tx, sql, params)
    }

    /// Same as [`SqlStorage::query_map`], inside the transaction.
    pub fn query_map<T>(
        &self,
        sql: &str,
        params: &[SqlValue],
        f: impl FnMut(&SqlRow) -> Result<T>,
    ) -> Result<Vec<T>> {
        query_map_on(&self.tx, sql, params, f)
    }
}

// =========================================================
// Statement execution
// =========================================================

fn execute_on(conn: &Connection, sql: &str, params: &[SqlValue]) -> Result<usize> {
    let (expanded_sql, flat_params) = expand_params(sql, params);
    let param_refs: Vec<&dyn rusqlite::types::ToSql> = flat_params
        .iter()
        .map(|v| v as &dyn rusqlite::types::ToSql)
        .collect();
    conn.execute(&expanded_sql, param_refs.as_slice())
        .context("execute SQL statement")
}

fn query_map_on<T>(
    conn: &Connection,
    sql: &str,
    params: &[SqlValue],
    mut f: impl FnMut(&SqlRow) -> Result<T>,
) -> Result<Vec<T>> {
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
// Connection Confinement
//
// A gadget's `[storage.sql]` grants its own database file and
// nothing else, but SQLite can open further files from SQL:
// `ATTACH` reads and writes any SQLite file the user can
// access and creates new ones, and `VACUUM INTO` writes a copy
// of the database to any path. Both open the other file
// through an attach, which plain `VACUUM` uses as well, so
// plain `VACUUM` is unavailable on these connections.
// =========================================================

/// Deny every attach and detach, and allow no attached
/// database at all as a second barrier should the authorizer
/// ever be missing.
fn confine_connection(conn: &Connection) -> Result<()> {
    conn.set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0)
        .context("limit attached databases")?;
    conn.authorizer(Some(authorize))
        .context("install SQL authorizer")?;
    Ok(())
}

/// SQLite asks this for every action while it prepares a
/// statement; a denied statement fails to prepare with "not
/// authorized" and never runs.
fn authorize(ctx: AuthContext<'_>) -> Authorization {
    match ctx.action {
        // rusqlite reports an attach or detach whose argument is
        // not a string literal (`ATTACH ? AS x`, `ATTACH 'a' || 'b'
        // AS x`) as `Unknown` with the raw action code.
        AuthAction::Attach { .. }
        | AuthAction::Detach { .. }
        | AuthAction::Unknown {
            code: ffi::SQLITE_ATTACH | ffi::SQLITE_DETACH,
            ..
        } => Authorization::Deny,
        _ => Authorization::Allow,
    }
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
        let db_path = dir.path().join("test.sqlite3");

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
        let db_path = dir.path().join("test.sqlite3");

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
        let db_path = dir.path().join("test.sqlite3");

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

    fn items_storage(dir: &tempfile::TempDir) -> SqlStorage {
        SqlStorage::open(
            dir.path().join("test.sqlite3"),
            &["CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT NOT NULL);"],
        )
        .expect("open database")
    }

    fn item_names(storage: &SqlStorage) -> Vec<String> {
        storage
            .query_map("SELECT name FROM items ORDER BY name", &[], |row| {
                row.get(0)
            })
            .expect("query")
    }

    fn insert(tx: &SqlTransaction<'_>, name: &str) -> Result<usize> {
        tx.execute(
            "INSERT INTO items (name) VALUES (?)",
            &[SqlValue::from(name)],
        )
    }

    #[test]
    fn transaction_commit() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);

        storage
            .transaction(|tx| {
                insert(tx, "one")?;
                insert(tx, "two")?;
                Ok(())
            })
            .expect("transaction commit");

        assert_eq!(item_names(&storage), vec!["one", "two"]);
    }

    #[test]
    fn transaction_rollback() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);

        // Insert one row outside the transaction so we can verify
        // it survives the rollback.
        storage
            .execute(
                "INSERT INTO items (name) VALUES (?)",
                &[SqlValue::from("before")],
            )
            .expect("insert before");

        let result: anyhow::Result<()> = storage.transaction(|tx| {
            insert(tx, "inside")?;
            anyhow::bail!("intentional failure");
        });
        assert!(result.is_err());

        // Only the row inserted before the transaction should remain.
        assert_eq!(item_names(&storage), vec!["before"]);
    }

    #[test]
    fn transaction_returns_value() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);

        let count = storage
            .transaction(|tx| {
                insert(tx, "a")?;
                insert(tx, "b")?;
                let rows: Vec<String> =
                    tx.query_map("SELECT name FROM items", &[], |row| row.get(0))?;
                Ok(rows.len())
            })
            .expect("transaction with return value");

        assert_eq!(count, 2);
    }

    #[test]
    fn consecutive_transactions_each_commit() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);

        for i in 0..5 {
            storage
                .transaction(|tx| insert(tx, &format!("item_{i}")))
                .expect("sequential transaction");
        }

        assert_eq!(
            item_names(&storage),
            vec!["item_0", "item_1", "item_2", "item_3", "item_4"]
        );
    }

    #[test]
    fn a_transaction_holds_the_connection_until_it_ends() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);

        storage
            .transaction(|_tx| {
                assert!(
                    storage.conn.try_lock().is_err(),
                    "another thread could run statements inside this transaction"
                );
                Ok(())
            })
            .expect("transaction commits");
    }

    #[test]
    fn a_panic_in_a_transaction_rolls_back_and_leaves_the_storage_usable() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);

        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            storage.transaction(|tx| -> Result<()> {
                insert(tx, "inside")?;
                panic!("closure fails");
            })
        }));
        assert!(outcome.is_err());

        storage
            .execute(
                "INSERT INTO items (name) VALUES (?)",
                &[SqlValue::from("after")],
            )
            .expect("storage works after the panic");
        assert_eq!(item_names(&storage), vec!["after"]);
    }

    #[test]
    fn statements_from_another_thread_wait_for_the_transaction_and_survive_its_rollback() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = std::sync::Arc::new(items_storage(&dir));
        let (go_tx, go_rx) = std::sync::mpsc::channel::<()>();

        let other = {
            let storage = std::sync::Arc::clone(&storage);
            std::thread::spawn(move || {
                go_rx.recv().expect("the transaction signals");
                storage
                    .execute(
                        "INSERT INTO items (name) VALUES (?)",
                        &[SqlValue::from("other thread")],
                    )
                    .expect("insert from the other thread");
            })
        };

        let result: Result<()> = storage.transaction(|tx| {
            insert(tx, "transaction")?;
            go_tx.send(()).expect("the other thread listens");
            anyhow::bail!("roll back");
        });
        assert!(result.is_err());
        other.join().expect("the other thread finishes");

        assert_eq!(item_names(&storage), vec!["other thread"]);
    }

    // =========================================================
    // Connection confinement
    //
    // Every statement below would reach a file other than the
    // storage's own database if SQLite ran it.
    // =========================================================

    /// The full anyhow chain, since the outer context
    /// (`execute SQL statement`) hides SQLite's message.
    fn error_chain(err: &anyhow::Error) -> String {
        format!("{err:#}")
    }

    /// A denial reaches the caller as `SQLITE_AUTH`. Its message
    /// varies: "not authorized" for a denied statement, the
    /// generic "authorization denied" when the denied attach is
    /// the one `VACUUM` runs internally.
    fn assert_denied_err(err: &anyhow::Error) {
        let code = err
            .chain()
            .find_map(|cause| cause.downcast_ref::<rusqlite::Error>())
            .and_then(rusqlite::Error::sqlite_error_code);
        assert_eq!(
            code,
            Some(rusqlite::ErrorCode::AuthorizationForStatementDenied),
            "unexpected error: {err:#}"
        );
    }

    fn assert_denied(result: Result<usize>) {
        assert_denied_err(&result.expect_err("SQLite runs the statement"));
    }

    #[test]
    fn attach_of_a_literal_path_is_denied_and_creates_no_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);
        let other = dir.path().join("other.sqlite3");

        assert_denied(storage.execute(
            &format!("ATTACH DATABASE '{}' AS other", other.display()),
            &[],
        ));
        assert!(!other.exists());
    }

    #[test]
    fn attach_of_a_bound_path_is_denied_and_creates_no_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);
        let other = dir.path().join("other.sqlite3");

        assert_denied(storage.execute(
            "ATTACH DATABASE ? AS other",
            &[SqlValue::from(other.display().to_string())],
        ));
        assert!(!other.exists());
    }

    #[test]
    fn attach_of_a_computed_path_is_denied_and_creates_no_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);
        let other = dir.path().join("other.sqlite3");

        assert_denied(storage.execute(
            &format!(
                "ATTACH DATABASE '{}/' || 'other.sqlite3' AS other",
                dir.path().display()
            ),
            &[],
        ));
        assert!(!other.exists());
    }

    #[test]
    fn attach_through_a_query_is_denied_and_leaves_the_other_database_alone() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);
        let other_dir = tempfile::tempdir().expect("create second temp dir");
        let other = items_storage(&other_dir);
        other
            .execute(
                "INSERT INTO items (name) VALUES (?)",
                &[SqlValue::from("kept")],
            )
            .expect("insert into the other database");
        let other_path = other_dir.path().join("test.sqlite3");

        let err = storage
            .query_map(
                &format!("ATTACH DATABASE '{}' AS other", other_path.display()),
                &[],
                |_| Ok(()),
            )
            .expect_err("SQLite runs the statement");
        assert_denied_err(&err);
        assert_eq!(item_names(&other), vec!["kept"]);
    }

    #[test]
    fn detach_is_denied() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);

        assert_denied(storage.execute("DETACH DATABASE other", &[]));
    }

    #[test]
    fn detach_of_a_bound_name_is_denied() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);

        assert_denied(storage.execute("DETACH DATABASE ?", &[SqlValue::from("other")]));
    }

    #[test]
    fn vacuum_into_is_denied_and_creates_no_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);
        let copy = dir.path().join("copy.sqlite3");

        assert_denied(storage.execute(&format!("VACUUM INTO '{}'", copy.display()), &[]));
        assert!(!copy.exists());
    }

    #[test]
    fn vacuum_into_a_non_utf8_uri_is_denied_and_creates_no_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);
        let copy = dir.path().join("copy.sqlite3");

        // The invalid byte sits in a URI query parameter, so the file
        // name itself stays valid UTF-8 and the file system accepts it.
        // rusqlite cannot hand the filename to the authorizer as a
        // `&str` and fails, which SQLite counts as a denial.
        assert_denied(storage.execute(
            &format!(
                "VACUUM INTO 'file:{}?x=' || CAST(X'ff' AS TEXT)",
                copy.display()
            ),
            &[],
        ));
        assert!(!copy.exists());
    }

    #[test]
    fn plain_vacuum_is_denied() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);

        assert_denied(storage.execute("VACUUM", &[]));
    }

    #[test]
    fn attach_limit_stops_attach_without_the_authorizer() {
        use rusqlite::hooks::{AuthContext, Authorization};

        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);
        let other = dir.path().join("other.sqlite3");

        let conn = storage.connection();
        conn.authorizer(None::<fn(AuthContext<'_>) -> Authorization>)
            .expect("remove the authorizer");
        let err = execute_on(
            &conn,
            &format!("ATTACH DATABASE '{}' AS other", other.display()),
            &[],
        )
        .expect_err("SQLite runs the statement");
        assert!(
            error_chain(&err).contains("too many attached databases - max 0"),
            "unexpected error: {err:#}"
        );
        assert!(!other.exists());
    }

    #[test]
    fn a_migration_that_attaches_fails_open_and_creates_no_file() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let other = dir.path().join("other.sqlite3");

        let result = SqlStorage::open(
            dir.path().join("test.sqlite3"),
            &[&format!("ATTACH DATABASE '{}' AS other;", other.display())],
        );
        let err = match result {
            Ok(_) => panic!("the migration ran"),
            Err(err) => err,
        };
        assert_denied_err(&err);
        assert!(!other.exists());
    }

    #[test]
    fn a_denied_statement_inside_a_transaction_leaves_it_usable() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);
        let other = dir.path().join("other.sqlite3");

        storage
            .transaction(|tx| {
                insert(tx, "before")?;
                assert_denied(tx.execute(
                    &format!("ATTACH DATABASE '{}' AS other", other.display()),
                    &[],
                ));
                insert(tx, "after")?;
                Ok(())
            })
            .expect("transaction commits");

        assert_eq!(item_names(&storage), vec!["after", "before"]);
    }

    #[test]
    fn a_denied_statement_leaves_the_storage_usable() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);
        let other = dir.path().join("other.sqlite3");

        assert_denied(storage.execute(
            &format!("ATTACH DATABASE '{}' AS other", other.display()),
            &[],
        ));
        storage
            .execute(
                "INSERT INTO items (name) VALUES (?)",
                &[SqlValue::from("after")],
            )
            .expect("insert after the denied statement");
        assert_eq!(item_names(&storage), vec!["after"]);
    }

    #[test]
    fn load_extension_is_unavailable() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let storage = items_storage(&dir);

        let result = storage.query_map(
            &format!(
                "SELECT load_extension('{}')",
                dir.path().join("extension").display()
            ),
            &[],
            |_| Ok(()),
        );
        assert!(result.is_err());
    }
}
