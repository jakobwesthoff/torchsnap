// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Ergonomic wrappers around the host's `sql` interface.
//!
//! The WIT `sql-handle` resource exposes a raw
//! `query(sql, params) -> Vec<Vec<SqlValue>>` API; real plugin
//! code always wants typed column access, so this module
//! layers:
//!
//! - A `Row` newtype with `integer` / `text` / `real` / `blob`
//!   / `is_null` accessors.
//! - `query_one` / `query_all` helpers that hand back `Row`s.
//! - `From` conversions for `SqlValue`, so
//!   `vec![SqlValue::from("x"), SqlValue::from(42_i64)]` works
//!   without hand-written `Text(..)` / `Integer(..)` calls.
//!
//! Higher-level patterns (`derive(FromRow)`,
//! `transaction(|tx| …)`, named bind parameters) are left for
//! a future iteration once plugin authors actually motivate
//! them.

pub use crate::torchsnap::gadget::sql::{SqlHandle, SqlValue, connection};

// =========================================================
// Row — typed read-side accessor
// =========================================================

/// A single query result row, positional by column index.
///
/// Each accessor returns `None` when the column doesn't exist
/// OR when the stored value is of a different storage class;
/// plugins that need to distinguish "missing column" from
/// "wrong type" can match on the underlying `Vec<SqlValue>`
/// directly (it's a public field).
pub struct Row(pub Vec<SqlValue>);

impl Row {
    /// Read a column as an `INTEGER`. Returns `None` for any
    /// non-integer storage class (including `NULL`).
    pub fn integer(&self, idx: usize) -> Option<i64> {
        match self.0.get(idx) {
            Some(SqlValue::Integer(n)) => Some(*n),
            _ => None,
        }
    }

    /// Read a column as a `TEXT`. Returns `None` for non-text
    /// values (including `NULL`).
    pub fn text(&self, idx: usize) -> Option<&str> {
        match self.0.get(idx) {
            Some(SqlValue::Text(s)) => Some(s.as_str()),
            _ => None,
        }
    }

    /// Read a column as a `REAL`. Returns `None` for non-real
    /// values (including `NULL`).
    pub fn real(&self, idx: usize) -> Option<f64> {
        match self.0.get(idx) {
            Some(SqlValue::Real(f)) => Some(*f),
            _ => None,
        }
    }

    /// Read a column as a `BLOB`. Returns `None` for non-blob
    /// values (including `NULL`).
    pub fn blob(&self, idx: usize) -> Option<&[u8]> {
        match self.0.get(idx) {
            Some(SqlValue::Blob(b)) => Some(b.as_slice()),
            _ => None,
        }
    }

    /// `true` for `NULL` columns AND for columns beyond the
    /// row's width. The latter keeps callers that scan a
    /// sparse-ish column list from tripping on index errors.
    pub fn is_null(&self, idx: usize) -> bool {
        matches!(self.0.get(idx), Some(SqlValue::Null) | None)
    }
}

// =========================================================
// SqlValue construction
//
// `From` instead of a trait full of `v()` / `to_sql()`
// helpers — plugin call sites then read
// `&[SqlValue::from(expr), SqlValue::from(days)]` which is
// marginally more keystrokes than a custom helper but
// doesn't pollute the namespace, and survives type inference
// so literal integers need an explicit suffix.
// =========================================================

impl From<&str> for SqlValue {
    fn from(s: &str) -> Self {
        SqlValue::Text(s.to_owned())
    }
}

impl From<String> for SqlValue {
    fn from(s: String) -> Self {
        SqlValue::Text(s)
    }
}

impl From<i64> for SqlValue {
    fn from(n: i64) -> Self {
        SqlValue::Integer(n)
    }
}

impl From<f64> for SqlValue {
    fn from(f: f64) -> Self {
        SqlValue::Real(f)
    }
}

impl From<Vec<u8>> for SqlValue {
    fn from(b: Vec<u8>) -> Self {
        SqlValue::Blob(b)
    }
}

// =========================================================
// Query helpers
//
// `query_all` / `query_one` wrap the raw handle call and
// collect rows into the `Row` newtype so column access goes
// through the typed accessors above.
// =========================================================

/// Run a SELECT and collect every row.
pub fn query_all(db: &SqlHandle, sql: &str, params: &[SqlValue]) -> Result<Vec<Row>, String> {
    db.query(sql, params)
        .map(|rows| rows.into_iter().map(Row).collect())
}

/// Run a SELECT and return the first row (or `None` if
/// empty). Convenience wrapper around [`query_all`].
pub fn query_one(db: &SqlHandle, sql: &str, params: &[SqlValue]) -> Result<Option<Row>, String> {
    Ok(query_all(db, sql, params)?.into_iter().next())
}
