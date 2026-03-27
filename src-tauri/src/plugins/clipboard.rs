// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Clipboard Manager Plugin
//
// Catalog plugin that monitors the system clipboard, stores
// history (text, HTML, RTF, images), and lets users browse
// and paste previous entries.
//
// Architecture:
//   - CatalogPlugin: provides a single "Clipboard History"
//     entry. Executing it returns ShowCustomUI.
//   - Watcher thread: polls clipboard for changes via
//     clipboard-rs, stores new entries in SQLite + FileStorage.
//   - handle_message: serves "subscribe", "paste", "delete"
//     requests from the frontend plugin UI.
//
// Storage:
//   - SqlStorage: clipboard_entries + clipboard_content tables
//     with FTS5 for full-text search.
//   - FileStorage: image blobs stored as PNG in sharded dirs.
//
// The watcher notifies subscriber channels on each new entry
// so the frontend receives live updates.
// =========================================================

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use anyhow::{Context, Result};
use clipboard_rs::{
    Clipboard, ClipboardContext, ClipboardHandler, ClipboardWatcher, ClipboardWatcherContext,
    ContentFormat, WatcherShutdown,
};
// RustImage is a trait that must be in scope for method calls
// on RustImageData (is_empty, to_png, get_size, from_bytes).
use clipboard_rs::common::RustImage;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::Manager;

use crate::platform::clipboard::ClipboardPlatform;
use crate::search::types::{Action, ActionId, CatalogEntry, EntryIcon, PostAction};
use crate::storage::{FileStorage, SqlStorage, StorageKey};

use super::CatalogPlugin;

// =========================================================
// Plugin ID
// =========================================================

const PLUGIN_ID: &str = "clipboard-manager";

// =========================================================
// Database Schema
// =========================================================

/// Single migration that creates the clipboard schema including
/// FTS5 for full-text search and triggers to keep the index in
/// sync with the content table.
const MIGRATION_001: &str = "
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClipboardHistoryEntry {
    pub id: String,
    pub captured_at: String,
    pub preview: String,
    pub formats: Vec<String>,
    /// Absolute path to the image file, if this entry has an
    /// image format. The frontend converts this to an asset URL.
    pub image_path: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SubscribePayload {
    #[serde(default)]
    query: Option<String>,
    #[serde(default = "default_limit")]
    limit: usize,
}

fn default_limit() -> usize {
    50
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct EntryIdPayload {
    id: String,
}

// =========================================================
// Retention
// =========================================================

/// Entries older than this many days are deleted on startup
/// and periodically during operation.
const RETENTION_DAYS: u32 = 30;

/// Run retention cleanup every N captured entries.
const RETENTION_INTERVAL: u32 = 100;

// =========================================================
// Shared State
//
// Both the plugin methods and the watcher handler need access
// to the same SQL/file storage and subscriber list. This inner
// struct is wrapped in an Arc so it can be shared across threads.
// =========================================================

struct SharedState {
    sql: SqlStorage,
    files: FileStorage,
    subscribers: Mutex<Vec<Channel<serde_json::Value>>>,
    capture_count: AtomicU32,
}

impl SharedState {
    /// Query clipboard history, optionally filtering with FTS5.
    fn query_history(
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

    /// Store a new clipboard entry with its content formats.
    fn store_entry(
        &self,
        id: &str,
        preview: &str,
        contents: &[(String, Option<String>, Option<Vec<u8>>)],
    ) -> Result<()> {
        self.sql.execute(
            "INSERT INTO clipboard_entries (id, preview) VALUES (?1, ?2)",
            &[&id as &dyn rusqlite::types::ToSql, &preview],
        )?;

        for (format, text_value, image_data) in contents {
            let file_key = if let Some(data) = image_data {
                let key = StorageKey::new(id);
                self.files
                    .store(&key, data, "png")
                    .context("store clipboard image")?;
                Some(key.to_string())
            } else {
                None
            };

            self.sql.execute(
                "INSERT INTO clipboard_content (entry_id, format, text_value, file_key)
                 VALUES (?1, ?2, ?3, ?4)",
                &[
                    &id as &dyn rusqlite::types::ToSql,
                    format,
                    &text_value.as_deref(),
                    &file_key.as_deref(),
                ],
            )?;
        }

        Ok(())
    }

    /// Delete an entry and its associated files.
    fn delete_entry(&self, id: &str) -> Result<()> {
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
    fn run_retention(&self) -> Result<()> {
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
    fn notify_subscribers(&self) {
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

// =========================================================
// ClipboardPlugin
// =========================================================

pub struct ClipboardPlugin {
    platform: Arc<dyn ClipboardPlatform>,
    state: OnceLock<Arc<SharedState>>,
    shutdown: Arc<AtomicBool>,
    watcher_shutdown: Mutex<Option<WatcherShutdown>>,
}

impl ClipboardPlugin {
    pub fn new(platform: impl ClipboardPlatform + 'static) -> Self {
        Self {
            platform: Arc::new(platform),
            state: OnceLock::new(),
            shutdown: Arc::new(AtomicBool::new(false)),
            watcher_shutdown: Mutex::new(None),
        }
    }

    fn state(&self) -> &Arc<SharedState> {
        self.state.get().expect("clipboard state initialized")
    }
}

// =========================================================
// Watcher Handler
// =========================================================

/// Clipboard change handler passed to clipboard-rs. Reads new
/// content, checks platform flags, stores the entry, and
/// notifies subscribers.
struct WatcherHandler {
    platform: Arc<dyn ClipboardPlatform>,
    state: Arc<SharedState>,
    shutdown: Arc<AtomicBool>,
    clipboard: ClipboardContext,
}

impl ClipboardHandler for WatcherHandler {
    fn on_clipboard_change(&mut self) {
        if self.shutdown.load(Ordering::Relaxed) {
            return;
        }

        // Skip our own writes and sensitive content (e.g.,
        // password manager entries).
        if self.platform.is_self_written() || self.platform.is_sensitive() {
            return;
        }

        if let Err(e) = self.capture() {
            eprintln!("clipboard: capture failed: {e:#}");
        }
    }
}

impl WatcherHandler {
    fn capture(&mut self) -> Result<()> {
        let id = ulid::Ulid::new().to_string().to_lowercase();
        let mut contents: Vec<(String, Option<String>, Option<Vec<u8>>)> = Vec::new();
        let mut preview = String::new();

        // Read all available formats. Text formats go into the
        // SQL content table; images go to FileStorage as PNG.
        if self.clipboard.has(ContentFormat::Text) {
            if let Ok(text) = self.clipboard.get_text() {
                if !text.is_empty() {
                    if preview.is_empty() {
                        preview = text.chars().take(500).collect();
                    }
                    contents.push(("text".to_string(), Some(text), None));
                }
            }
        }

        if self.clipboard.has(ContentFormat::Html) {
            if let Ok(html) = self.clipboard.get_html() {
                if !html.is_empty() {
                    contents.push(("html".to_string(), Some(html), None));
                }
            }
        }

        if self.clipboard.has(ContentFormat::Rtf) {
            if let Ok(rtf) = self.clipboard.get_rich_text() {
                if !rtf.is_empty() {
                    contents.push(("rtf".to_string(), Some(rtf), None));
                }
            }
        }

        // Only capture images when the clipboard does NOT contain
        // files. When a file is copied from Finder, macOS puts the
        // file's type icon on the clipboard as the "image"
        // representation — not the file's actual content.
        let has_files = self.clipboard.has(ContentFormat::Files);

        if !has_files && self.clipboard.has(ContentFormat::Image) {
            if let Ok(image) = self.clipboard.get_image() {
                if !image.is_empty() {
                    if let Ok(png_buf) = image.to_png() {
                        let png_bytes = png_buf.get_bytes().to_vec();
                        if preview.is_empty() {
                            let (w, h) = image.get_size();
                            preview = format!("Image ({w}×{h})");
                        }
                        contents.push(("image".to_string(), None, Some(png_bytes)));
                    }
                }
            }
        }

        // Nothing captured — skip (e.g., file copies).
        if contents.is_empty() {
            return Ok(());
        }

        self.state.store_entry(&id, &preview, &contents)?;
        self.state.notify_subscribers();

        // Periodic retention cleanup.
        let count = self.state.capture_count.fetch_add(1, Ordering::Relaxed);
        if count > 0 && count % RETENTION_INTERVAL == 0 {
            if let Err(e) = self.state.run_retention() {
                eprintln!("clipboard: periodic retention failed: {e:#}");
            }
        }

        Ok(())
    }
}

// =========================================================
// CatalogPlugin Implementation
// =========================================================

impl CatalogPlugin for ClipboardPlugin {
    fn id(&self) -> &str {
        PLUGIN_ID
    }

    fn setup(&self, app: &tauri::AppHandle) {
        let data_dir = app
            .path()
            .app_data_dir()
            .expect("resolve app data dir")
            .join("plugins")
            .join(PLUGIN_ID);

        let db_path = data_dir.join("clipboard.db");
        let files_dir = data_dir.join("files");

        let sql = SqlStorage::open(db_path, &[MIGRATION_001])
            .expect("open clipboard database");
        let files = FileStorage::new(files_dir);

        let shared = Arc::new(SharedState {
            sql,
            files,
            subscribers: Mutex::new(Vec::new()),
            capture_count: AtomicU32::new(0),
        });

        assert!(
            self.state.set(Arc::clone(&shared)).is_ok(),
            "clipboard state already initialized"
        );

        // Clean up old entries on startup.
        if let Err(e) = shared.run_retention() {
            eprintln!("clipboard: retention cleanup failed: {e:#}");
        }

        // Spawn the clipboard watcher on a dedicated thread.
        let mut watcher_ctx: ClipboardWatcherContext<WatcherHandler> =
            ClipboardWatcherContext::new().expect("create clipboard watcher");

        let handler = WatcherHandler {
            platform: Arc::clone(&self.platform),
            state: Arc::clone(&shared),
            shutdown: Arc::clone(&self.shutdown),
            clipboard: ClipboardContext::new().expect("clipboard context"),
        };

        watcher_ctx.add_handler(handler);
        let ws = watcher_ctx.get_shutdown_channel();

        *self
            .watcher_shutdown
            .lock()
            .expect("watcher_shutdown not poisoned") = Some(ws);

        // start_watch() blocks, so run on a dedicated thread.
        thread::spawn(move || {
            watcher_ctx.start_watch();
        });
    }

    fn teardown(&self) {
        self.shutdown.store(true, Ordering::Relaxed);

        if let Some(shutdown) = self
            .watcher_shutdown
            .lock()
            .expect("watcher_shutdown not poisoned")
            .take()
        {
            shutdown.stop();
        }
    }

    fn entries(&self) -> Vec<CatalogEntry> {
        vec![CatalogEntry {
            id: "clipboard-history".into(),
            title: "Clipboard History".into(),
            subtitle: Some("Browse and paste from clipboard history".into()),
            icon: Some(EntryIcon::HeroIcon("clipboard-document-list".into())),
            keywords: vec![
                "clipboard".into(),
                "paste".into(),
                "history".into(),
                "copy".into(),
            ],
            actions: vec![Action {
                id: ActionId::Open,
                label: "Open".into(),
                keybinding: None,
            }],
        }]
    }

    fn execute(
        &self,
        _entry_id: &str,
        action_id: &ActionId,
        _app: &tauri::AppHandle,
    ) -> Result<PostAction> {
        match action_id {
            ActionId::Open => Ok(PostAction::ShowCustomUI),
            other => anyhow::bail!("unsupported action {other:?} for clipboard-manager"),
        }
    }

    fn handle_message(
        &self,
        method: &str,
        payload: serde_json::Value,
        channel: Channel<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let state = self.state();

        match method {
            // -----------------------------------------------
            // Subscribe: store the channel for live updates,
            // return the current history as the initial snapshot.
            // -----------------------------------------------
            "subscribe" => {
                let params: SubscribePayload =
                    serde_json::from_value(payload).context("parse subscribe payload")?;

                {
                    let mut subs = state.subscribers.lock().expect("subscribers not poisoned");
                    subs.push(channel);
                }

                let history = state.query_history(params.query.as_deref(), params.limit)?;
                Ok(serde_json::to_value(&history).context("serialize history")?)
            }

            // -----------------------------------------------
            // Delete: remove an entry and notify subscribers.
            // -----------------------------------------------
            "delete" => {
                let params: EntryIdPayload =
                    serde_json::from_value(payload).context("parse delete payload")?;
                state.delete_entry(&params.id)?;
                state.notify_subscribers();
                Ok(serde_json::Value::Null)
            }

            // -----------------------------------------------
            // Paste: write the entry's content back to the
            // clipboard and dismiss the launcher.
            // -----------------------------------------------
            "paste" => {
                let params: EntryIdPayload =
                    serde_json::from_value(payload).context("parse paste payload")?;

                let formats: Vec<(String, Option<String>, Option<String>)> = state
                    .sql
                    .query_map(
                        "SELECT format, text_value, file_key FROM clipboard_content
                         WHERE entry_id = ?1",
                        &[&params.id as &dyn rusqlite::types::ToSql],
                        |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
                    )?;

                let ctx =
                    ClipboardContext::new().map_err(|e| anyhow::anyhow!("{e}"))?;

                let mut clipboard_contents = Vec::new();

                for (format, text_value, _file_key) in &formats {
                    match format.as_str() {
                        "text" => {
                            if let Some(text) = text_value {
                                clipboard_contents
                                    .push(clipboard_rs::ClipboardContent::Text(text.clone()));
                            }
                        }
                        "html" => {
                            if let Some(html) = text_value {
                                clipboard_contents
                                    .push(clipboard_rs::ClipboardContent::Html(html.clone()));
                            }
                        }
                        "rtf" => {
                            if let Some(rtf) = text_value {
                                clipboard_contents
                                    .push(clipboard_rs::ClipboardContent::Rtf(rtf.clone()));
                            }
                        }
                        "image" => {
                            let key = StorageKey::new(&params.id);
                            if let Ok(Some(data)) = state.files.load(&key, "png") {
                                if let Ok(img) =
                                    clipboard_rs::RustImageData::from_bytes(&data)
                                {
                                    clipboard_contents
                                        .push(clipboard_rs::ClipboardContent::Image(img));
                                }
                            }
                        }
                        _ => {}
                    }
                }

                if !clipboard_contents.is_empty() {
                    ctx.set(clipboard_contents)
                        .map_err(|e| anyhow::anyhow!("{e}"))?;

                    // Mark as our own write so the watcher skips it.
                    if let Err(e) = self.platform.mark_self_written() {
                        eprintln!("clipboard: mark_self_written failed: {e:#}");
                    }
                }

                Ok(serde_json::json!({ "postAction": "Dismiss" }))
            }

            other => anyhow::bail!("unknown clipboard message method: {other}"),
        }
    }
}
