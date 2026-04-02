// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Bangs — JSON Parsing & SQL Import
//
// Parses the DDG bang.js JSON array and bulk-imports it into the
// plugin's SqlStorage. Each import runs inside a single savepoint
// transaction for performance (~13k INSERTs).
// =========================================================

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::storage::SqlStorage;
use crate::storage::SqlValue;

// =========================================================
// JSON Schema
//
// Each entry in DDG's bang.js is a flat object with short field
// names. We deserialize into a typed struct and map to our SQL
// schema during import.
// =========================================================

/// A single entry from DuckDuckGo's bang.js JSON array.
///
/// Field names match the DDG format exactly:
/// - `t`: trigger (what you type after `!`)
/// - `s`: service name (human-readable)
/// - `u`: URL template (contains `{{{s}}}` placeholder)
/// - `d`: domain of the target service
/// - `c`: category (e.g. "Tech", "Entertainment")
/// - `sc`: subcategory (e.g. "Downloads", "Forum")
/// - `r`: relevance/popularity rank (higher = more popular)
#[derive(Debug, Deserialize)]
pub struct BangEntry {
    pub t: String,
    pub s: String,
    pub u: String,
    pub d: String,
    #[serde(default)]
    pub c: Option<String>,
    #[serde(default)]
    pub sc: Option<String>,
    #[serde(default)]
    pub r: i64,
}

/// Parse a bang.js JSON string into a list of bang entries.
pub fn parse_bang_json(json: &str) -> Result<Vec<BangEntry>> {
    serde_json::from_str(json).context("parse bang.js JSON")
}

// =========================================================
// SQL Import
// =========================================================

/// Import a list of parsed bang entries into the SQL database,
/// replacing any existing data. Records metadata about the import
/// for the settings UI.
///
/// `source` should be `"network"` or `"builtin"` to indicate
/// where the data came from.
pub fn import_bangs(sql: &SqlStorage, entries: &[BangEntry], source: &str) -> Result<()> {
    sql.transaction(|| {
        // Clear existing data before re-importing.
        sql.execute("DELETE FROM bangs", &[])
            .context("clear existing bangs")?;
        sql.execute("DELETE FROM metadata", &[])
            .context("clear existing metadata")?;

        // Bulk-insert all bang entries.
        for entry in entries {
            sql.execute(
                "INSERT OR IGNORE INTO bangs (trigger, service_name, url_template, domain, category, subcategory, rank)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                &[
                    SqlValue::from(entry.t.as_str()),
                    SqlValue::from(entry.s.as_str()),
                    SqlValue::from(entry.u.as_str()),
                    SqlValue::from(entry.d.as_str()),
                    SqlValue::from(entry.c.as_deref()),
                    SqlValue::from(entry.sc.as_deref()),
                    SqlValue::from(entry.r),
                ],
            )
            .context("insert bang entry")?;
        }

        // Compute and store import metadata.
        let bang_count: Vec<i64> = sql
            .query_map("SELECT COUNT(*) FROM bangs", &[], |row| row.get(0))
            .context("count bangs")?;
        let domain_count: Vec<i64> = sql
            .query_map(
                "SELECT COUNT(DISTINCT domain) FROM bangs",
                &[],
                |row| row.get(0),
            )
            .context("count distinct domains")?;

        // Use SQLite's strftime for the import timestamp, matching the
        // ISO 8601 format used by the clipboard and calculator plugins.
        sql.execute(
            "INSERT INTO metadata (key, value) VALUES ('import_date', strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))",
            &[],
        )
        .context("insert metadata import_date")?;

        for (key, value) in [
            ("source", source),
            ("bang_count", &bang_count[0].to_string()),
            ("domain_count", &domain_count[0].to_string()),
        ] {
            sql.execute(
                "INSERT INTO metadata (key, value) VALUES (?1, ?2)",
                &[SqlValue::from(key), SqlValue::from(value)],
            )
            .with_context(|| format!("insert metadata key {key}"))?;
        }

        Ok(())
    })
}
