// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Frecency Database Schema
//
// Single migration that creates the frecency_events table
// and its age-sweep index. See the plan for rationale on
// the WITHOUT ROWID layout and composite primary key.
// =========================================================

/// Schema migrations for the frecency database. Each entry is
/// one forward migration step, applied in order by SqlStorage.
pub const MIGRATIONS: &[&str] = &[
    // Migration 1: initial schema
    //
    // WITHOUT ROWID stores rows directly in the PRIMARY KEY B-tree
    // instead of a separate rowid heap + secondary index. Since our
    // three columns ARE the primary key, this eliminates the redundant
    // rowid-based table storage entirely.
    //
    // All lookup queries filter by (plugin_id, item_id) which is a
    // prefix of the PK, so they're served directly from this single
    // B-tree with no extra index needed.
    //
    // The standalone timestamp index supports the startup age sweep
    // (WHERE timestamp < cutoff) which the PK ordering cannot
    // accelerate.
    "CREATE TABLE frecency_events (
        plugin_id TEXT    NOT NULL,
        item_id   TEXT    NOT NULL,
        timestamp INTEGER NOT NULL,
        PRIMARY KEY (plugin_id, item_id, timestamp)
    ) WITHOUT ROWID;

    CREATE INDEX idx_frecency_age
        ON frecency_events (timestamp);",
];
