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

/// Data to insert or replace for a single domain.
pub struct CacheEntry<'a> {
    pub domain: &'a str,
    pub title: Option<&'a str>,
    pub description: Option<&'a str>,
    pub favicon_url: Option<&'a str>,
    pub favicon_key: Option<&'a str>,
    pub favicon_ext: Option<&'a str>,
    pub reachable: bool,
}

/// Insert or replace a metadata entry for a domain.
pub fn store(db: &SqlStorage, entry: &CacheEntry<'_>) {
    let _ = db.execute(
        "INSERT OR REPLACE INTO website_metadata \
             (domain, title, description, favicon_url, favicon_key, favicon_ext, reachable, fetched_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
        &[
            SqlValue::from(entry.domain.to_string()),
            entry.title.map(|s| SqlValue::from(s.to_string())).unwrap_or(SqlValue::Null),
            entry.description.map(|s| SqlValue::from(s.to_string())).unwrap_or(SqlValue::Null),
            entry.favicon_url.map(|s| SqlValue::from(s.to_string())).unwrap_or(SqlValue::Null),
            entry.favicon_key.map(|s| SqlValue::from(s.to_string())).unwrap_or(SqlValue::Null),
            entry.favicon_ext.map(|s| SqlValue::from(s.to_string())).unwrap_or(SqlValue::Null),
            SqlValue::from(if entry.reachable { 1i64 } else { 0i64 }),
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
        .query_map("SELECT COUNT(*) FROM website_metadata", &[], |row| {
            Ok(row.get::<i64>(0).unwrap_or(0))
        })
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

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn open_db(tmp: &TempDir) -> SqlStorage {
        SqlStorage::open(tmp.path().join("test.sqlite3"), &[MIGRATION_001])
            .expect("open test sqlite db")
    }

    /// Insert a row whose `fetched_at` is `n_days` days in the past.
    /// Bypasses `store()` because that pins `fetched_at` to "now".
    fn insert_with_age(db: &SqlStorage, domain: &str, n_days: i64) {
        db.execute(
            "INSERT OR REPLACE INTO website_metadata \
                 (domain, title, description, favicon_url, favicon_key, favicon_ext, reachable, fetched_at) \
             VALUES (?, ?, NULL, NULL, ?, ?, 1, strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?))",
            &[
                SqlValue::from(domain.to_string()),
                SqlValue::from(format!("title-{domain}")),
                SqlValue::from(format!("key-{domain}")),
                SqlValue::from("webp".to_string()),
                SqlValue::from(format!("-{n_days} days")),
            ],
        )
        .expect("insert with backdated fetched_at");
    }

    fn fresh_entry(domain: &str) -> CacheEntry<'_> {
        CacheEntry {
            domain,
            title: Some("Title"),
            description: Some("Desc"),
            favicon_url: Some("https://example.test/icon.png"),
            favicon_key: Some("abc123"),
            favicon_ext: Some("webp"),
            reachable: true,
        }
    }

    #[test]
    fn lookup_returns_none_for_unknown_domain() {
        let tmp = TempDir::new().expect("temp dir");
        let db = open_db(&tmp);
        assert!(lookup(&db, "missing.test", 30).is_none());
    }

    #[test]
    fn lookup_returns_some_for_fresh_entry() {
        let tmp = TempDir::new().expect("temp dir");
        let db = open_db(&tmp);
        store(&db, &fresh_entry("present.test"));

        let entry = lookup(&db, "present.test", 30).expect("entry exists");
        assert_eq!(entry.title.as_deref(), Some("Title"));
        assert_eq!(entry.description.as_deref(), Some("Desc"));
        assert_eq!(entry.favicon_key.as_deref(), Some("abc123"));
        assert_eq!(entry.favicon_ext.as_deref(), Some("webp"));
        assert!(entry.reachable);
    }

    #[test]
    fn lookup_returns_none_for_expired_entry() {
        let tmp = TempDir::new().expect("temp dir");
        let db = open_db(&tmp);
        // Inserted 10 days ago; TTL of 1 day → expired.
        insert_with_age(&db, "expired.test", 10);
        assert!(lookup(&db, "expired.test", 1).is_none());
    }

    #[test]
    fn lookup_returns_some_for_entry_within_ttl() {
        let tmp = TempDir::new().expect("temp dir");
        let db = open_db(&tmp);
        // Inserted 5 days ago; TTL of 30 days → still fresh.
        insert_with_age(&db, "fresh.test", 5);
        assert!(lookup(&db, "fresh.test", 30).is_some());
    }

    #[test]
    fn lookup_returns_unreachable_flag_correctly() {
        let tmp = TempDir::new().expect("temp dir");
        let db = open_db(&tmp);
        store(
            &db,
            &CacheEntry {
                domain: "unreachable.test",
                title: None,
                description: None,
                favicon_url: None,
                favicon_key: None,
                favicon_ext: None,
                reachable: false,
            },
        );

        let entry = lookup(&db, "unreachable.test", 30).expect("row exists");
        assert!(!entry.reachable);
    }

    #[test]
    fn store_replaces_existing_row_for_same_domain() {
        let tmp = TempDir::new().expect("temp dir");
        let db = open_db(&tmp);
        store(&db, &fresh_entry("dup.test"));
        store(
            &db,
            &CacheEntry {
                domain: "dup.test",
                title: Some("New Title"),
                description: Some("New Desc"),
                favicon_url: Some("https://example.test/new.png"),
                favicon_key: Some("xyz999"),
                favicon_ext: Some("webp"),
                reachable: true,
            },
        );

        let entry = lookup(&db, "dup.test", 30).expect("row exists");
        assert_eq!(entry.title.as_deref(), Some("New Title"));
        assert_eq!(entry.favicon_key.as_deref(), Some("xyz999"));

        // Total row count is still 1 — INSERT OR REPLACE updated, not added.
        let stats = stats(&db);
        assert_eq!(stats.entry_count, 1);
    }

    #[test]
    fn evict_expired_removes_only_stale_rows() {
        let tmp = TempDir::new().expect("temp dir");
        let db = open_db(&tmp);
        store(&db, &fresh_entry("keep.test"));
        insert_with_age(&db, "drop.test", 100);

        let remaining_keys = evict_expired(&db, 30);

        // The fresh row's favicon_key is returned; the stale one is gone.
        assert!(lookup(&db, "keep.test", 30).is_some());
        assert!(lookup(&db, "drop.test", 30).is_none());
        assert_eq!(remaining_keys, vec!["abc123".to_string()]);
    }

    #[test]
    fn evict_expired_skips_rows_with_null_favicon_key() {
        let tmp = TempDir::new().expect("temp dir");
        let db = open_db(&tmp);
        store(
            &db,
            &CacheEntry {
                domain: "no-icon.test",
                title: Some("X"),
                description: None,
                favicon_url: None,
                favicon_key: None,
                favicon_ext: None,
                reachable: true,
            },
        );

        let remaining = evict_expired(&db, 30);
        // Row is kept, but it has no favicon to track.
        assert!(remaining.is_empty());
        assert!(lookup(&db, "no-icon.test", 30).is_some());
    }

    #[test]
    fn clear_all_empties_table() {
        let tmp = TempDir::new().expect("temp dir");
        let db = open_db(&tmp);
        store(&db, &fresh_entry("a.test"));
        store(&db, &fresh_entry("b.test"));

        clear_all(&db).expect("clear_all succeeds");
        assert_eq!(stats(&db).entry_count, 0);
    }

    #[test]
    fn stats_returns_correct_entry_count() {
        let tmp = TempDir::new().expect("temp dir");
        let db = open_db(&tmp);
        assert_eq!(stats(&db).entry_count, 0);

        store(&db, &fresh_entry("one.test"));
        store(&db, &fresh_entry("two.test"));
        store(&db, &fresh_entry("three.test"));
        assert_eq!(stats(&db).entry_count, 3);
    }

    #[test]
    fn store_and_lookup_handle_optional_fields_as_null() {
        let tmp = TempDir::new().expect("temp dir");
        let db = open_db(&tmp);
        store(
            &db,
            &CacheEntry {
                domain: "sparse.test",
                title: None,
                description: None,
                favicon_url: None,
                favicon_key: None,
                favicon_ext: None,
                reachable: true,
            },
        );

        let entry = lookup(&db, "sparse.test", 30).expect("row exists");
        assert!(entry.title.is_none());
        assert!(entry.description.is_none());
        assert!(entry.favicon_key.is_none());
        assert!(entry.favicon_ext.is_none());
        assert!(entry.reachable);
    }
}
