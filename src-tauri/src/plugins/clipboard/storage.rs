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
//
// All clipboard content is stored format-agnostically as raw
// bytes. Small content (<=INLINE_STORAGE_MAX_BYTES) is stored
// inline as a BLOB in the clipboard_content table. Large or
// always-binary formats are stored in FileStorage with a
// file_key reference in the table.
// =========================================================

use std::sync::Mutex;

use anyhow::{Context, Result};
use tauri::ipc::Channel;

use crate::storage::{FileStorage, SqlStorage, SqlValue, StorageKey};

use super::formats::CapturedFormat;
use super::schema::{
    ClipboardHistoryEntry, ClipboardListEntry, ClipboardStats, FormatData,
    INLINE_STORAGE_MAX_BYTES, LIST_DISPLAY_MAX_CHARS,
};

// =========================================================
// SharedState
// =========================================================

pub struct SharedState {
    pub sql: SqlStorage,
    pub files: FileStorage,
    pub subscribers: Mutex<Vec<Channel<serde_json::Value>>>,
}

impl SharedState {
    /// Query clipboard history as lightweight list entries, optionally
    /// filtering with FTS5. Returns all matching entries — the frontend
    /// handles windowed rendering.
    pub fn search_history(&self, search: Option<&str>) -> Result<Vec<ClipboardListEntry>> {
        // Each entry needs its primary_format derived from clipboard_content.
        // We use GROUP_CONCAT in SQL to collect format names per entry, then
        // pick the highest-priority format in Rust. This keeps the SQL simple
        // and avoids duplicating priority logic in CASE expressions.
        //
        // Display text comes from clipboard_display (1:1 PK join) and is
        // truncated to LIST_DISPLAY_MAX_CHARS for the list view.
        //
        // Performance note: if this join becomes a bottleneck with very
        // large histories, we could denormalize primary_format into a
        // column on clipboard_entries and set it at capture time.
        let entries: Vec<(String, String, String, Option<String>)> = match search {
            Some(term) if !term.is_empty() => {
                let fts_query = format!("{term}*");
                self.sql.query_map(
                    "SELECT e.id, e.captured_at,
                            COALESCE(d.display_text, ''),
                            GROUP_CONCAT(c.format)
                     FROM clipboard_entries e
                     JOIN clipboard_display d ON d.entry_id = e.id
                     JOIN clipboard_fts f ON f.rowid = d.rowid
                     LEFT JOIN clipboard_content c ON c.entry_id = e.id
                     WHERE clipboard_fts MATCH ?1
                     GROUP BY e.id
                     ORDER BY rank",
                    &[SqlValue::from(fts_query)],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )?
            }
            _ => self.sql.query_map(
                "SELECT e.id, e.captured_at,
                        COALESCE(d.display_text, ''),
                        GROUP_CONCAT(c.format)
                 FROM clipboard_entries e
                 LEFT JOIN clipboard_display d ON d.entry_id = e.id
                 LEFT JOIN clipboard_content c ON c.entry_id = e.id
                 GROUP BY e.id
                 ORDER BY e.captured_at DESC",
                &[],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?,
        };

        let result = entries
            .into_iter()
            .map(|(id, captured_at, display_text, formats_csv)| {
                let primary_format = primary_format_from_csv(formats_csv.as_deref());
                let truncated = truncate_chars(&display_text, LIST_DISPLAY_MAX_CHARS);
                ClipboardListEntry {
                    id,
                    captured_at,
                    display_text: truncated,
                    primary_format: primary_format.to_owned(),
                }
            })
            .collect();

        Ok(result)
    }

    /// Load the full detail for a single clipboard entry, including
    /// all format data and the derived primary format.
    pub fn load_full_entry(&self, id: &str) -> Result<Option<ClipboardHistoryEntry>> {
        let entry: Option<(String, String, String)> = self
            .sql
            .query_map(
                "SELECT e.id, e.captured_at,
                        COALESCE(d.display_text, '')
                 FROM clipboard_entries e
                 LEFT JOIN clipboard_display d ON d.entry_id = e.id
                 WHERE e.id = ?1",
                &[SqlValue::from(id)],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?
            .into_iter()
            .next();

        let Some((id, captured_at, display_text)) = entry else {
            return Ok(None);
        };

        // Load all content rows: format name, inline data, file_key.
        let content_rows: Vec<(String, Option<Vec<u8>>, Option<String>)> = self
            .sql
            .query_map(
                "SELECT format, data, file_key FROM clipboard_content
                 WHERE entry_id = ?1",
                &[SqlValue::from(id.as_str())],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap_or_default();

        // Build the format name list for primary_format derivation.
        let format_names_csv: String = content_rows
            .iter()
            .map(|(f, _, _)| f.as_str())
            .collect::<Vec<_>>()
            .join(",");

        let primary_format = primary_format_from_csv(Some(&format_names_csv)).to_owned();

        // Build the formats map with typed FormatData for each entry.
        let mut formats = std::collections::HashMap::new();
        for (format, inline_data, file_key) in &content_rows {
            let format_data = if let Some(raw_key) = file_key {
                // Content stored in FileStorage → asset reference.
                let key = StorageKey::from_raw(raw_key.clone());
                let ext = file_ext_for_format(format);
                let path = self.files.resolve(&key, ext);
                FormatData::Asset {
                    path: path.to_string_lossy().into_owned(),
                }
            } else if let Some(blob) = inline_data {
                // Inline content — choose string vs json based on format.
                inline_blob_to_format_data(format, blob)
            } else {
                continue;
            };

            formats.insert(format.clone(), format_data);
        }

        Ok(Some(ClipboardHistoryEntry {
            id,
            captured_at,
            display_text,
            primary_format,
            formats,
        }))
    }

    /// Load all raw format data for a single entry, ready for
    /// paste-back via `ClipboardContent::Other`.
    pub fn load_captured_formats(&self, id: &str) -> Result<Vec<CapturedFormat>> {
        let rows: Vec<(String, Option<Vec<u8>>, Option<String>)> = self
            .sql
            .query_map(
                "SELECT format, data, file_key FROM clipboard_content
                 WHERE entry_id = ?1",
                &[SqlValue::from(id)],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .context("query clipboard content for paste")?;

        let mut formats = Vec::with_capacity(rows.len());

        for (format, inline_data, file_key) in rows {
            let data = if let Some(blob) = inline_data {
                blob
            } else if let Some(ref raw_key) = file_key {
                let key = StorageKey::from_raw(raw_key.clone());
                match self.files.load(&key, file_ext_for_format(&format)) {
                    Ok(Some(bytes)) => bytes,
                    Ok(None) => {
                        eprintln!(
                            "clipboard: file missing for entry {id}, \
                             format {format}, key {raw_key}"
                        );
                        continue;
                    }
                    Err(e) => {
                        eprintln!(
                            "clipboard: load file for entry {id}, \
                             format {format}: {e:#}"
                        );
                        continue;
                    }
                }
            } else {
                // Row has neither inline data nor file_key — skip.
                continue;
            };

            formats.push(CapturedFormat { format, data });
        }

        Ok(formats)
    }

    /// Store a new clipboard entry with its captured formats, or
    /// deduplicate if an identical entry already exists.
    ///
    /// Computes a blake3 hash over all format names and data (in
    /// sorted order for determinism). If an existing entry has the
    /// same hash, its `captured_at` timestamp is updated to now
    /// (moving it to the top of the history) and no new data is
    /// stored. Returns `true` if a new entry was created, `false`
    /// if an existing entry was bumped.
    pub fn store_entry(
        &self,
        id: &str,
        display_text: &str,
        formats: &[CapturedFormat],
    ) -> Result<bool> {
        let content_hash = compute_content_hash(formats);

        // Check for an existing entry with the same content.
        let existing: Option<String> = self
            .sql
            .query_map(
                "SELECT id FROM clipboard_entries WHERE content_hash = ?1",
                &[SqlValue::from(content_hash.as_str())],
                |row| row.get(0),
            )?
            .into_iter()
            .next();

        if let Some(existing_id) = existing {
            // Bump the existing entry's timestamp to now.
            self.sql.execute(
                "UPDATE clipboard_entries
                 SET captured_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                 WHERE id = ?1",
                &[SqlValue::from(existing_id.as_str())],
            )?;
            return Ok(false);
        }

        // No duplicate — insert a new entry.
        self.sql.execute(
            "INSERT INTO clipboard_entries (id, content_hash) VALUES (?1, ?2)",
            &[SqlValue::from(id), SqlValue::from(content_hash.as_str())],
        )?;

        self.sql.execute(
            "INSERT INTO clipboard_display (entry_id, display_text) VALUES (?1, ?2)",
            &[SqlValue::from(id), SqlValue::from(display_text)],
        )?;

        for fmt in formats {
            let use_file_storage =
                fmt.format == "image" || fmt.data.len() > INLINE_STORAGE_MAX_BYTES;

            if use_file_storage {
                // Store in FileStorage. The key is derived from
                // "{id}-{format}" so each format gets its own file.
                let ext = file_ext_for_format(&fmt.format);
                let key = StorageKey::new(&format!("{id}-{}", fmt.format));
                self.files
                    .store(&key, &fmt.data, ext)
                    .context("store file content")?;

                self.sql.execute(
                    "INSERT INTO clipboard_content (entry_id, format, data, file_key)
                     VALUES (?1, ?2, NULL, ?3)",
                    &[
                        SqlValue::from(id),
                        SqlValue::from(fmt.format.as_str()),
                        SqlValue::from(key.to_string()),
                    ],
                )?;
            } else {
                self.sql.execute(
                    "INSERT INTO clipboard_content (entry_id, format, data, file_key)
                     VALUES (?1, ?2, ?3, NULL)",
                    &[
                        SqlValue::from(id),
                        SqlValue::from(fmt.format.as_str()),
                        SqlValue::from(fmt.data.clone()),
                    ],
                )?;
            }
        }

        Ok(true)
    }

    /// Delete an entry and its associated FileStorage files.
    pub fn delete_entry(&self, id: &str) -> Result<()> {
        // Collect file_keys before CASCADE deletes the rows.
        self.delete_file_storage_for_entry(id);

        // CASCADE deletes clipboard_content rows and FTS triggers
        // handle the index cleanup.
        self.sql.execute(
            "DELETE FROM clipboard_entries WHERE id = ?1",
            &[SqlValue::from(id)],
        )?;

        Ok(())
    }

    /// Delete entries older than the given retention period (in days).
    pub fn delete_expired_entries(&self, retention_days: u32) -> Result<()> {
        let cutoff = format!("-{retention_days} days");

        let old_ids: Vec<String> = self.sql.query_map(
            "SELECT id FROM clipboard_entries
             WHERE captured_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?1)",
            &[SqlValue::from(cutoff.as_str())],
            |row| row.get(0),
        )?;

        for id in &old_ids {
            self.delete_file_storage_for_entry(id);
        }

        self.sql.execute(
            "DELETE FROM clipboard_entries
             WHERE captured_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?1)",
            &[SqlValue::from(cutoff)],
        )?;

        Ok(())
    }

    /// Compute statistics about the clipboard history.
    ///
    /// Returns total entry count, per-format counts, and total
    /// storage size (inline blobs + file storage on disk).
    pub fn stats(&self) -> Result<ClipboardStats> {
        // Total entries.
        let total_entries: i64 = self
            .sql
            .query_map(
                "SELECT COUNT(*) FROM clipboard_entries",
                &[],
                |row| row.get(0),
            )?
            .into_iter()
            .next()
            .unwrap_or(0);

        // Count per primary format. We use the same format-priority
        // logic as the list view: GROUP_CONCAT formats per entry,
        // then pick the primary one in Rust.
        let rows: Vec<Option<String>> = self.sql.query_map(
            "SELECT GROUP_CONCAT(c.format)
             FROM clipboard_entries e
             LEFT JOIN clipboard_content c ON c.entry_id = e.id
             GROUP BY e.id",
            &[],
            |row| row.get(0),
        )?;

        let mut entries_by_format = std::collections::HashMap::new();
        for formats_csv in &rows {
            let fmt = primary_format_from_csv(formats_csv.as_deref());
            *entries_by_format.entry(fmt.to_owned()).or_insert(0u64) += 1;
        }

        // Total inline blob size.
        let inline_size: i64 = self
            .sql
            .query_map(
                "SELECT COALESCE(SUM(LENGTH(data)), 0) FROM clipboard_content WHERE data IS NOT NULL",
                &[],
                |row| row.get(0),
            )?
            .into_iter()
            .next()
            .unwrap_or(0);

        // Total file storage size on disk.
        let file_size: u64 = self.files.entries().map(|(_, _, meta)| meta.size).sum();

        Ok(ClipboardStats {
            total_entries: total_entries as u64,
            entries_by_format,
            total_size_bytes: inline_size as u64 + file_size,
        })
    }

    /// Delete all clipboard history entries and their associated
    /// files on disk. Used by the "clear history" button in settings.
    pub fn clear_all(&self) -> Result<()> {
        // Collect all file keys before CASCADE deletes the rows.
        let file_rows: Vec<(String, String)> = self
            .sql
            .query_map(
                "SELECT format, file_key FROM clipboard_content
                 WHERE file_key IS NOT NULL",
                &[],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap_or_default();

        for (format, raw_key) in &file_rows {
            let key = StorageKey::from_raw(raw_key.clone());
            let _ = self.files.delete(&key, file_ext_for_format(format));
        }

        self.sql.execute("DELETE FROM clipboard_entries", &[])?;

        Ok(())
    }

    /// Push the current history to all connected subscriber channels.
    /// Drops channels that have been closed by the frontend.
    pub fn notify_subscribers(&self) {
        let history = match self.search_history(None) {
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

    // -------------------------------------------------------
    // Internal helpers
    // -------------------------------------------------------

    /// Delete all FileStorage files associated with an entry.
    /// Must be called before the SQL rows are deleted (CASCADE
    /// would remove the file_key references we need to find the
    /// files).
    fn delete_file_storage_for_entry(&self, id: &str) {
        let rows: Vec<(String, String)> = self
            .sql
            .query_map(
                "SELECT format, file_key FROM clipboard_content
                 WHERE entry_id = ?1 AND file_key IS NOT NULL",
                &[SqlValue::from(id)],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap_or_default();

        for (format, raw_key) in &rows {
            let key = StorageKey::from_raw(raw_key.clone());
            let _ = self.files.delete(&key, file_ext_for_format(format));
        }
    }
}

// =========================================================
// Helpers
// =========================================================

/// Compute a deterministic blake3 hash over all captured format data.
///
/// Formats are sorted by name before hashing so the result is
/// independent of capture order. Each format contributes its name
/// (as UTF-8) followed by its raw data bytes, with a length prefix
/// to prevent ambiguous concatenation.
fn compute_content_hash(formats: &[CapturedFormat]) -> String {
    let mut sorted: Vec<&CapturedFormat> = formats.iter().collect();
    sorted.sort_by(|a, b| a.format.cmp(&b.format));

    let mut hasher = blake3::Hasher::new();
    for fmt in &sorted {
        let name_bytes = fmt.format.as_bytes();
        hasher.update(&(name_bytes.len() as u32).to_le_bytes());
        hasher.update(name_bytes);
        hasher.update(&(fmt.data.len() as u64).to_le_bytes());
        hasher.update(&fmt.data);
    }

    hasher.finalize().to_hex().to_string()
}

/// Convert inline BLOB data to the appropriate `FormatData` variant.
///
/// "files" is stored as JSON-encoded paths → `Json` with parsed value.
/// All other inline formats are UTF-8 text → `String`.
fn inline_blob_to_format_data(format: &str, blob: &[u8]) -> FormatData {
    if format == "files"
        && let Ok(text) = std::str::from_utf8(blob)
        && let Ok(value) = serde_json::from_str::<serde_json::Value>(text)
    {
        return FormatData::Json { document: value };
    }

    // All other inline formats are UTF-8 text.
    let data = String::from_utf8_lossy(blob).into_owned();
    FormatData::String { data }
}

/// File extension used for FileStorage. Images are stored as
/// PNG so Tauri's asset protocol can serve them with the correct
/// content type. Everything else uses a generic extension.
fn file_ext_for_format(format: &str) -> &'static str {
    match format {
        "image" => "png",
        _ => "bin",
    }
}

/// Pick the highest-priority format from a comma-separated list
/// returned by SQLite's `GROUP_CONCAT(format)`.
/// Priority: image > files > text. Returns "text" as default.
fn primary_format_from_csv(csv: Option<&str>) -> &'static str {
    const PRIORITY: &[&str] = &["files", "image", "html", "rtf", "text"];

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
fn truncate_chars(s: &str, max_chars: usize) -> String {
    let mut chars = s.chars();
    let truncated: String = chars.by_ref().take(max_chars).collect();
    if chars.next().is_some() {
        format!("{truncated}…")
    } else {
        truncated
    }
}
