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
// All public methods return `anyhow::Result` so callers can
// attach context without dealing with `rusqlite::Error`
// directly.
// =========================================================

use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{Context, Result};
use rusqlite::types::ToSql;
use rusqlite::{Connection, OptionalExtension, Row};
use rusqlite_migration::{Migrations, M};

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

        let migration_set: Vec<M<'_>> = migrations.iter().map(|sql| M::up(*sql)).collect();
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
    pub fn execute(&self, sql: &str, params: &[&dyn ToSql]) -> Result<usize> {
        let conn = self.conn.lock().expect("sql connection not poisoned");
        conn.execute(sql, params)
            .context("execute SQL statement")
    }

    /// Execute a query and map each result row into `T` using
    /// the provided closure.
    pub fn query_map<T>(
        &self,
        sql: &str,
        params: &[&dyn ToSql],
        f: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Vec<T>> {
        let conn = self.conn.lock().expect("sql connection not poisoned");
        let mut stmt = conn.prepare(sql).context("prepare SQL query")?;
        let rows = stmt
            .query_map(params, f)
            .context("execute SQL query")?;

        rows.collect::<rusqlite::Result<Vec<T>>>()
            .context("collect query results")
    }

    /// Execute a query expected to return zero or one row.
    /// Returns `Ok(None)` if the query produces no results.
    pub fn query_optional<T>(
        &self,
        sql: &str,
        params: &[&dyn ToSql],
        f: impl FnOnce(&Row<'_>) -> rusqlite::Result<T>,
    ) -> Result<Option<T>> {
        let conn = self.conn.lock().expect("sql connection not poisoned");
        let mut stmt = conn.prepare(sql).context("prepare SQL query")?;
        stmt.query_row(params, f)
            .optional()
            .context("execute optional query")
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
                &[&"hello" as &dyn ToSql],
            )
            .expect("insert row");
        assert_eq!(affected, 1);

        let names: Vec<String> = storage
            .query_map("SELECT name FROM items", &[], |row| row.get(0))
            .expect("query rows");
        assert_eq!(names, vec!["hello".to_string()]);

        let found: Option<String> = storage
            .query_optional("SELECT name FROM items WHERE id = ?1", &[&1i64 as &dyn ToSql], |row| {
                row.get(0)
            })
            .expect("optional query");
        assert_eq!(found, Some("hello".to_string()));

        let missing: Option<String> = storage
            .query_optional("SELECT name FROM items WHERE id = ?1", &[&999i64 as &dyn ToSql], |row| {
                row.get(0)
            })
            .expect("optional query for missing row");
        assert_eq!(missing, None);
    }
}
