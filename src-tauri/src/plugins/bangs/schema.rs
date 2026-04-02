// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Bangs — Database Schema
// =========================================================

pub const PLUGIN_ID: &str = "bangs";

/// The bang database bundled into the binary at compile time. Used as
/// a fallback when the plugin cannot fetch a fresh copy from DDG at
/// startup.
pub const BAKED_IN_BANGS: &str = include_str!("../../../derived/bang.json");

/// Initial schema: bang lookup table and key-value metadata store.
///
/// `trigger` uses `COLLATE NOCASE` so lookups are case-insensitive
/// without needing `LOWER()` in every query — matching DDG's own
/// behaviour.
pub const MIGRATION_001: &str = "
    CREATE TABLE bangs (
        trigger TEXT PRIMARY KEY COLLATE NOCASE,
        service_name TEXT NOT NULL,
        url_template TEXT NOT NULL,
        domain TEXT NOT NULL,
        category TEXT,
        subcategory TEXT,
        rank INTEGER NOT NULL DEFAULT 0
    );

    CREATE TABLE metadata (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
";
