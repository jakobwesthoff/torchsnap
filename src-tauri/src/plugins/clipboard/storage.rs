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

use crate::storage::{FileStorage, SqlStorage, StorageKey};

use super::formats::{CapturedContent, CapturedFormat};
use super::schema::{ClipboardHistoryEntry, RETENTION_DAYS};

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
    /// Query clipboard history, optionally filtering with FTS5.
    pub fn query_history(
        &self,
        search: Option<&str>,
        limit: usize,
    ) -> Result<Vec<ClipboardHistoryEntry>> {
        let entries: Vec<(String, String, String)> = match search {
            Some(term) if !term.is_empty() => {
                let fts_query = format!("{term}*");
                self.sql.query_map(
                    "SELECT e.id, e.captured_at, e.preview
                     FROM clipboard_entries e
                     JOIN clipboard_fts f ON f.rowid = e.rowid
                     WHERE clipboard_fts MATCH ?1
                     ORDER BY rank
                     LIMIT ?2",
                    &[&fts_query as &dyn rusqlite::types::ToSql, &(limit as i64)],
                    |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                )?
            }
            _ => self.sql.query_map(
                "SELECT id, captured_at, preview
                 FROM clipboard_entries
                 ORDER BY captured_at DESC
                 LIMIT ?1",
                &[&(limit as i64) as &dyn rusqlite::types::ToSql],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?,
        };

        let mut result = Vec::with_capacity(entries.len());
        for (id, captured_at, preview) in entries {
            let formats: Vec<String> = self
                .sql
                .query_map(
                    "SELECT format FROM clipboard_content WHERE entry_id = ?1",
                    &[&id as &dyn rusqlite::types::ToSql],
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

            result.push(ClipboardHistoryEntry {
                id,
                captured_at,
                preview,
                formats,
                image_path,
            });
        }

        Ok(result)
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
            &[&id as &dyn rusqlite::types::ToSql, &preview],
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
                    &id as &dyn rusqlite::types::ToSql,
                    &fmt.name.as_str(),
                    &text_value,
                    &file_key.as_deref(),
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
            &[&id as &dyn rusqlite::types::ToSql],
        )?;

        Ok(())
    }

    /// Delete entries older than the retention period.
    pub fn run_retention(&self) -> Result<()> {
        let cutoff = format!("-{RETENTION_DAYS} days");

        let old_ids: Vec<String> = self.sql.query_map(
            "SELECT id FROM clipboard_entries
             WHERE captured_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?1)",
            &[&cutoff as &dyn rusqlite::types::ToSql],
            |row| row.get(0),
        )?;

        for id in &old_ids {
            let key = StorageKey::new(id);
            let _ = self.files.delete(&key, "png");
        }

        self.sql.execute(
            "DELETE FROM clipboard_entries
             WHERE captured_at < strftime('%Y-%m-%dT%H:%M:%fZ', 'now', ?1)",
            &[&cutoff as &dyn rusqlite::types::ToSql],
        )?;

        Ok(())
    }

    /// Push the current history to all connected subscriber channels.
    /// Drops channels that have been closed by the frontend.
    pub fn notify_subscribers(&self) {
        let history = match self.query_history(None, 50) {
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
