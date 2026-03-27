// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Clipboard Storage
//
// Shared state holding the SQL database, file storage, and
// subscriber channels. Provides query, store, delete,
// retention, and notification methods used by both the plugin
// message handler and the watcher thread.
// =========================================================

use std::sync::atomic::AtomicU32;
use std::sync::Mutex;

use anyhow::Result;
use tauri::ipc::Channel;

use crate::storage::{FileStorage, SqlStorage, SqlValue, StorageKey};

use super::formats::{CapturedContent, CapturedFormat};
use super::schema::{
    ClipboardHistoryEntry, ClipboardListEntry, DETAIL_PREVIEW_MAX_CHARS, LIST_PREVIEW_MAX_CHARS,
    RETENTION_DAYS,
};

// =========================================================
// SharedState
// =========================================================

pub struct SharedState {
    pub sql: SqlStorage,
    pub files: FileStorage,
    pub subscribers: Mutex<Vec<Channel<serde_json::Value>>>,
    pub capture_count: AtomicU32,
}

impl SharedState {
    /// Query clipboard history as lightweight list entries, optionally
    /// filtering with FTS5. Returns all matching entries — the frontend
    /// handles windowed rendering.
    pub fn query_history(
        &self,
        search: Option<&str>,
    ) -> Result<Vec<ClipboardListEntry>> {
        // Each entry needs its primary_format derived from clipboard_content.
        // We use GROUP_CONCAT in SQL to collect format names per entry, then
        // pick the highest-priority format in Rust. This keeps the SQL simple
        // and avoids duplicating priority logic in CASE expressions.
        //
        // Performance note: if this join becomes a bottleneck with very
        // large histories, we could denormalize primary_format into a
        // column on clipboard_entries and set it at capture time.
        let entries: Vec<(String, String, String, Option<String>)> = match search {
            Some(term) if !term.is_empty() => {
                let fts_query = format!("{term}*");
                self.sql.query_map(
                    "SELECT e.id, e.captured_at, e.preview,
                            GROUP_CONCAT(c.format)
                     FROM clipboard_entries e
                     JOIN clipboard_fts f ON f.rowid = e.rowid
                     LEFT JOIN clipboard_content c ON c.entry_id = e.id
                     WHERE clipboard_fts MATCH ?1
                     GROUP BY e.id
                     ORDER BY rank",
                    &[SqlValue::from(fts_query)],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )?
            }
            _ => self.sql.query_map(
                "SELECT e.id, e.captured_at, e.preview,
                        GROUP_CONCAT(c.format)
                 FROM clipboard_entries e
                 LEFT JOIN clipboard_content c ON c.entry_id = e.id
                 GROUP BY e.id
                 ORDER BY e.captured_at DESC",
                &[],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?,
        };

        let result = entries
            .into_iter()
            .map(|(id, captured_at, preview, formats_csv)| {
                let primary_format = primary_format_from_csv(formats_csv.as_deref());
                let truncated = truncate_preview(&preview, LIST_PREVIEW_MAX_CHARS);
                ClipboardListEntry {
                    id,
                    captured_at,
                    preview: truncated,
                    primary_format: primary_format.to_owned(),
                }
            })
            .collect();

        Ok(result)
    }

    /// Load the full detail for a single clipboard entry, including all
    /// format names, the resolved image path, and a longer preview.
    pub fn load_full_entry(&self, id: &str) -> Result<Option<ClipboardHistoryEntry>> {
        let entry: Option<(String, String, String)> = self
            .sql
            .query_map(
                "SELECT id, captured_at, preview FROM clipboard_entries WHERE id = ?1",
                &[SqlValue::from(id)],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?
            .into_iter()
            .next();

        let Some((id, captured_at, preview)) = entry else {
            return Ok(None);
        };

        let formats: Vec<String> = self
            .sql
            .query_map(
                "SELECT format FROM clipboard_content WHERE entry_id = ?1",
                &[SqlValue::from(id.as_str())],
                |row| row.get(0),
            )
            .unwrap_or_default();

        let image_path = if formats.iter().any(|f| f == "image") {
            let key = StorageKey::new(&id);
            let path = self.files.resolve(&key, "png");
            if path.exists() {
                Some(path.to_string_lossy().into_owned())
            } else {
                None
            }
        } else {
            None
        };

        let truncated = truncate_preview(&preview, DETAIL_PREVIEW_MAX_CHARS);

        Ok(Some(ClipboardHistoryEntry {
            id,
            captured_at,
            preview: truncated,
            formats,
            image_path,
        }))
    }

    /// Store a new clipboard entry with its captured formats.
    pub fn store_entry(
        &self,
        id: &str,
        preview: &str,
        formats: &[CapturedFormat],
    ) -> Result<()> {
        self.sql.execute(
            "INSERT INTO clipboard_entries (id, preview) VALUES (?1, ?2)",
            &[SqlValue::from(id), SqlValue::from(preview)],
        )?;

        for fmt in formats {
            let (text_value, file_key) = match &fmt.content {
                CapturedContent::Text(s) => (Some(s.as_str()), None),
                CapturedContent::Binary { data, ext } => {
                    let key = StorageKey::new(id);
                    self.files
                        .store(&key, data, ext)
                        .map_err(|e| anyhow::anyhow!("store binary content: {e:#}"))?;
                    (None, Some(key.to_string()))
                }
            };

            self.sql.execute(
                "INSERT INTO clipboard_content (entry_id, format, text_value, file_key)
                 VALUES (?1, ?2, ?3, ?4)",
                &[
                    SqlValue::from(id),
                    SqlValue::from(fmt.name.as_str()),
                    SqlValue::from(text_value),
                    SqlValue::from(file_key),
                ],
            )?;
        }

        Ok(())
    }

    /// Delete an entry and its associated files.
    pub fn delete_entry(&self, id: &str) -> Result<()> {
        let key = StorageKey::new(id);
        let _ = self.files.delete(&key, "png");

        // CASCADE deletes clipboard_content rows and FTS triggers
        // handle the index cleanup.
        self.sql.execute(
            "DELETE FROM clipboard_entries WHERE id = ?1",
            &[SqlValue::from(id)],
        )?;

        Ok(())
    }

    /// Delete entries older than the retention period.
    pub fn run_retention(&self) -> Result<()> {
        let cutoff = format!("-{RETENTION_DAYS} days");

        let old_ids: Vec<String> = self.sql.query_map(
            "SELECT id FROM clipboard_entries
             WHERE captured_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?1)",
            &[SqlValue::from(cutoff.as_str())],
            |row| row.get(0),
        )?;

        for id in &old_ids {
            let key = StorageKey::new(id);
            let _ = self.files.delete(&key, "png");
        }

        self.sql.execute(
            "DELETE FROM clipboard_entries
             WHERE captured_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?1)",
            &[SqlValue::from(cutoff)],
        )?;

        Ok(())
    }

    /// Push the current history to all connected subscriber channels.
    /// Drops channels that have been closed by the frontend.
    pub fn notify_subscribers(&self) {
        let history = match self.query_history(None) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("clipboard: notify query failed: {e:#}");
                return;
            }
        };

        let payload = match serde_json::to_value(&history) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("clipboard: notify serialize failed: {e:#}");
                return;
            }
        };

        let mut subs = self.subscribers.lock().expect("subscribers not poisoned");
        subs.retain(|ch| ch.send(payload.clone()).is_ok());
    }
}

// =========================================================
// Helpers
// =========================================================

/// Pick the highest-priority format from a comma-separated list returned
/// by SQLite's `GROUP_CONCAT(format)`. Priority: image > files > text.
/// Returns `"text"` as the default when no formats are present.
fn primary_format_from_csv(csv: Option<&str>) -> &'static str {
    // Format priority from highest to lowest. The first match wins.
    const PRIORITY: &[&str] = &["image", "files", "html", "rtf", "text"];

    let Some(csv) = csv else { return "text" };
    for &fmt in PRIORITY {
        if csv.split(',').any(|f| f == fmt) {
            return fmt;
        }
    }
    "text"
}

/// Truncate a string to at most `max_chars` Unicode characters, appending
/// an ellipsis when the string is longer.
fn truncate_preview(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}
