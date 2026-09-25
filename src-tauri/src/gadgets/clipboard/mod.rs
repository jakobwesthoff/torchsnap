// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Clipboard Manager Gadget
//
// Catalog gadget that monitors the system clipboard, stores
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
//   The host manages enable/disable. On enable(), the gadget
//   opens its database, starts the clipboard watcher and the
//   retention cleanup thread. On disable(), both are stopped.
//   The host calls setting_changed() for runtime updates to
//   retentionDays and bringToFrontOnPaste.
// =========================================================

mod formats;
mod schema;
mod search;
mod storage;
mod watcher;

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use clipboard_rs::{
    Clipboard, ClipboardContext, ClipboardWatcher, ClipboardWatcherContext, WatcherShutdown,
};
use tauri::ipc::Channel;

use crate::caps::{CapRequest, ProvisionedCaps};
use crate::commands::types::{CatalogEntry, EntryActions, EntryIcon, PostAction};
use crate::platform::clipboard::ClipboardPlatform;
use crate::settings::SettingsInit;
use crate::storage::{FileStorage, SqlStorage};

use self::formats::captured_to_clipboard_contents;
use self::schema::{EntryIdPayload, MIGRATION_001, PLUGIN_ID, SearchPayload};
use self::storage::SharedState;
use self::watcher::WatcherHandler;

use super::{Gadget, Search};

/// How often the retention cleanup thread wakes to delete expired
/// entries. Chosen to be infrequent enough to be negligible, but
/// frequent enough that disk usage doesn't grow unbounded.
const RETENTION_CLEANUP_INTERVAL: Duration = Duration::from_secs(30 * 60);

// =========================================================
// WatcherLifecycle — shared mutable watcher state
//
// Extracted into its own struct behind Arc<Mutex<...>> so that
// both the lifecycle management thread and disable() can
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
// ClipboardGadget
// =========================================================

pub struct ClipboardGadget {
    caps: Arc<ProvisionedCaps>,
    platform: Arc<dyn ClipboardPlatform>,

    /// Shared state (DB + file storage). Initialized in `enable()`,
    /// cleared in `disable()`.
    state: Mutex<Option<Arc<SharedState>>>,

    /// Shared lifecycle state for the watcher, accessible from
    /// both the enable/disable path and background threads.
    lifecycle: Arc<Mutex<WatcherLifecycle>>,

    /// Condvar paired with `lifecycle` mutex. Signaled when the
    /// retention thread should wake (either for cleanup or
    /// shutdown).
    retention_condvar: Arc<Condvar>,

    /// Current retention days value. Updated by the host via
    /// `setting_changed("retentionDays", ...)`. Read by the
    /// retention cleanup thread each cycle.
    retention_days: Arc<AtomicU32>,

    /// Whether to bump pasted entries to the top of history.
    /// Updated by the host via `setting_changed("bringToFrontOnPaste", ...)`.
    bring_to_front: AtomicBool,
}

impl ClipboardGadget {
    pub fn cap_requests() -> Vec<CapRequest> {
        vec![CapRequest::Settings, CapRequest::PathResolver]
    }

    pub fn new(caps: Arc<ProvisionedCaps>, platform: impl ClipboardPlatform + 'static) -> Self {
        Self {
            caps,
            platform: Arc::new(platform),
            state: Mutex::new(None),
            lifecycle: Arc::new(Mutex::new(WatcherLifecycle {
                watcher_shutdown: None,
                running: false,
                app_shutting_down: false,
            })),
            retention_condvar: Arc::new(Condvar::new()),
            retention_days: Arc::new(AtomicU32::new(30)),
            bring_to_front: AtomicBool::new(true),
        }
    }

    /// The state `enable()` opened. Messages from a still-open view
    /// can arrive before `enable()` finishes or after `disable()`, so
    /// its absence is an error for the caller, not a bug.
    fn state(&self) -> Result<Arc<SharedState>> {
        self.state
            .lock()
            .expect("state not poisoned")
            .clone()
            .context("clipboard gadget is not enabled")
    }
}

/// The stored `retentionDays` setting as a day count. The settings
/// slider offers 1 to 365, but the store accepts any number: below one
/// day counts as one day, and values beyond `u32` saturate instead of
/// wrapping around to zero.
fn retention_days_from(days: u64) -> u32 {
    u32::try_from(days).unwrap_or(u32::MAX).max(1)
}

/// Start the clipboard watcher and retention cleanup thread.
///
/// Standalone function (not a method) so it can be called from
/// `enable()` without borrowing issues.
fn start_watcher(
    platform: &Arc<dyn ClipboardPlatform>,
    state: &Arc<SharedState>,
    lifecycle: &Arc<Mutex<WatcherLifecycle>>,
    retention_condvar: &Arc<Condvar>,
    retention_days: &Arc<AtomicU32>,
) -> Result<()> {
    let mut lc = lifecycle.lock().expect("lifecycle not poisoned");
    if lc.running {
        return Ok(());
    }

    // ----- Clipboard watcher thread -----
    let mut watcher_ctx: ClipboardWatcherContext<WatcherHandler> =
        ClipboardWatcherContext::new()
            .map_err(|e| anyhow::anyhow!("create clipboard watcher: {e}"))?;

    let handler = WatcherHandler {
        platform: Arc::clone(platform),
        state: Arc::clone(state),
        shutdown: Arc::clone(lifecycle),
        clipboard: ClipboardContext::new()
            .map_err(|e| anyhow::anyhow!("acquire clipboard context: {e}"))?,
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
    let retention_days = Arc::clone(retention_days);

    thread::spawn(move || {
        retention_cleanup_loop(
            retention_state,
            retention_lifecycle,
            retention_cv,
            retention_days,
        );
    });

    Ok(())
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
/// current `retentionDays` from the shared atomic each cycle.
/// Exits when the lifecycle transitions to not-running or app
/// shutdown.
fn retention_cleanup_loop(
    state: Arc<SharedState>,
    lifecycle: Arc<Mutex<WatcherLifecycle>>,
    condvar: Arc<Condvar>,
    retention_days: Arc<AtomicU32>,
) {
    loop {
        let days = retention_days.load(Ordering::Relaxed);

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
// Gadget Implementation
// =========================================================

impl Gadget for ClipboardGadget {
    fn id(&self) -> &str {
        PLUGIN_ID
    }

    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit {
        settings
            .ensure("retentionDays", 30)
            .ensure("bringToFrontOnPaste", true)
            .ensure("shortcut.open-clipboard", "CmdOrCtrl+Shift+V")
    }

    fn shortcuts(&self) -> Vec<super::GadgetShortcut> {
        vec![super::GadgetShortcut {
            id: "open-clipboard",
            label: "Open Clipboard History",
            default_shortcut: "CmdOrCtrl+Shift+V",
            settings_key: "shortcut.open-clipboard",
        }]
    }

    fn handle_shortcut(&self, _shortcut_id: &str) -> anyhow::Result<PostAction> {
        Ok(PostAction::ShowCustomUI {
            view: "history".into(),
            data: None,
        })
    }

    fn enable(&self) -> anyhow::Result<()> {
        // Resolve the gadget data directory from the path resolver cap.
        let data_dir = self
            .caps
            .path_resolver()
            .resolve("gadget-data")
            .expect("gadget-data is a recognized path variable")
            .to_path_buf();

        let db_path = data_dir.join("sql").join("clipboard.sqlite3");
        let files_dir = data_dir.join("files");

        let sql = SqlStorage::open(db_path, &[MIGRATION_001]).context("open clipboard database")?;
        let files = FileStorage::new(files_dir);

        let shared = Arc::new(SharedState {
            sql,
            files,
            active_query: Mutex::new(None),
        });

        *self.state.lock().expect("state not poisoned") = Some(Arc::clone(&shared));

        // Read initial values for settings managed via setting_changed.
        let settings = self.caps.settings();
        let retention = settings
            .get::<u64>("retentionDays")
            .map_or(30, retention_days_from);
        self.retention_days.store(retention, Ordering::Relaxed);

        let bring_to_front: bool = settings.get("bringToFrontOnPaste").unwrap_or(true);
        self.bring_to_front.store(bring_to_front, Ordering::Relaxed);

        // Reset the shutdown flag in case this is a re-enable.
        {
            let mut lc = self.lifecycle.lock().expect("lifecycle not poisoned");
            lc.app_shutting_down = false;
        }

        // ----- Start watcher + retention thread -----
        if let Err(e) = start_watcher(
            &self.platform,
            &shared,
            &self.lifecycle,
            &self.retention_condvar,
            &self.retention_days,
        ) {
            eprintln!("clipboard: failed to start watcher: {e:#}");
        }
        Ok(())
    }

    fn disable(&self) {
        // Signal shutdown and stop the watcher + retention thread.
        {
            let mut lc = self.lifecycle.lock().expect("lifecycle not poisoned");
            lc.app_shutting_down = true;
        }
        stop_watcher(&self.lifecycle, &self.retention_condvar);

        // Drop the host's reference to the database and file storage.
        // Operations already running hold their own `Arc` and finish;
        // the database closes when the last one does.
        self.state.lock().expect("state not poisoned").take();
    }

    fn setting_changed(&self, key: &str, value: serde_json::Value) {
        match key {
            "retentionDays" => {
                if let Some(days) = value.as_u64() {
                    self.retention_days
                        .store(retention_days_from(days), Ordering::Relaxed);
                    // Wake the retention thread so it picks up the new value.
                    self.retention_condvar.notify_all();
                }
            }
            "bringToFrontOnPaste" => {
                if let Some(v) = value.as_bool() {
                    self.bring_to_front.store(v, Ordering::Relaxed);
                }
            }
            _ => {}
        }
    }

    fn handle_message(
        &self,
        method: &str,
        payload: serde_json::Value,
        channel: Channel<serde_json::Value>,
    ) -> Result<serde_json::Value> {
        let state = self.state()?;

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
                let bring_to_front = self.bring_to_front.load(Ordering::Relaxed);

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

/// What the clipboard gadget's catalog entry does.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum Command {
    /// Switch the launcher to the clipboard history view.
    ShowHistory,
}

impl Search for ClipboardGadget {
    type Command = Command;

    fn entries(&self) -> Vec<CatalogEntry<Command>> {
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
            actions: EntryActions::new().primary("Open", Command::ShowHistory),
        }]
    }

    fn execute(&self, command: Command) -> Result<PostAction> {
        match command {
            Command::ShowHistory => Ok(PostAction::ShowCustomUI {
                view: "history".into(),
                data: None,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::PathResolverCap;
    use crate::paths::{GadgetPaths, PlatformPaths};

    struct QuietClipboard;

    impl ClipboardPlatform for QuietClipboard {
        fn is_sensitive(&self) -> bool {
            false
        }

        fn is_self_written(&self) -> bool {
            false
        }

        fn ownership_marker(&self) -> Option<clipboard_rs::ClipboardContent> {
            None
        }
    }

    /// Caps with only the path resolver, so `gadget-data` points at
    /// `data_dir`. The settings cap needs a Tauri store and is left
    /// out, which limits these tests to paths that fail or finish
    /// before reading settings.
    fn gadget_with_data_dir(data_dir: &std::path::Path) -> ClipboardGadget {
        let paths = GadgetPaths {
            platform: Arc::new(PlatformPaths {
                home: data_dir.join("home"),
                xdg_config: data_dir.join("config"),
                xdg_data: data_dir.join("data"),
            }),
            gadget_data: data_dir.to_path_buf(),
            gadget_archive: data_dir.join("archive"),
        };
        let caps = ProvisionedCaps {
            opener: None,
            http: None,
            filesystem: None,
            command: None,
            clipboard: None,
            sql_storage: None,
            website_metadata: None,
            icon_cache: None,
            settings: None,
            frecency: None,
            path_resolver: Some(Arc::new(PathResolverCap::new(Arc::new(paths)))),
        };
        ClipboardGadget::new(Arc::new(caps), QuietClipboard)
    }

    fn opened_state(data_dir: &std::path::Path) -> Arc<SharedState> {
        Arc::new(SharedState {
            sql: SqlStorage::open(data_dir.join("clipboard.sqlite3"), &[MIGRATION_001])
                .expect("a fresh temp dir holds a database"),
            files: FileStorage::new(data_dir.join("files")),
            active_query: Mutex::new(None),
        })
    }

    fn discarding_channel() -> Channel<serde_json::Value> {
        Channel::new(|_| Ok(()))
    }

    #[test]
    fn catalog_entry_opens_the_history_view() {
        let dir = tempfile::tempdir().expect("tempdir");
        let gadget = gadget_with_data_dir(dir.path());

        let entries = Search::entries(&gadget);
        assert_eq!(entries.len(), 1);
        let primary = entries[0].actions.primary.as_ref().expect("primary action");
        assert_eq!(primary.command, Command::ShowHistory);

        let post_action = Search::execute(&gadget, Command::ShowHistory).expect("runs");
        assert!(matches!(
            post_action,
            PostAction::ShowCustomUI { ref view, data: None } if view == "history"
        ));
    }

    #[test]
    fn enable_reports_a_database_that_cannot_be_opened() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            dir.path().join("sql"),
            b"a file where the directory belongs",
        )
        .expect("write blocker file");

        let error = gadget_with_data_dir(dir.path())
            .enable()
            .expect_err("enable fails without a database");

        assert!(format!("{error:#}").contains("open clipboard database"));
    }

    #[test]
    fn messages_before_enable_fail_instead_of_panicking() {
        let dir = tempfile::tempdir().expect("tempdir");
        let gadget = gadget_with_data_dir(dir.path());

        let error = gadget
            .handle_message("stats", serde_json::Value::Null, discarding_channel())
            .expect_err("no state before enable");

        assert!(format!("{error:#}").contains("not enabled"));
    }

    #[test]
    fn disable_releases_the_database() {
        let dir = tempfile::tempdir().expect("tempdir");
        let gadget = gadget_with_data_dir(dir.path());
        *gadget.state.lock().expect("state not poisoned") = Some(opened_state(dir.path()));
        assert!(
            gadget
                .handle_message("stats", serde_json::Value::Null, discarding_channel())
                .is_ok()
        );

        gadget.disable();

        assert!(gadget.state.lock().expect("state not poisoned").is_none());
        assert!(
            gadget
                .handle_message("stats", serde_json::Value::Null, discarding_channel())
                .is_err()
        );
    }

    #[test]
    fn a_retention_below_one_day_counts_as_one_day() {
        let dir = tempfile::tempdir().expect("tempdir");
        let gadget = gadget_with_data_dir(dir.path());

        gadget.setting_changed("retentionDays", serde_json::json!(0));

        assert_eq!(gadget.retention_days.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn an_oversized_retention_saturates_instead_of_wrapping_to_zero() {
        let dir = tempfile::tempdir().expect("tempdir");
        let gadget = gadget_with_data_dir(dir.path());

        gadget.setting_changed("retentionDays", serde_json::json!(4_294_967_296u64));

        assert_eq!(gadget.retention_days.load(Ordering::Relaxed), u32::MAX);
    }

    #[test]
    fn a_retention_in_range_is_used_as_is() {
        let dir = tempfile::tempdir().expect("tempdir");
        let gadget = gadget_with_data_dir(dir.path());

        gadget.setting_changed("retentionDays", serde_json::json!(90));

        assert_eq!(gadget.retention_days.load(Ordering::Relaxed), 90);
    }

    #[test]
    fn a_retention_that_is_not_a_whole_number_is_ignored() {
        let dir = tempfile::tempdir().expect("tempdir");
        let gadget = gadget_with_data_dir(dir.path());

        for value in [
            serde_json::json!(-5),
            serde_json::json!("7"),
            serde_json::json!(1.5),
        ] {
            gadget.setting_changed("retentionDays", value);
        }

        assert_eq!(gadget.retention_days.load(Ordering::Relaxed), 30);
    }
}
