// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! SQLite cache layer for website metadata.
//!
//! Stores domain-keyed metadata (title, description, favicon reference)
//! with time-based expiration. Favicon image files are stored separately
//! by the `FaviconStore`; this module tracks the `favicon_key` and
//! `favicon_ext` that link a domain to its cached icon file.

use crate::storage::{SqlStorage, SqlValue};

// =========================================================
// Schema
// =========================================================

pub const MIGRATION_001: &str = "\
CREATE TABLE website_metadata (
    domain       TEXT PRIMARY KEY,
    title        TEXT,
    description  TEXT,
    favicon_url  TEXT,
    favicon_key  TEXT,
    favicon_ext  TEXT,
    reachable    INTEGER NOT NULL DEFAULT 1,
    fetched_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);\
";

// =========================================================
// Types
// =========================================================

/// A cached metadata row retrieved from the database.
pub struct CachedEntry {
    pub title: Option<String>,
    pub description: Option<String>,
    pub favicon_key: Option<String>,
    pub favicon_ext: Option<String>,
    pub reachable: bool,
}

/// Summary statistics for the settings UI.
pub struct CacheStats {
    pub entry_count: i64,
    /// Total size of all cached favicon files on disk (bytes).
    pub favicon_bytes: u64,
}

// =========================================================
// Cache operations
// =========================================================

/// Look up a cached entry by domain, respecting the TTL.
///
/// Returns `None` if no row exists or the row has expired.
pub fn lookup(db: &SqlStorage, domain: &str, ttl_days: u32) -> Option<CachedEntry> {
    db.query_map(
        "SELECT title, description, favicon_key, favicon_ext, reachable \
         FROM website_metadata \
         WHERE domain = ? \
           AND fetched_at >= strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?)",
        &[
            SqlValue::from(domain.to_string()),
            SqlValue::from(format!("-{ttl_days} days")),
        ],
        |row| {
            Ok(CachedEntry {
                title: row.get(0).ok(),
                description: row.get(1).ok(),
                favicon_key: row.get(2).ok(),
                favicon_ext: row.get(3).ok(),
                reachable: row.get::<i64>(4).map(|v| v != 0).unwrap_or(true),
            })
        },
    )
    .ok()
    .and_then(|mut rows| rows.pop())
}

/// Insert or replace a metadata entry for a domain.
pub fn store(
    db: &SqlStorage,
    domain: &str,
    title: Option<&str>,
    description: Option<&str>,
    favicon_url: Option<&str>,
    favicon_key: Option<&str>,
    favicon_ext: Option<&str>,
    reachable: bool,
) {
    let _ = db.execute(
        "INSERT OR REPLACE INTO website_metadata \
             (domain, title, description, favicon_url, favicon_key, favicon_ext, reachable, fetched_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
        &[
            SqlValue::from(domain.to_string()),
            title.map(|s| SqlValue::from(s.to_string())).unwrap_or(SqlValue::Null),
            description.map(|s| SqlValue::from(s.to_string())).unwrap_or(SqlValue::Null),
            favicon_url.map(|s| SqlValue::from(s.to_string())).unwrap_or(SqlValue::Null),
            favicon_key.map(|s| SqlValue::from(s.to_string())).unwrap_or(SqlValue::Null),
            favicon_ext.map(|s| SqlValue::from(s.to_string())).unwrap_or(SqlValue::Null),
            SqlValue::from(if reachable { 1i64 } else { 0i64 }),
        ],
    );
}

/// Delete expired entries and return the favicon keys of remaining
/// (non-expired) rows. Callers use this set for `FaviconStore::cleanup`.
pub fn evict_expired(db: &SqlStorage, ttl_days: u32) -> Vec<String> {
    // Delete expired rows.
    let _ = db.execute(
        "DELETE FROM website_metadata \
         WHERE fetched_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?)",
        &[SqlValue::from(format!("-{ttl_days} days"))],
    );

    // Collect remaining favicon keys for cleanup validation.
    db.query_map(
        "SELECT favicon_key FROM website_metadata WHERE favicon_key IS NOT NULL",
        &[],
        |row| Ok(row.get::<String>(0).unwrap_or_default()),
    )
    .unwrap_or_default()
}

/// Remove all cached entries.
pub fn clear_all(db: &SqlStorage) -> anyhow::Result<()> {
    db.execute("DELETE FROM website_metadata", &[])?;
    Ok(())
}

/// Gather summary statistics for the settings UI.
pub fn stats(db: &SqlStorage) -> CacheStats {
    let entry_count = db
        .query_map(
            "SELECT COUNT(*) FROM website_metadata",
            &[],
            |row| Ok(row.get::<i64>(0).unwrap_or(0)),
        )
        .ok()
        .and_then(|mut rows| rows.pop())
        .unwrap_or(0);

    CacheStats {
        entry_count,
        // Favicon disk usage is computed by the service layer,
        // which has access to the favicon store.
        favicon_bytes: 0,
    }
}
