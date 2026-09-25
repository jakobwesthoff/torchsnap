// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Clipboard Storage
//
// Shared state holding the SQL database, file storage, and the
// active search query. Provides query, store, delete, retention,
// and update-push methods used by both the gadget message handler
// and the watcher thread.
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
use super::search::build_fts_query;

// =========================================================
// ActiveQuery
// =========================================================

/// Tracks the currently active search: the channel to push
/// updates through and the query to filter results by. When
/// the frontend issues a new search, the previous `ActiveQuery`
/// is replaced (dropping the old channel). When the underlying
/// data changes (new entry, delete, clear), the stored query is
/// re-executed and filtered results are pushed through the
/// channel.
pub struct ActiveQuery {
    pub channel: Channel<serde_json::Value>,
    pub query: Option<String>,
}

// =========================================================
// SharedState
// =========================================================

pub struct SharedState {
    pub sql: SqlStorage,
    pub files: FileStorage,
    pub active_query: Mutex<Option<ActiveQuery>>,
}

impl SharedState {
    /// Query clipboard history as lightweight list entries, optionally
    /// filtering with FTS5. Results are ordered most recently captured
    /// first, with or without a search term. Returns all matching
    /// entries — the frontend handles windowed rendering.
    ///
    /// A search term that contains no alphanumeric characters filters
    /// nothing and yields the full history; see [`build_fts_query`].
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
        //
        // A search term only narrows the result set; it never reorders
        // it. Both branches sort by captured_at so the entry the user
        // wants — almost always the one copied most recently — stays at
        // the top whether or not they have typed anything. Ordering the
        // filtered branch by FTS5 relevance instead would scatter recent
        // entries among older ones that happen to score higher.
        let entries: Vec<(String, String, String, Option<String>)> =
            match search.and_then(build_fts_query) {
                Some(fts_query) => self.sql.query_map(
                    "SELECT e.id, e.captured_at,
                        COALESCE(d.display_text, ''),
                        GROUP_CONCAT(c.format)
                 FROM clipboard_entries e
                 JOIN clipboard_display d ON d.entry_id = e.id
                 JOIN clipboard_fts f ON f.rowid = d.rowid
                 LEFT JOIN clipboard_content c ON c.entry_id = e.id
                 WHERE clipboard_fts MATCH ?1
                 GROUP BY e.id
                 ORDER BY e.captured_at DESC",
                    &[SqlValue::from(fts_query)],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
                )?,
                None => self.sql.query_map(
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
    /// A period below one day counts as one day: a cutoff of "now"
    /// would delete the whole history.
    pub fn delete_expired_entries(&self, retention_days: u32) -> Result<()> {
        let cutoff = format!("-{} days", retention_days.max(1));

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
            .query_map("SELECT COUNT(*) FROM clipboard_entries", &[], |row| {
                row.get(0)
            })?
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

    /// Bump an entry's `captured_at` timestamp to now, moving it to
    /// the top of the chronological history. Used when restoring an
    /// entry from history to the clipboard.
    pub fn touch_entry(&self, id: &str) -> Result<()> {
        self.sql.execute(
            "UPDATE clipboard_entries
             SET captured_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
             WHERE id = ?1",
            &[SqlValue::from(id)],
        )?;
        Ok(())
    }

    /// Re-execute the active query and push filtered results through
    /// the stored channel. If the channel has been closed by the
    /// frontend (send returns an error), the active query is cleared.
    pub fn refresh_active_query(&self) {
        let mut active = self.active_query.lock().expect("active_query not poisoned");
        let Some(aq) = active.as_ref() else { return };

        let history = match self.search_history(aq.query.as_deref()) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("clipboard: refresh query failed: {e:#}");
                return;
            }
        };

        let payload = match serde_json::to_value(&history) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("clipboard: refresh serialize failed: {e:#}");
                return;
            }
        };

        if aq.channel.send(payload).is_err() {
            *active = None;
        }
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

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gadgets::clipboard::schema::MIGRATION_001;

    /// Build a `SharedState` backed by a temp directory. The
    /// TempDir is returned so the caller keeps it alive for the
    /// duration of the test.
    fn test_state() -> (SharedState, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("create temp dir");
        let sql = SqlStorage::open(dir.path().join("clipboard.sqlite3"), &[MIGRATION_001])
            .expect("open test db");
        let files = FileStorage::new(dir.path().join("files"));
        let state = SharedState {
            sql,
            files,
            active_query: Mutex::new(None),
        };
        (state, dir)
    }

    /// Store a text entry and pin its `captured_at` so ordering
    /// assertions do not depend on insertion timing, which has only
    /// millisecond resolution and would otherwise tie.
    fn store_text(state: &SharedState, id: &str, text: &str, captured_at: &str) {
        let formats = vec![CapturedFormat {
            format: "text".to_owned(),
            data: text.as_bytes().to_vec(),
        }];
        state
            .store_entry(id, text, &formats)
            .expect("store test entry");
        state
            .sql
            .execute(
                "UPDATE clipboard_entries SET captured_at = ?2 WHERE id = ?1",
                &[SqlValue::from(id), SqlValue::from(captured_at)],
            )
            .expect("pin captured_at");
    }

    /// Store the six `claude --resume` entries from a real history,
    /// oldest first, so both ordering and matching can be asserted.
    fn store_resume_history(state: &SharedState) {
        let entries = [
            (
                "e1",
                "claude --resume c764a17e-1fc7-4737-b98a-c9a4aa3eddd0",
                "2026-08-17T16:10:07.754Z",
            ),
            (
                "e2",
                "claude --resume 0ba724c1-569c-453b-8a23-83309bfdcb4f",
                "2026-08-19T11:58:52.990Z",
            ),
            (
                "e3",
                "claude --resume ba3edf90-f34c-4097-be05-45a96255bd36",
                "2026-09-01T11:21:37.250Z",
            ),
            (
                "e4",
                "claude --resume 979713a6-1365-4f40-92fd-ba04599f175f",
                "2026-09-03T09:39:32.556Z",
            ),
            (
                "e5",
                "claude --resume 84f2311c-502b-4957-ab88-cee40d5be444",
                "2026-09-03T09:40:35.523Z",
            ),
            (
                "e6",
                "claude --resume 90c805ea-38cb-4a1b-893a-5fc7880de861",
                "2026-09-03T09:41:27.710Z",
            ),
        ];
        for (id, text, captured_at) in entries {
            store_text(state, id, text, captured_at);
        }
    }

    fn ids(entries: &[ClipboardListEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.id.as_str()).collect()
    }

    // -----------------------------------------------------
    // Retention
    // -----------------------------------------------------

    fn store_now(state: &SharedState, id: &str, text: &str) {
        let formats = vec![CapturedFormat {
            format: "text".to_owned(),
            data: text.as_bytes().to_vec(),
        }];
        state
            .store_entry(id, text, &formats)
            .expect("store test entry");
    }

    #[test]
    fn retention_deletes_entries_older_than_the_period_only() {
        let (state, _dir) = test_state();
        store_text(&state, "old", "from long ago", "2020-01-01T00:00:00.000Z");
        store_now(&state, "new", "from just now");

        state.delete_expired_entries(30).expect("cleanup runs");

        let remaining = state.search_history(None).expect("history loads");
        assert_eq!(ids(&remaining), vec!["new"]);
    }

    #[test]
    fn retention_of_zero_days_keeps_todays_entries() {
        let (state, _dir) = test_state();
        store_now(&state, "new", "from just now");

        state.delete_expired_entries(0).expect("cleanup runs");

        let remaining = state.search_history(None).expect("history loads");
        assert_eq!(ids(&remaining), vec!["new"]);
    }

    // -----------------------------------------------------
    // Queries that previously failed with an FTS5 syntax error
    // -----------------------------------------------------

    #[test]
    fn search_with_hyphenated_flag_matches() {
        let (state, _dir) = test_state();
        store_resume_history(&state);

        let results = state
            .search_history(Some("claude --resume"))
            .expect("hyphenated query must not be an FTS5 syntax error");
        assert_eq!(results.len(), 6);
    }

    #[test]
    fn search_while_typing_a_flag_keeps_matching() {
        let (state, _dir) = test_state();
        store_resume_history(&state);

        // Each of these is a keystroke on the way to "claude --resume".
        // None of them may error or drop to zero results.
        for prefix in [
            "c",
            "claude",
            "claude ",
            "claude -",
            "claude --",
            "claude --res",
        ] {
            let results = state
                .search_history(Some(prefix))
                .unwrap_or_else(|e| panic!("query {prefix:?} failed: {e}"));
            assert_eq!(results.len(), 6, "query {prefix:?} lost results");
        }
    }

    #[test]
    fn search_by_uuid_fragment_matches() {
        let (state, _dir) = test_state();
        store_resume_history(&state);

        let results = state
            .search_history(Some("979713a6-1365"))
            .expect("uuid fragment must not be an FTS5 syntax error");
        assert_eq!(ids(&results), vec!["e4"]);
    }

    #[test]
    fn search_with_url_punctuation_matches() {
        let (state, _dir) = test_state();
        store_text(
            &state,
            "u1",
            "https://example.com/foo/bar",
            "2026-09-01T10:00:00.000Z",
        );

        for query in ["https://example.com", "example.com", "example.com/foo"] {
            let results = state
                .search_history(Some(query))
                .unwrap_or_else(|e| panic!("query {query:?} failed: {e}"));
            assert_eq!(ids(&results), vec!["u1"], "query {query:?}");
        }
    }

    #[test]
    fn search_with_code_punctuation_matches() {
        let (state, _dir) = test_state();
        store_text(
            &state,
            "c1",
            "fn main() { let x = 1; }",
            "2026-09-01T10:00:00.000Z",
        );

        for query in ["fn main()", "main() {", "x = 1"] {
            let results = state
                .search_history(Some(query))
                .unwrap_or_else(|e| panic!("query {query:?} failed: {e}"));
            assert_eq!(ids(&results), vec!["c1"], "query {query:?}");
        }
    }

    #[test]
    fn search_with_unbalanced_quote_matches() {
        let (state, _dir) = test_state();
        store_text(&state, "q1", "quoted text", "2026-09-01T10:00:00.000Z");

        let results = state
            .search_history(Some("\"quoted"))
            .expect("unbalanced quote must not be an FTS5 syntax error");
        assert_eq!(ids(&results), vec!["q1"]);
    }

    #[test]
    fn search_for_an_fts_keyword_matches_the_literal_word() {
        let (state, _dir) = test_state();
        store_text(&state, "k1", "this AND that", "2026-09-01T10:00:00.000Z");
        store_text(&state, "k2", "unrelated entry", "2026-09-01T11:00:00.000Z");

        let results = state
            .search_history(Some("AND"))
            .expect("keyword must not be an FTS5 syntax error");
        assert_eq!(ids(&results), vec!["k1"]);
    }

    // -----------------------------------------------------
    // Result ordering: most recent first, never relevance
    // -----------------------------------------------------

    #[test]
    fn search_results_are_ordered_most_recent_first() {
        let (state, _dir) = test_state();
        store_resume_history(&state);

        let results = state.search_history(Some("resume")).expect("search");
        assert_eq!(ids(&results), vec!["e6", "e5", "e4", "e3", "e2", "e1"]);
    }

    #[test]
    fn unfiltered_results_are_ordered_most_recent_first() {
        let (state, _dir) = test_state();
        store_resume_history(&state);

        let results = state.search_history(None).expect("search");
        assert_eq!(ids(&results), vec!["e6", "e5", "e4", "e3", "e2", "e1"]);
    }

    #[test]
    fn recency_beats_relevance() {
        let (state, _dir) = test_state();
        // The short entry is the stronger bm25 match (higher term
        // density, shorter document) but the older one, so ordering
        // by relevance would put it first.
        store_text(&state, "short", "resume", "2026-08-01T10:00:00.000Z");
        store_text(
            &state,
            "long",
            "a much longer entry that also mentions resume somewhere inside it",
            "2026-09-01T10:00:00.000Z",
        );

        let results = state.search_history(Some("resume")).expect("search");
        assert_eq!(ids(&results), vec!["long", "short"]);
    }

    #[test]
    fn bumping_a_duplicate_moves_it_to_the_top_of_search_results() {
        let (state, _dir) = test_state();
        store_text(&state, "old", "resume one", "2026-08-01T10:00:00.000Z");
        store_text(&state, "new", "resume two", "2026-09-01T10:00:00.000Z");

        // Re-copying "resume one" bumps the existing entry rather than
        // inserting a new one, which must reorder search results too.
        let formats = vec![CapturedFormat {
            format: "text".to_owned(),
            data: b"resume one".to_vec(),
        }];
        let created = state
            .store_entry("ignored", "resume one", &formats)
            .expect("bump existing entry");
        assert!(!created, "identical content must bump, not insert");

        let results = state.search_history(Some("resume")).expect("search");
        assert_eq!(ids(&results), vec!["old", "new"]);
    }

    // -----------------------------------------------------
    // Empty and separator-only queries
    // -----------------------------------------------------

    #[test]
    fn empty_query_returns_the_full_history() {
        let (state, _dir) = test_state();
        store_resume_history(&state);

        assert_eq!(state.search_history(Some("")).expect("search").len(), 6);
    }

    #[test]
    fn separator_only_query_returns_the_full_history() {
        let (state, _dir) = test_state();
        store_resume_history(&state);

        for query in ["-", "---", "://", "   "] {
            let results = state
                .search_history(Some(query))
                .unwrap_or_else(|e| panic!("query {query:?} failed: {e}"));
            assert_eq!(results.len(), 6, "query {query:?}");
        }
    }

    // -----------------------------------------------------
    // Matching semantics
    // -----------------------------------------------------

    #[test]
    fn search_is_case_insensitive() {
        let (state, _dir) = test_state();
        store_text(&state, "m1", "ClipboardManager", "2026-09-01T10:00:00.000Z");

        for query in ["clipboardmanager", "CLIPBOARDMANAGER", "ClipboardManager"] {
            let results = state
                .search_history(Some(query))
                .unwrap_or_else(|e| panic!("query {query:?} failed: {e}"));
            assert_eq!(ids(&results), vec!["m1"], "query {query:?}");
        }
    }

    #[test]
    fn search_matches_token_prefixes_but_not_infixes() {
        let (state, _dir) = test_state();
        store_text(&state, "m1", "ClipboardManager", "2026-09-01T10:00:00.000Z");

        assert_eq!(
            ids(&state.search_history(Some("clip")).expect("search")),
            vec!["m1"]
        );
        // Documents the standing limitation: FTS5 indexes whole tokens,
        // so a fragment from the middle of a word does not match.
        assert!(
            state
                .search_history(Some("board"))
                .expect("search")
                .is_empty()
        );
    }

    #[test]
    fn multiple_terms_are_anded() {
        let (state, _dir) = test_state();
        store_text(&state, "a1", "alpha beta", "2026-09-01T10:00:00.000Z");
        store_text(&state, "a2", "alpha gamma", "2026-09-01T11:00:00.000Z");

        assert_eq!(
            ids(&state.search_history(Some("alpha beta")).expect("search")),
            vec!["a1"]
        );
        assert!(
            state
                .search_history(Some("alpha delta"))
                .expect("search")
                .is_empty()
        );
    }

    #[test]
    fn search_matches_diacritics_case_and_accent_insensitively() {
        let (state, _dir) = test_state();
        store_text(
            &state,
            "d1",
            "Grüße aus München",
            "2026-09-01T10:00:00.000Z",
        );

        for query in ["München", "munchen", "MÜNCHEN"] {
            let results = state
                .search_history(Some(query))
                .unwrap_or_else(|e| panic!("query {query:?} failed: {e}"));
            assert_eq!(ids(&results), vec!["d1"], "query {query:?}");
        }
    }

    #[test]
    fn search_with_no_match_returns_empty() {
        let (state, _dir) = test_state();
        store_resume_history(&state);

        assert!(
            state
                .search_history(Some("nonexistentterm"))
                .expect("search")
                .is_empty()
        );
    }
}
