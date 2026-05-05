// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Plugin-owned `networks` history table.
//!
//! Persists every network the daemon has ever surfaced plus
//! every entry imported from the macOS UI's
//! `saved_networks.json`. Joined networks are upserted on
//! every observation; entries that have been left
//! (`DELETE /network/{id}`) survive as
//! "Stored, not currently joined" so the launcher can
//! still surface them as candidates for re-Connect.
//!
//! Schema: `id TEXT PRIMARY KEY, name TEXT, first_seen INTEGER,
//! last_seen INTEGER, last_status TEXT NULL,
//! last_snapshot TEXT NULL` (see `migrations/001_init.sql`).

use serde::Deserialize;
use torchsnap_gadget_sdk::sql::{self, SqlHandle, SqlValue, query_all};

use crate::api::Network;

/// One row from the `networks` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRow {
    pub id: String,
    pub name: String,
    pub first_seen: i64,
    pub last_seen: i64,
    pub last_status: Option<String>,
    pub last_snapshot: Option<String>,
}

/// Insert or update a row from a live daemon observation.
/// `now_ms` is injected rather than read from a clock so tests
/// stay deterministic and the plugin doesn't need a wall-clock
/// host import for what is really just a free-running counter.
pub fn upsert_observed(db: &SqlHandle, net: &Network, now_ms: i64) -> Result<(), String> {
    let snapshot =
        serde_json::to_string(net).map_err(|e| format!("serialize network snapshot: {e}"))?;
    // `INSERT ... ON CONFLICT(id) DO UPDATE` keeps `first_seen`
    // pinned to the original timestamp and rolls forward only
    // the live-state columns.
    // The `CASE` on `name` preserves a previously-captured
    // name when the current observation carries an empty one
    // (typical right after a first-time Connect, when the
    // daemon is still in `RequestingConfiguration` and hasn't
    // pulled the network's config from the controller yet).
    // Without it, every empty-name observation would erase a
    // name we'd already learned, forcing the user back to
    // searching by id.
    db.execute(
        "INSERT INTO networks
         (id, name, first_seen, last_seen, last_status, last_snapshot)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(id) DO UPDATE SET
             name          = CASE WHEN excluded.name = ''
                                  THEN name
                                  ELSE excluded.name
                             END,
             last_seen     = excluded.last_seen,
             last_status   = excluded.last_status,
             last_snapshot = excluded.last_snapshot",
        &[
            SqlValue::from(net.id.as_str()),
            SqlValue::from(net.name.as_str()),
            SqlValue::from(now_ms),
            SqlValue::from(now_ms),
            SqlValue::from(serde_json::to_string(&net.status).unwrap_or_default()),
            SqlValue::from(snapshot),
        ],
    )?;
    Ok(())
}

/// Read every row, newest-seen first.
pub fn list_all(db: &SqlHandle) -> Result<Vec<HistoryRow>, String> {
    let rows = query_all(
        db,
        "SELECT id, name, first_seen, last_seen, last_status, last_snapshot
         FROM networks
         ORDER BY last_seen DESC",
        &[],
    )?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            Some(HistoryRow {
                id: row.text(0)?.to_string(),
                name: row.text(1)?.to_string(),
                first_seen: row.integer(2)?,
                last_seen: row.integer(3)?,
                last_status: row.text(4).map(|s| s.to_string()),
                last_snapshot: row.text(5).map(|s| s.to_string()),
            })
        })
        .collect())
}

/// Drop a single row. Used by Forget actions.
pub fn forget(db: &SqlHandle, id: &str) -> Result<(), String> {
    db.execute("DELETE FROM networks WHERE id = ?", &[SqlValue::from(id)])?;
    Ok(())
}

/// Drop every row. Used by the settings panel's "Clear all".
pub fn clear_all(db: &SqlHandle) -> Result<(), String> {
    db.execute("DELETE FROM networks", &[])?;
    Ok(())
}

/// Merge entries from the macOS UI's `saved_networks.json`
/// into the table. `INSERT OR IGNORE` semantics — entries the
/// plugin already knows about are not overwritten so daemon
/// observations remain authoritative once we have them.
/// Returns the number of newly-inserted rows.
pub fn import_saved_networks(
    db: &SqlHandle,
    json: &str,
    now_ms: i64,
) -> Result<usize, String> {
    let entries = parse_saved_networks(json)?;
    let mut inserted = 0;
    for entry in entries {
        let result = db.execute(
            "INSERT OR IGNORE INTO networks
             (id, name, first_seen, last_seen, last_status, last_snapshot)
             VALUES (?, ?, ?, ?, NULL, NULL)",
            &[
                SqlValue::from(entry.id.as_str()),
                SqlValue::from(entry.name.as_str()),
                SqlValue::from(now_ms),
                SqlValue::from(now_ms),
            ],
        )?;
        // `db.execute` returns rows-affected; `INSERT OR IGNORE`
        // returns 0 when the row already existed.
        if result > 0 {
            inserted += 1;
        }
    }
    Ok(inserted)
}

/// Decoded entry from `saved_networks.json`. The macOS UI
/// stores each value as a flat-keyed object; the inner
/// `settings` field holds a stringified `Network` JSON we do
/// not currently need (we only seed the history with id+name
/// and pick up live state on the next refresh).
#[derive(Debug, Deserialize)]
struct SavedNetworkEntry {
    id: String,
    #[serde(default)]
    name: String,
}

fn parse_saved_networks(json: &str) -> Result<Vec<SavedNetworkEntry>, String> {
    // The file's top-level shape is
    // `{ "<id>": { id, name, settings }, ... }`. We only need
    // the values, so collect them after parsing.
    let map: std::collections::HashMap<String, SavedNetworkEntry> =
        serde_json::from_str(json).map_err(|e| format!("parse saved_networks.json: {e}"))?;
    Ok(map.into_values().collect())
}

/// Re-export so the lib crate has a single import surface.
pub use sql::connection;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_saved_networks_canonical_shape() {
        let json = r#"{
            "abcdef0123456789": {
                "id": "abcdef0123456789",
                "name": "homenet",
                "settings": "{ ... stringified Network JSON ... }"
            },
            "fedcba9876543210": {
                "id": "fedcba9876543210",
                "name": "officevpn",
                "settings": "{}"
            }
        }"#;
        let mut entries = parse_saved_networks(json).expect("parse");
        entries.sort_by(|a, b| a.id.cmp(&b.id));
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "abcdef0123456789");
        assert_eq!(entries[0].name, "homenet");
        assert_eq!(entries[1].id, "fedcba9876543210");
        assert_eq!(entries[1].name, "officevpn");
    }

    #[test]
    fn parses_empty_saved_networks_object() {
        let entries = parse_saved_networks("{}").expect("parse");
        assert!(entries.is_empty());
    }

    #[test]
    fn rejects_malformed_saved_networks() {
        assert!(parse_saved_networks("not json").is_err());
        assert!(parse_saved_networks("[]").is_err()); // top-level must be an object
    }

    #[test]
    fn entry_with_missing_name_defaults_to_empty_string() {
        let json = r#"{"x":{"id":"abcdef0123456789"}}"#;
        let entries = parse_saved_networks(json).expect("parse");
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "abcdef0123456789");
        assert_eq!(entries[0].name, "");
    }
}
