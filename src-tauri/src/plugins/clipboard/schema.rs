// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Clipboard Schema & Types
//
// Database migration SQL, serialized types for frontend
// communication, and plugin-level constants.
// =========================================================

use serde::{Deserialize, Serialize};

// =========================================================
// Constants
// =========================================================

pub const PLUGIN_ID: &str = "clipboard-manager";

/// Entries older than this many days are deleted on startup
/// and periodically during operation.
pub const RETENTION_DAYS: u32 = 30;

/// Run retention cleanup every N captured entries.
pub const RETENTION_INTERVAL: u32 = 100;

// =========================================================
// Database Migration
// =========================================================

/// Single migration that creates the clipboard schema including
/// FTS5 for full-text search and triggers to keep the index in
/// sync with the content table.
pub const MIGRATION_001: &str = "
CREATE TABLE clipboard_entries (
    id          TEXT PRIMARY KEY,
    captured_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    preview     TEXT NOT NULL DEFAULT ''
);

CREATE INDEX idx_entries_captured_at ON clipboard_entries(captured_at DESC);

CREATE TABLE clipboard_content (
    entry_id    TEXT NOT NULL REFERENCES clipboard_entries(id) ON DELETE CASCADE,
    format      TEXT NOT NULL,
    text_value  TEXT,
    file_key    TEXT,
    PRIMARY KEY (entry_id, format)
);

CREATE VIRTUAL TABLE clipboard_fts USING fts5(
    preview,
    content='clipboard_entries',
    content_rowid='rowid'
);

CREATE TRIGGER clipboard_fts_insert AFTER INSERT ON clipboard_entries BEGIN
    INSERT INTO clipboard_fts(rowid, preview) VALUES (new.rowid, new.preview);
END;

CREATE TRIGGER clipboard_fts_delete AFTER DELETE ON clipboard_entries BEGIN
    INSERT INTO clipboard_fts(clipboard_fts, rowid, preview)
        VALUES ('delete', old.rowid, old.preview);
END;
";

// =========================================================
// Serialized Types (sent to/from the frontend)
// =========================================================

/// Lightweight entry used for list display. Sent via subscribe/notify
/// to keep payload size minimal — detail data is loaded on demand via
/// `load_full_entry`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardListEntry {
    pub id: String,
    pub captured_at: String,
    /// Truncated to [`LIST_PREVIEW_MAX_CHARS`] characters.
    pub preview: String,
    /// Highest-priority format for this entry, used to determine
    /// the list icon (e.g. image vs text vs files).
    pub primary_format: String,
}

/// Maximum preview length transmitted for list entries.
pub const LIST_PREVIEW_MAX_CHARS: usize = 40;

/// Full entry returned by `load_full_entry` when the user selects an
/// entry in the list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardHistoryEntry {
    pub id: String,
    pub captured_at: String,
    /// Truncated to [`DETAIL_PREVIEW_MAX_CHARS`] characters.
    pub preview: String,
    pub formats: Vec<String>,
    /// Absolute path to the image file, if this entry has an
    /// image format. The frontend converts this to an asset URL.
    pub image_path: Option<String>,
}

/// Maximum preview length transmitted for detail entries.
pub const DETAIL_PREVIEW_MAX_CHARS: usize = 1000;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscribePayload {
    #[serde(default)]
    pub query: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntryIdPayload {
    pub id: String,
}
