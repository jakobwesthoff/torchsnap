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
//   formats  — format-agnostic extraction and conversion logic
//   schema   — database migrations, serialized types, constants
//   storage  — SharedState (SQL + file storage + active query)
//   watcher  — clipboard change handler (background thread)
//
// Lifecycle:
//   State (DB, file storage) is always initialized in setup().
//   The clipboard watcher and retention thread are only started
//   when the plugin is enabled. The `enabled` setting is watched
//   reactively — toggling it starts/stops the watcher without
//   requiring an app restart.
// =========================================================

mod formats;
mod schema;
mod storage;
mod watcher;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use clipboard_rs::{
    Clipboard, ClipboardContext, ClipboardWatcher, ClipboardWatcherContext, WatcherShutdown,
};
use tauri::Manager;
use tauri::ipc::Channel;

use crate::platform::clipboard::ClipboardPlatform;
use crate::search::types::{Action, ActionId, CatalogEntry, EntryIcon, PostAction};
use crate::settings::SettingsInit;
use crate::settings_notifier::SettingsWatch;
use crate::storage::{FileStorage, SqlStorage};

use self::formats::captured_to_clipboard_contents;
use self::schema::{EntryIdPayload, MIGRATION_001, PLUGIN_ID, SearchPayload};
use self::storage::SharedState;
use self::watcher::WatcherHandler;

use super::{CatalogPlugin, PluginContext};

/// How often the retention cleanup thread wakes to delete expired
/// entries. Chosen to be infrequent enough to be negligible, but
/// frequent enough that disk usage doesn't grow unbounded.
const RETENTION_CLEANUP_INTERVAL: Duration = Duration::from_secs(30 * 60);

// =========================================================
// WatcherLifecycle — shared mutable watcher state
//
// Extracted into its own struct behind Arc<Mutex<...>> so that
// both the lifecycle management thread and teardown() can
// start/stop the watcher independently.
// =========================================================

pub(super) struct WatcherLifecycle {
    /// Shutdown handle for clipboard-rs. Present while running.
    watcher_shutdown: Option<WatcherShutdown>,

    /// Whether the watcher is currently running.
    pub running: bool,

    /// Flipped to true during app teardown. The lifecycle thread
    /// checks this to know when to exit entirely (vs. just
    /// stopping the watcher because `enabled` was toggled off).
    pub app_shutting_down: bool,
}

// =========================================================
// ClipboardPlugin
// =========================================================

pub struct ClipboardPlugin {
    platform: Arc<dyn ClipboardPlatform>,

    /// Shared state (DB + file storage). Always initialized in
    /// `setup()` regardless of enabled state — the data persists
    /// even when the plugin is disabled.
    state: Mutex<Option<Arc<SharedState>>>,

    /// Tracks whether the plugin is currently enabled. Shared
    /// with the lifecycle management thread via Arc so both
    /// sides see the same value.
    enabled: Arc<AtomicBool>,

    /// Shared lifecycle state for the watcher, accessible from
    /// both the lifecycle management thread and teardown().
    lifecycle: Arc<Mutex<WatcherLifecycle>>,

    /// Condvar paired with `lifecycle` mutex. Signaled when the
    /// retention thread should wake (either for cleanup or
    /// shutdown).
    retention_condvar: Arc<Condvar>,

    /// Plugin settings handle for reading settings outside of
    /// `setup()`. Initialized in `setup()`.
    settings: Mutex<Option<crate::settings::PluginSettings>>,
}

impl ClipboardPlugin {
    pub fn new(platform: impl ClipboardPlatform + 'static) -> Self {
        Self {
            platform: Arc::new(platform),
            state: Mutex::new(None),
            enabled: Arc::new(AtomicBool::new(true)),
            lifecycle: Arc::new(Mutex::new(WatcherLifecycle {
                watcher_shutdown: None,
                running: false,
                app_shutting_down: false,
            })),
            retention_condvar: Arc::new(Condvar::new()),
            settings: Mutex::new(None),
        }
    }

    fn state(&self) -> Arc<SharedState> {
        self.state
            .lock()
            .expect("state not poisoned")
            .as_ref()
            .expect("clipboard state initialized")
            .clone()
    }

    fn setting<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        self.settings
            .lock()
            .expect("settings not poisoned")
            .as_ref()
            .expect("clipboard settings initialized")
            .get(key)
    }
}

/// Start the clipboard watcher and retention cleanup thread.
///
/// Standalone function (not a method) so it can be called from
/// the lifecycle management thread without holding a reference
/// to `ClipboardPlugin`.
fn start_watcher(
    platform: &Arc<dyn ClipboardPlatform>,
    state: &Arc<SharedState>,
    lifecycle: &Arc<Mutex<WatcherLifecycle>>,
    retention_condvar: &Arc<Condvar>,
    retention_days_watch: &SettingsWatch<u32>,
) {
    let mut lc = lifecycle.lock().expect("lifecycle not poisoned");
    if lc.running {
        return;
    }

    // ----- Clipboard watcher thread -----
    let mut watcher_ctx: ClipboardWatcherContext<WatcherHandler> =
        ClipboardWatcherContext::new().expect("create clipboard watcher");

    let handler = WatcherHandler {
        platform: Arc::clone(platform),
        state: Arc::clone(state),
        shutdown: Arc::clone(lifecycle),
        clipboard: ClipboardContext::new().expect("clipboard context"),
    };

    watcher_ctx.add_handler(handler);
    lc.watcher_shutdown = Some(watcher_ctx.get_shutdown_channel());
    lc.running = true;

    // Drop the lock before spawning threads.
    drop(lc);

    thread::spawn(move || {
        watcher_ctx.start_watch();
    });

    // ----- Retention cleanup thread -----
    let retention_state = Arc::clone(state);
    let retention_lifecycle = Arc::clone(lifecycle);
    let retention_cv = Arc::clone(retention_condvar);
    let retention_days = retention_days_watch.clone();

    thread::spawn(move || {
        retention_cleanup_loop(
            retention_state,
            retention_lifecycle,
            retention_cv,
            retention_days,
        );
    });
}

/// Stop the clipboard watcher and signal the retention thread
/// to exit.
fn stop_watcher(lifecycle: &Arc<Mutex<WatcherLifecycle>>, retention_condvar: &Arc<Condvar>) {
    let mut lc = lifecycle.lock().expect("lifecycle not poisoned");
    if !lc.running {
        return;
    }

    lc.running = false;

    if let Some(shutdown) = lc.watcher_shutdown.take() {
        shutdown.stop();
    }

    // Wake the retention thread so it sees the flag and exits.
    retention_condvar.notify_all();
}

/// Retention cleanup loop. Runs on a dedicated thread, sleeping
/// for `RETENTION_CLEANUP_INTERVAL` between cycles. Reads the
/// current `retentionDays` setting each cycle. Exits when the
/// lifecycle transitions to not-running or app shutdown.
fn retention_cleanup_loop(
    state: Arc<SharedState>,
    lifecycle: Arc<Mutex<WatcherLifecycle>>,
    condvar: Arc<Condvar>,
    days_watch: SettingsWatch<u32>,
) {
    loop {
        let days = days_watch.get();

        if let Err(e) = state.delete_expired_entries(days) {
            eprintln!("clipboard: retention cleanup failed: {e:#}");
        }

        // Sleep until either the interval elapses or we're
        // signaled to wake (shutdown/disable).
        let lc = lifecycle.lock().expect("lifecycle not poisoned");
        let result = condvar
            .wait_timeout(lc, RETENTION_CLEANUP_INTERVAL)
            .expect("lifecycle not poisoned");

        // Check if we should exit: either no longer running
        // (disabled) or app is shutting down.
        let lc = result.0;
        if !lc.running || lc.app_shutting_down {
            break;
        }
    }
}

// =========================================================
// CatalogPlugin Implementation
// =========================================================

impl CatalogPlugin for ClipboardPlugin {
    fn id(&self) -> &str {
        PLUGIN_ID
    }

    fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }

    fn enabled_settings_key(&self) -> Option<&'static str> {
        Some("enabled")
    }

    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit {
        settings
            .ensure("enabled", true)
            .ensure("retentionDays", 30)
            .ensure("bringToFrontOnPaste", true)
            .ensure("shortcut.open-clipboard", "CmdOrCtrl+Shift+V")
    }

    fn shortcuts(&self) -> Vec<super::PluginShortcut> {
        vec![super::PluginShortcut {
            id: "open-clipboard",
            label: "Open Clipboard History",
            default_shortcut: "CmdOrCtrl+Shift+V",
            settings_key: "shortcut.open-clipboard",
        }]
    }

    fn handle_shortcut(
        &self,
        _shortcut_id: &str,
        _app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        Ok(PostAction::ShowCustomUI)
    }

    fn setup(&self, app: &tauri::AppHandle, ctx: &PluginContext) {
        // ----- Always initialize state (DB + file storage) -----
        let data_dir = app
            .path()
            .app_data_dir()
            .expect("resolve app data dir")
            .join("plugins")
            .join(PLUGIN_ID);

        let db_path = data_dir.join("clipboard.db");
        let files_dir = data_dir.join("files");

        let sql = SqlStorage::open(db_path, &[MIGRATION_001]).expect("open clipboard database");
        let files = FileStorage::new(files_dir);

        let shared = Arc::new(SharedState {
            sql,
            files,
            active_query: Mutex::new(None),
        });

        *self.state.lock().expect("state not poisoned") = Some(Arc::clone(&shared));
        *self.settings.lock().expect("settings not poisoned") = Some(ctx.settings.clone());

        // ----- Read initial enabled state -----
        let initial_enabled: bool = ctx.settings.get("enabled").unwrap_or(true);
        self.enabled.store(initial_enabled, Ordering::Relaxed);

        // Subscribe to future changes for both settings.
        let mut enabled_watch: SettingsWatch<bool> = ctx.notifier.watch("enabled");
        let retention_days_watch: SettingsWatch<u32> = ctx.notifier.watch("retentionDays");

        // ----- Start watcher if enabled -----
        if initial_enabled {
            start_watcher(
                &self.platform,
                &shared,
                &self.lifecycle,
                &self.retention_condvar,
                &retention_days_watch,
            );
        }

        // ----- Lifecycle management thread -----
        //
        // Watches the `enabled` setting and starts/stops the
        // watcher accordingly. Runs until app shutdown.
        let platform = Arc::clone(&self.platform);
        let state_for_lifecycle = Arc::clone(&shared);
        let lifecycle = Arc::clone(&self.lifecycle);
        let retention_condvar = Arc::clone(&self.retention_condvar);
        let enabled_flag = Arc::clone(&self.enabled);

        thread::spawn(move || {
            loop {
                // Block until `enabled` changes.
                let Some(new_enabled) = enabled_watch.blocking_changed() else {
                    // Sender dropped — app is shutting down.
                    break;
                };

                enabled_flag.store(new_enabled, Ordering::Relaxed);

                {
                    let lc = lifecycle.lock().expect("lifecycle not poisoned");
                    if lc.app_shutting_down {
                        break;
                    }
                }

                if new_enabled {
                    start_watcher(
                        &platform,
                        &state_for_lifecycle,
                        &lifecycle,
                        &retention_condvar,
                        &retention_days_watch,
                    );
                } else {
                    stop_watcher(&lifecycle, &retention_condvar);
                }
            }
        });
    }

    fn teardown(&self) {
        // Signal app shutdown and stop everything.
        {
            let mut lc = self.lifecycle.lock().expect("lifecycle not poisoned");
            lc.app_shutting_down = true;
        }
        stop_watcher(&self.lifecycle, &self.retention_condvar);
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
            // Search: run a filtered query and stream results
            // through the channel. The channel is stored as the
            // active query so that data changes (new entry,
            // delete, clear) can re-run the query and push
            // updated results automatically.
            // -----------------------------------------------
            "search" => {
                let params: SearchPayload =
                    serde_json::from_value(payload).context("parse search payload")?;

                let history = state.search_history(params.query.as_deref())?;
                let payload = serde_json::to_value(&history).context("serialize search results")?;

                // Push initial results through the channel — same path
                // as subsequent updates from refresh_active_query.
                channel
                    .send(payload)
                    .context("send initial search results")?;

                // Store the channel for future data-change pushes.
                // Replacing the previous ActiveQuery drops the old
                // channel, which closes it on the frontend side.
                {
                    let mut active = state
                        .active_query
                        .lock()
                        .expect("active_query not poisoned");
                    *active = Some(storage::ActiveQuery {
                        channel,
                        query: params.query,
                    });
                }

                Ok(serde_json::Value::Null)
            }

            // -----------------------------------------------
            // Load full entry: return complete detail data for
            // a single entry (formats, image path, longer preview).
            // -----------------------------------------------
            "load_full_entry" => {
                let params: EntryIdPayload =
                    serde_json::from_value(payload).context("parse load_full_entry payload")?;
                let entry = state.load_full_entry(&params.id)?;
                Ok(serde_json::to_value(&entry).context("serialize full entry")?)
            }

            // -----------------------------------------------
            // Delete: remove an entry and refresh the active
            // query so the UI reflects the change.
            // -----------------------------------------------
            "delete" => {
                let params: EntryIdPayload =
                    serde_json::from_value(payload).context("parse delete payload")?;
                state.delete_entry(&params.id)?;
                state.refresh_active_query();
                Ok(serde_json::Value::Null)
            }

            // -----------------------------------------------
            // Paste: write the entry's content back to the
            // clipboard and dismiss the launcher.
            // -----------------------------------------------
            "paste" => {
                let params: EntryIdPayload =
                    serde_json::from_value(payload).context("parse paste payload")?;

                let captured = state.load_captured_formats(&params.id)?;
                let platform = Arc::clone(&self.platform);

                // When enabled, bump the entry's timestamp so it moves
                // to the top of the history list. The ownership marker
                // prevents the watcher from re-processing the paste,
                // so dedup won't run — we handle the update directly.
                let bring_to_front: bool = self.setting("bringToFrontOnPaste").unwrap_or(true);

                if bring_to_front {
                    state
                        .touch_entry(&params.id)
                        .context("touch pasted entry")?;
                    state.refresh_active_query();
                }

                thread::spawn(move || {
                    let mut clipboard_contents = captured_to_clipboard_contents(&captured);

                    if clipboard_contents.is_empty() {
                        return;
                    }

                    if let Some(marker) = platform.ownership_marker() {
                        clipboard_contents.push(marker);
                    }

                    let ctx = match ClipboardContext::new() {
                        Ok(c) => c,
                        Err(e) => {
                            eprintln!("clipboard: create context for paste: {e}");
                            return;
                        }
                    };

                    if let Err(e) = ctx.set(clipboard_contents) {
                        eprintln!("clipboard: write to clipboard: {e}");
                    }
                });

                Ok(serde_json::Value::Null)
            }

            // -----------------------------------------------
            // Stats: return history statistics for the
            // settings UI (entry counts, storage size).
            // -----------------------------------------------
            "stats" => {
                let stats = state.stats()?;
                Ok(serde_json::to_value(&stats).context("serialize stats")?)
            }

            // -----------------------------------------------
            // Clear history: delete all entries and files,
            // then refresh the active query so the UI updates.
            // -----------------------------------------------
            "clear_history" => {
                state.clear_all()?;
                state.refresh_active_query();
                Ok(serde_json::Value::Null)
            }

            other => anyhow::bail!("unknown clipboard message method: {other}"),
        }
    }
}
