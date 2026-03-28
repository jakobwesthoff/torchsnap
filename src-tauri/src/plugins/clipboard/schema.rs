// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Clipboard Schema & Types
//
// Database migration SQL, serialized types for frontend
// communication, and plugin-level constants.
// =========================================================

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// =========================================================
// Constants
// =========================================================

pub const PLUGIN_ID: &str = "clipboard-manager";

// =========================================================
// Database Migration
// =========================================================

/// Single migration that creates the clipboard schema including
/// FTS5 for full-text search and triggers to keep the index in
/// sync with the display table.
///
/// Content is stored as raw bytes per format, either inline (as a
/// BLOB) or in FileStorage (via file_key). Format names are our own
/// identifiers: "text", "html", "rtf", "files", "image".
pub const MIGRATION_001: &str = "
CREATE TABLE clipboard_entries (
    id           TEXT PRIMARY KEY,
    captured_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    content_hash TEXT NOT NULL
);

CREATE INDEX idx_entries_captured_at ON clipboard_entries(captured_at DESC);
CREATE UNIQUE INDEX idx_entries_content_hash ON clipboard_entries(content_hash);

-- Raw clipboard content per format (text, html, rtf, files, image).
-- Small content (<=256KB) stored inline as data BLOB. Large content
-- or always-binary formats stored in FileStorage with a file_key ref.
CREATE TABLE clipboard_content (
    entry_id  TEXT NOT NULL REFERENCES clipboard_entries(id) ON DELETE CASCADE,
    format    TEXT NOT NULL,
    data      BLOB,
    file_key  TEXT,
    PRIMARY KEY (entry_id, format)
);

-- Derived human-readable display text, capped at DISPLAY_TEXT_MAX_CHARS.
CREATE TABLE clipboard_display (
    entry_id     TEXT PRIMARY KEY REFERENCES clipboard_entries(id) ON DELETE CASCADE,
    display_text TEXT NOT NULL DEFAULT ''
);

CREATE VIRTUAL TABLE clipboard_fts USING fts5(
    display_text,
    content='clipboard_display',
    content_rowid='rowid'
);

CREATE TRIGGER clipboard_fts_insert AFTER INSERT ON clipboard_display BEGIN
    INSERT INTO clipboard_fts(rowid, display_text)
        VALUES (new.rowid, new.display_text);
END;

CREATE TRIGGER clipboard_fts_delete AFTER DELETE ON clipboard_display BEGIN
    INSERT INTO clipboard_fts(clipboard_fts, rowid, display_text)
        VALUES ('delete', old.rowid, old.display_text);
END;
";

// =========================================================
// Serialized Types (sent to/from the frontend)
// =========================================================

/// Maximum length of the `display_text` stored in the database and
/// indexed by FTS. Capping at this bound keeps the DB and search
/// index at a reasonable size even for very large clipboard entries.
pub const DISPLAY_TEXT_MAX_CHARS: usize = 128_000;

/// Maximum display text length transmitted for list entries.
pub const LIST_DISPLAY_MAX_CHARS: usize = 40;

/// Content larger than this goes to FileStorage instead of inline BLOB.
pub const INLINE_STORAGE_MAX_BYTES: usize = 256 * 1024;

/// Lightweight entry used for list display. Sent via subscribe/notify
/// to keep payload size minimal — detail data is loaded on demand via
/// `load_full_entry`.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardListEntry {
    pub id: String,
    pub captured_at: String,
    /// Display text truncated to [`LIST_DISPLAY_MAX_CHARS`] characters.
    pub display_text: String,
    /// Highest-priority format for this entry, used to determine
    /// the list icon (e.g. image vs text vs files).
    pub primary_format: String,
}

/// How a format's content is delivered to the frontend.
///
/// The variant determines how the frontend accesses the data:
/// - `String`: inline text content, ready to display
/// - `Json`: inline structured data (e.g. file paths as a parsed array)
/// - `Asset`: binary or large content in FileStorage, accessed via asset URL
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type")]
pub enum FormatData {
    #[serde(rename = "string")]
    String { data: String },
    #[serde(rename = "json")]
    Json { document: serde_json::Value },
    #[serde(rename = "asset")]
    Asset { path: String },
}

/// Full entry returned by `load_full_entry` when the user selects an
/// entry in the list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardHistoryEntry {
    pub id: String,
    pub captured_at: String,
    /// Full display text (up to [`DISPLAY_TEXT_MAX_CHARS`]).
    pub display_text: String,
    /// Entry type derived from format priority
    /// (e.g. "text", "files", "image").
    pub primary_format: String,
    /// All stored formats keyed by name, each with its content
    /// delivered as string, json, or asset reference.
    pub formats: HashMap<String, FormatData>,
}

/// Statistics about the clipboard history. Returned by the
/// `stats` message handler for the settings UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardStats {
    /// Total number of entries in the history.
    pub total_entries: u64,
    /// Number of entries per primary format (e.g., "text": 42).
    pub entries_by_format: HashMap<String, u64>,
    /// Total storage size in bytes (inline blobs + file storage).
    pub total_size_bytes: u64,
}

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
