// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Clipboard Manager Plugin
//
// Catalog plugin that monitors the system clipboard, stores
// history across all content formats, and lets users browse
// and paste previous entries via a custom UI.
//
// Module layout:
//   formats  — format-agnostic capture and restore logic
//   schema   — database migrations, serialized types, constants
//   storage  — SharedState (SQL + file storage + subscribers)
//   watcher  — clipboard change handler (background thread)
// =========================================================

mod formats;
mod schema;
mod storage;
mod watcher;

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread;

use anyhow::{Context, Result};
use clipboard_rs::{
    Clipboard, ClipboardContext, ClipboardWatcher, ClipboardWatcherContext, WatcherShutdown,
};
use tauri::ipc::Channel;
use tauri::Manager;

use crate::platform::clipboard::ClipboardPlatform;
use crate::search::types::{Action, ActionId, CatalogEntry, EntryIcon, PostAction};
use crate::storage::{FileStorage, SqlStorage};

use self::formats::{restore_contents, StoredContent};
use self::schema::{EntryIdPayload, SubscribePayload, MIGRATION_001, PLUGIN_ID};
use self::storage::SharedState;
use self::watcher::WatcherHandler;

use super::CatalogPlugin;

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

                // Load all stored content rows for this entry.
                let stored: Vec<StoredContent> = state
                    .sql
                    .query_map(
                        "SELECT format, text_value, file_key FROM clipboard_content
                         WHERE entry_id = ?1",
                        &[&params.id as &dyn rusqlite::types::ToSql],
                        |row| {
                            Ok(StoredContent {
                                format: row.get(0)?,
                                text_value: row.get(1)?,
                                file_key: row.get(2)?,
                            })
                        },
                    )?;

                // Reconstruct clipboard contents and write them.
                let clipboard_contents =
                    restore_contents(&stored, &state.files);

                if !clipboard_contents.is_empty() {
                    let ctx = ClipboardContext::new()
                        .map_err(|e| anyhow::anyhow!("{e}"))?;
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
