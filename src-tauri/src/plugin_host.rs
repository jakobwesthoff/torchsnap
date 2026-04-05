// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Plugin Host
//
// Central authority for the plugin lifecycle. Owns all plugin
// references and is the single entry point for:
//
// - Registration (register)
// - Settings initialization (Phase 1: synchronous defaults)
// - Parallel plugin enable (Phase 2: tokio spawn_blocking)
// - Host-managed enable/disable via `enabled.<id>` keys
// - Settings change dispatch via CoalescingDispatcher
// - Global shortcut registration and reactive re-registration
// - Search routing (nucleo + prefix matching)
// - Action execution and message routing
// - Shutdown (disable all plugins)
//
// Managed as `Arc<PluginHost>` in Tauri state — no Mutex needed
// since all fields are either immutable after init or use
// interior mutability (AtomicBool, watch channels).
// =========================================================

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_store::Store;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::coalescing_dispatcher::CoalescingDispatcher;
use crate::frecency::{FrecencyStore, PluginFrecency};
use crate::platform::{LauncherPanel as _, PlatformLauncherPanel};
use crate::plugins::{Plugin, PluginContext, PluginShortcut};
use crate::search::types::{
    ActionId, PluginResponse, PluginViewRef, PostAction,
    ScoredEntry, SearchMessage, SourcedEntry,
};
use crate::settings::{PluginSettings, SettingsInit};
use crate::unicode::Utf16Positions;

// =========================================================
// Internal Helpers
// =========================================================

/// Distinguishes CustomUI from InlineUI in `process_plugin_response`.
enum ViewKind {
    Custom,
    Inline,
}

// =========================================================
// Shortcut Types
// =========================================================

/// A registered shortcut with enough context to route the
/// activation back to the owning plugin.
struct RegisteredShortcut {
    shortcut: Shortcut,
    plugin_id: String,
    shortcut_id: String,
    owner: Arc<dyn Plugin>,
}

/// Payload emitted with the `activate-plugin-custom-ui` event.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivatePluginPayload {
    plugin_id: String,
    view: String,
    data: Option<serde_json::Value>,
}

// =========================================================
// PluginSlot — per-plugin state managed by the host
// =========================================================

/// Wraps a plugin with host-managed lifecycle state. The host
/// owns the enabled flag and the settings dispatcher — plugins
/// never manage their own enabled state.
struct PluginSlot {
    plugin: Arc<dyn Plugin>,

    /// Host-owned enabled flag. Checked before including the
    /// plugin in search results, shortcut registration, etc.
    /// Updated by the host when `enabled.<id>` changes in the
    /// settings store.
    enabled: AtomicBool,

    /// Serializes and deduplicates settings change dispatch for
    /// this plugin. Both `enabled.<id>` changes and
    /// `plugins.<id>.*` changes are funneled through here.
    dispatcher: CoalescingDispatcher,
}

impl PluginSlot {
    fn new(plugin: Arc<dyn Plugin>) -> Self {
        Self {
            plugin,
            enabled: AtomicBool::new(true),
            dispatcher: CoalescingDispatcher::new(),
        }
    }

    /// Whether this plugin is currently active. The host owns
    /// this flag — plugins never manage their own enabled state.
    fn is_active(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
}

// =========================================================
// PluginHost
// =========================================================

pub struct PluginHost {
    slots: Vec<PluginSlot>,
    store: Arc<Store<tauri::Wry>>,
    frecency: Arc<FrecencyStore>,

    /// Settings keys that affect shortcut registration. When
    /// any of these change, the reactor re-registers all shortcuts.
    watched_keys: HashSet<String>,

    /// Sender side of the shortcut re-registration signal.
    /// Capacity 1 — rapid changes coalesce naturally.
    shortcut_signal_tx: mpsc::Sender<()>,

    /// Receiver is taken out once in `initialize_and_start` and
    /// moved into the reactor task. `None` after that.
    shortcut_signal_rx: std::sync::Mutex<Option<mpsc::Receiver<()>>>,
}

impl PluginHost {
    pub fn new(
        store: Arc<Store<tauri::Wry>>,
        frecency: Arc<FrecencyStore>,
    ) -> Self {
        let (tx, rx) = mpsc::channel(1);
        Self {
            slots: Vec::new(),
            store,
            frecency,
            watched_keys: HashSet::new(),
            shortcut_signal_tx: tx,
            shortcut_signal_rx: std::sync::Mutex::new(Some(rx)),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        self.slots.push(PluginSlot::new(Arc::from(plugin)));
    }

    // =========================================================
    // Initialization
    // =========================================================

    /// Initialize settings, register shortcuts, enable plugins,
    /// and spawn the shortcut reactor.
    ///
    /// Must be called exactly once after all plugins are registered
    /// and before `app.manage()` stores the host.
    pub fn initialize_and_start(&mut self, app: &tauri::AppHandle) {
        // -------------------------------------------------------
        // Phase 1: Initialize plugin settings defaults (synchronous)
        // -------------------------------------------------------
        for slot in &self.slots {
            let id = slot.plugin.id();
            let prefix = format!("plugins.{id}.");
            let current = SettingsInit::from_store(&self.store, &prefix);
            let initialized = slot.plugin.initialize_settings(current);
            initialized.apply(&self.store, &prefix);

            // Host-managed enabled key: `enabled.<id>`.
            // Defaults to true if no value exists.
            let enabled_key = format!("enabled.{id}");
            if self.store.get(&enabled_key).is_none() {
                // Migration: if the plugin previously stored enabled
                // state at `plugins.<id>.enabled`, copy that value
                // to the new top-level key.
                let old_key = format!("plugins.{id}.enabled");
                let migrated_value = self
                    .store
                    .get(&old_key)
                    .and_then(|v| v.as_bool());
                let initial = migrated_value.unwrap_or(true);
                self.store.set(enabled_key.clone(), serde_json::Value::Bool(initial));
            }

            // Read the current enabled state and apply to the slot.
            let enabled = self
                .store
                .get(&enabled_key)
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            slot.enabled.store(enabled, Ordering::Relaxed);
        }

        // -------------------------------------------------------
        // Collect watched keys for reactive re-registration
        // -------------------------------------------------------
        self.watched_keys.insert("globalShortcut".to_string());

        let mut keys_to_watch = Vec::new();
        for slot in &self.slots {
            let id = slot.plugin.id();

            // Watch the host-managed enabled key for shortcut reactor.
            keys_to_watch.push(format!("enabled.{id}"));

            // Watch shortcut keys so the reactor re-registers when
            // a user changes a shortcut binding.
            for s in slot.plugin.shortcuts() {
                keys_to_watch.push(format!("plugins.{id}.{}", s.settings_key));
            }
        }
        self.watched_keys.extend(keys_to_watch);

        // -------------------------------------------------------
        // Register initial shortcuts
        // -------------------------------------------------------
        self.register_all_shortcuts(app);

        // -------------------------------------------------------
        // Phase 2: Parallel plugin startup (background)
        //
        // Call enable() on each plugin that is initially enabled.
        //
        // We obtain the runtime handle explicitly because this
        // method is called from Tauri's synchronous setup()
        // callback, where no Tokio guard is active on the current
        // thread.
        // -------------------------------------------------------
        let runtime = tauri::async_runtime::handle();
        let handle = app.clone();
        for slot in &self.slots {
            if !slot.enabled.load(Ordering::Relaxed) {
                continue;
            }

            let p = Arc::clone(&slot.plugin);
            let h = handle.clone();
            let ctx = PluginContext {
                settings: PluginSettings::new(Arc::clone(&self.store), p.id()),
                frecency: PluginFrecency::new(Arc::clone(&self.frecency), p.id()),
            };
            runtime.spawn_blocking(move || {
                p.enable(&h, &ctx);
            });
        }
    }

    /// Spawn the shortcut reactor task. Called once after the host
    /// is wrapped in `Arc` and managed as Tauri state, so we can
    /// pass `Arc<PluginHost>` into the async task.
    pub fn start_shortcut_reactor(self: &Arc<Self>, app: &tauri::AppHandle) {
        let rx = self
            .shortcut_signal_rx
            .lock()
            .expect("shortcut_signal_rx not poisoned")
            .take()
            .expect("start_shortcut_reactor called exactly once");

        let host = Arc::clone(self);
        let handle = app.clone();

        tauri::async_runtime::spawn(async move {
            let mut rx = rx;
            while rx.recv().await.is_some() {
                // Drain any additional queued signals to coalesce
                // rapid changes into a single re-registration.
                while rx.try_recv().is_ok() {}

                host.register_all_shortcuts(&handle);
            }
        });
    }

    /// Check whether a settings key change should trigger shortcut
    /// re-registration.
    pub fn is_key_watched(&self, key: &str) -> bool {
        self.watched_keys.contains(key)
    }

    /// Signal the reactor to re-register shortcuts. Non-blocking;
    /// drops the signal if the channel is full (coalescing).
    pub fn notify_shortcut_change(&self) {
        let _ = self.shortcut_signal_tx.try_send(());
    }

    // =========================================================
    // Shortcut Registration
    // =========================================================

    fn register_all_shortcuts(&self, app: &tauri::AppHandle) {
        // Unregister everything first — the only safe way to
        // re-register with tauri-plugin-global-shortcut.
        let _ = app.global_shortcut().unregister_all();

        let mut registered: Vec<RegisteredShortcut> = Vec::new();

        // Collect shortcuts from all enabled plugins.
        for slot in &self.slots {
            if !slot.is_active() {
                continue;
            }
            let plugin = &slot.plugin;
            let plugin_id = plugin.id().to_string();
            for decl in plugin.shortcuts() {
                if let Some(r) = self.resolve_shortcut(&plugin_id, &decl, Arc::clone(plugin)) {
                    registered.push(r);
                }
            }
        }

        // Read the launcher toggle shortcut.
        let launcher_combo_str = self
            .store
            .get("globalShortcut")
            .and_then(|v| v.as_str().map(String::from))
            .expect("globalShortcut initialized by settings defaults");

        let launcher_shortcut = match launcher_combo_str.parse::<Shortcut>() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("shortcut: invalid launcher combo '{launcher_combo_str}': {e}");
                return;
            }
        };

        // Collect all combos for bulk registration.
        let mut all_combos: Vec<Shortcut> = vec![launcher_shortcut];
        for r in &registered {
            all_combos.push(r.shortcut);
        }

        let registered = Arc::new(registered);
        let handle = app.clone();

        if let Err(e) =
            app.global_shortcut()
                .on_shortcuts(all_combos, move |_app, shortcut, event| {
                    if event.state != ShortcutState::Pressed {
                        return;
                    }

                    // Launcher toggle.
                    if *shortcut == launcher_shortcut {
                        crate::toggle_launcher_window(&handle);
                        return;
                    }

                    // Plugin shortcut routing.
                    let Some(r) = registered.iter().find(|r| r.shortcut == *shortcut) else {
                        return;
                    };

                    let result = r.owner.handle_shortcut(&r.shortcut_id, &handle);

                    match result {
                        Ok(PostAction::ShowCustomUI { view, data }) => {
                            show_launcher_with_plugin(&handle, &r.plugin_id, &view, data);
                        }
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!(
                                "shortcut: {}.{} handler failed: {e:#}",
                                r.plugin_id, r.shortcut_id
                            );
                        }
                    }
                })
        {
            eprintln!("shortcut: failed to register shortcuts: {e}");
        }
    }

    fn resolve_shortcut(
        &self,
        plugin_id: &str,
        decl: &PluginShortcut,
        owner: Arc<dyn Plugin>,
    ) -> Option<RegisteredShortcut> {
        let full_key = format!("plugins.{plugin_id}.{}", decl.settings_key);

        let combo_str = self
            .store
            .get(&full_key)
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| decl.default_shortcut.to_string());

        let shortcut = match combo_str.parse::<Shortcut>() {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "shortcut: invalid combo '{combo_str}' for {plugin_id}.{}: {e}",
                    decl.id
                );
                return None;
            }
        };

        Some(RegisteredShortcut {
            shortcut,
            plugin_id: plugin_id.to_string(),
            shortcut_id: decl.id.to_string(),
            owner,
        })
    }

    // =========================================================
    // Search
    // =========================================================

    /// Search all plugins against the given query, streaming
    /// results to the frontend as they become available.
    ///
    /// Catalog results are sent first (sync, fast). Query plugins
    /// are dispatched concurrently on the blocking thread pool and
    /// their results stream to the frontend as each plugin
    /// completes.
    pub async fn search(&self, query: &str, on_results: &tauri::ipc::Channel<SearchMessage>) {
        if query.is_empty() {
            let _ = on_results.send(SearchMessage::Done);
            return;
        }

        // =======================================================
        // Prefix routing: longest match wins. Exclusive — only
        // the matched plugin runs, no catalogs, no fan-out.
        // =======================================================

        if let Some((plugin, prefix)) = self.find_prefix_match(query) {
            // Note: prefix match already checked is_active() internally
            let stripped = query[prefix.len()..].to_string();
            let source = plugin.id().to_string();
            let prefix_owned = prefix.to_string();
            let plugin = Arc::clone(plugin);

            let prefix_for_search = prefix_owned.clone();
            let response = tokio::task::spawn_blocking(move || {
                plugin.search(&stripped, Some(&prefix_for_search))
            })
            .await
            .expect("prefix search task not panicked");

            let mut custom_plugin_view = None;
            let mut inline_plugin_view = None;
            let mut entries = Vec::new();

            if let Some(response) = response {
                let (view_ref, results) = self.process_plugin_response(
                    response, &source, true, // prefix mode — CustomUI allowed
                );

                if let Some((kind, vr)) = view_ref {
                    match kind {
                        ViewKind::Custom => custom_plugin_view = Some(vr),
                        ViewKind::Inline => inline_plugin_view = Some(vr),
                    }
                }

                entries.extend(results);
            }

            let _ = on_results.send(SearchMessage::SearchResults {
                entries,
                custom_plugin_view,
                inline_plugin_view,
                matched_prefix: Some(prefix_owned),
            });
            let _ = on_results.send(SearchMessage::Done);
            return;
        }

        // =======================================================
        // No prefix: catalog search + concurrent query plugins.
        // =======================================================

        // Phase 1: catalog search (sync, CPU-bound). Send results
        // to the frontend immediately.
        let plugins: Vec<Arc<dyn Plugin>> = self.slots.iter()
            .filter(|s| s.is_active())
            .map(|s| Arc::clone(&s.plugin))
            .collect();
        let frecency = self.frecency.clone();
        let query_owned = query.to_string();

        let catalog_results = tokio::task::spawn_blocking({
            let query = query_owned.clone();
            let frecency = frecency.clone();
            move || Self::search_catalogs_static(&plugins, &frecency, &query)
        })
        .await
        .expect("catalog search task not panicked");

        if !catalog_results.is_empty() {
            let _ = on_results.send(SearchMessage::SearchResults {
                entries: catalog_results,
                custom_plugin_view: None,
                inline_plugin_view: None,
                matched_prefix: None,
            });
        }

        // Phase 2: query plugins — spawn concurrently, deliver
        // results to the frontend as each plugin completes.
        let mut join_set = JoinSet::new();

        for slot in &self.slots {
            if !slot.is_active() {
                continue;
            }

            let source = slot.plugin.id().to_string();
            let plugin = Arc::clone(&slot.plugin);
            let query = query_owned.clone();

            join_set.spawn_blocking(move || {
                (source, plugin.search(&query, None))
            });
        }

        // Drain the JoinSet — each completed task yields one
        // plugin's results, preserving incremental delivery.
        let mut inline_claimed = false;

        while let Some(result) = join_set.join_next().await {
            let (source, response) = result.expect("query search task not panicked");

            let response = match response {
                Some(r) => r,
                None => continue,
            };

            let (view_ref, mut entries) = self.process_plugin_response(
                response, &source, false, // non-prefix — CustomUI downgraded
            );

            self.frecency.apply_scores(&source, &mut entries);
            entries.sort_by(|a, b| a.cmp_sort_key(b));

            let inline_plugin_view = match view_ref {
                Some((ViewKind::Inline, vr)) if !inline_claimed => {
                    inline_claimed = true;
                    Some(vr)
                }
                Some((ViewKind::Inline, _)) => {
                    eprintln!(
                        "search: dropping InlineUI from plugin '{}' — \
                         another plugin already claimed the inline slot",
                        source
                    );
                    None
                }
                _ => None,
            };

            if !entries.is_empty() || inline_plugin_view.is_some() {
                let _ = on_results.send(SearchMessage::SearchResults {
                    entries,
                    custom_plugin_view: None,
                    inline_plugin_view,
                    matched_prefix: None,
                });
            }
        }

        let _ = on_results.send(SearchMessage::Done);
    }

    /// Delegate to the standalone function for testability.
    fn process_plugin_response(
        &self,
        response: PluginResponse,
        source: &str,
        allow_custom_ui: bool,
    ) -> (Option<(ViewKind, PluginViewRef)>, Vec<SourcedEntry>) {
        process_plugin_response(response, source, allow_custom_ui)
    }

    /// Delegate to the standalone function for testability.
    fn find_prefix_match<'a>(&'a self, query: &str) -> Option<(&'a Arc<dyn Plugin>, &'a str)> {
        find_prefix_match(&self.slots, query)
    }

    /// Catalog search as a static method so it can run on
    /// `spawn_blocking` without borrowing `&self`.
    ///
    /// Calls `entries()` on every plugin — catalog-only plugins
    /// return their entry list, query-only plugins return the
    /// default empty vec (zero cost).
    fn search_catalogs_static(
        plugins: &[Arc<dyn Plugin>],
        frecency: &FrecencyStore,
        query: &str,
    ) -> Vec<SourcedEntry> {
        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);

        let mut results = Vec::new();
        let mut char_buf = Vec::new();
        let mut title_indices = Vec::new();

        for plugin in plugins {
            // Disabled plugins are already filtered out by the caller.
            let source = plugin.id().to_string();

            // Track where this plugin's results start so we can
            // apply frecency scores to just this slice afterwards.
            let plugin_start = results.len();

            for entry in plugin.entries() {
                title_indices.clear();
                let title_haystack = Utf32Str::new(&entry.title, &mut char_buf);
                let title_score = pattern.indices(title_haystack, &mut matcher, &mut title_indices);

                let score = match title_score {
                    Some(s) => Some(s),
                    None if !entry.keywords.is_empty() => {
                        let combined = format!("{} {}", entry.title, entry.keywords.join(" "));
                        let combined_haystack = Utf32Str::new(&combined, &mut char_buf);
                        pattern.score(combined_haystack, &mut matcher)
                    }
                    None => None,
                };

                if let Some(score) = score {
                    title_indices.sort_unstable();
                    title_indices.dedup();

                    let title_positions =
                        Utf16Positions::from_graphemes(title_indices.clone(), &entry.title);

                    results.push(SourcedEntry::new(
                        source.clone(),
                        ScoredEntry {
                            id: entry.id,
                            title: entry.title,
                            subtitle: entry.subtitle,
                            icon: entry.icon,
                            score,
                            title_positions,
                            subtitle_positions: Utf16Positions::empty(),
                            actions: entry.actions,
                        },
                    ));
                }
            }

            // Apply frecency bonuses to this plugin's results.
            frecency.apply_scores(&source, &mut results[plugin_start..]);
        }

        // Sort catalog results by the deterministic composite key
        // before sending to the frontend.
        results.sort_by(|a, b| a.cmp_sort_key(b));

        results
    }

    // =========================================================
    // Execute / Message Routing
    // =========================================================

    pub fn execute(
        &self,
        source: &str,
        entry_id: &str,
        action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        // Record frecency before execution — captures user intent
        // regardless of whether the action succeeds.
        self.frecency.record(source, entry_id);

        if let Some(slot) = self.slots.iter().find(|s| s.plugin.id() == source) {
            return slot.plugin.execute(entry_id, action_id, app);
        }
        anyhow::bail!("unknown plugin source: {source}");
    }

    pub fn handle_message(
        &self,
        source: &str,
        method: &str,
        payload: serde_json::Value,
        channel: tauri::ipc::Channel<serde_json::Value>,
    ) -> anyhow::Result<serde_json::Value> {
        if let Some(slot) = self.slots.iter().find(|s| s.plugin.id() == source) {
            return slot.plugin.handle_message(method, payload, channel);
        }
        anyhow::bail!("unknown plugin source: {source}");
    }

    // =========================================================
    // Shutdown
    // =========================================================

    /// Disable all plugins during app exit.
    pub fn disable_all(&self) {
        for slot in &self.slots {
            slot.plugin.disable();
        }
    }

    // =========================================================
    // Settings Change Dispatch
    // =========================================================

    /// Handle a settings change event from the store. Routes
    /// `enabled.<id>` changes to the host-managed lifecycle and
    /// `plugins.<id>.*` changes to the plugin's `setting_changed`.
    ///
    /// Called from the `settings-changed` Tauri event listener.
    /// Both paths go through the plugin's `CoalescingDispatcher`
    /// for serialization and dedup.
    pub fn handle_setting_changed(
        &self,
        key: &str,
        value: serde_json::Value,
        app: &tauri::AppHandle,
    ) {
        // -------------------------------------------------------
        // Path 1: enabled.<plugin-id>
        // -------------------------------------------------------
        if let Some(plugin_id) = key.strip_prefix("enabled.") {
            let Some(slot) = self.slots.iter().find(|s| s.plugin.id() == plugin_id) else {
                return;
            };

            slot.dispatcher.enqueue(key.to_string(), value);

            let plugin = Arc::clone(&slot.plugin);
            let store = Arc::clone(&self.store);
            let frecency = Arc::clone(&self.frecency);
            let enabled_flag = &slot.enabled;
            let app = app.clone();

            slot.dispatcher.dispatch(|dispatch_key, dispatch_value| {
                // Only handle enabled keys in this path.
                let Some(id) = dispatch_key.strip_prefix("enabled.") else {
                    return;
                };

                let new_enabled = dispatch_value.as_bool().unwrap_or(true);
                let was_enabled = enabled_flag.swap(new_enabled, Ordering::Relaxed);

                if new_enabled && !was_enabled {
                    let ctx = PluginContext {
                        settings: PluginSettings::new(Arc::clone(&store), id),
                        frecency: PluginFrecency::new(Arc::clone(&frecency), id),
                    };
                    plugin.enable(&app, &ctx);
                } else if !new_enabled && was_enabled {
                    plugin.disable();
                }
                // If same state → no-op (coalesced to identical value)
            });

            // Always signal shortcut reactor on enabled changes
            // so shortcuts are re-registered accordingly.
            self.notify_shortcut_change();
            return;
        }

        // -------------------------------------------------------
        // Path 2: plugins.<plugin-id>.<setting-key>
        // -------------------------------------------------------
        if let Some(rest) = key.strip_prefix("plugins.") {
            // Split "plugin-id.setting-key" at the first dot.
            let Some(dot_pos) = rest.find('.') else {
                return;
            };
            let plugin_id = &rest[..dot_pos];
            let setting_key = &rest[dot_pos + 1..];

            let Some(slot) = self.slots.iter().find(|s| s.plugin.id() == plugin_id) else {
                return;
            };

            slot.dispatcher
                .enqueue(setting_key.to_string(), value);

            let plugin = Arc::clone(&slot.plugin);
            slot.dispatcher.dispatch(|k, v| {
                plugin.setting_changed(k, v.clone());
            });
        }
    }
}

// =========================================================
// Search Helpers (standalone for testability)
// =========================================================

/// Process a `PluginResponse` into scored entries and an
/// optional view reference.
///
/// `allow_custom_ui` controls whether `CustomUI` responses
/// produce a view reference. In non-prefix (always-on) mode,
/// `CustomUI` is downgraded to plain results — entries are
/// still extracted but the custom view is dropped.
fn process_plugin_response(
    response: PluginResponse,
    source: &str,
    allow_custom_ui: bool,
) -> (Option<(ViewKind, PluginViewRef)>, Vec<SourcedEntry>) {
    let mut view_ref = None;

    match &response {
        PluginResponse::CustomUI { view, data, .. } if allow_custom_ui => {
            view_ref = Some((
                ViewKind::Custom,
                PluginViewRef {
                    plugin_id: source.to_string(),
                    view: view.clone(),
                    data: data.clone(),
                },
            ));
        }
        PluginResponse::CustomUI { .. } => {
            // CustomUI is only honoured in prefix mode. In non-prefix
            // (always-on) mode we downgrade to plain results so the
            // plugin's entries still appear but without the custom view.
            eprintln!(
                "search: dropping CustomUI from plugin '{}' — \
                 CustomUI is only supported in prefix mode",
                source
            );
        }
        PluginResponse::InlineUI { view, data, .. } => {
            view_ref = Some((
                ViewKind::Inline,
                PluginViewRef {
                    plugin_id: source.to_string(),
                    view: view.clone(),
                    data: data.clone(),
                },
            ));
        }
        PluginResponse::Results(_) => {}
    }

    let entries: Vec<SourcedEntry> = match response {
        PluginResponse::Results(results) => results,
        PluginResponse::CustomUI { results, .. } | PluginResponse::InlineUI { results, .. } => {
            results
        }
    }
    .into_iter()
    .map(|r| SourcedEntry::new(source.to_string(), r))
    .collect();

    (view_ref, entries)
}

/// Find the plugin whose registered prefix is the longest
/// match for `query`. Returns `None` when no prefix matches.
/// Disabled plugins are skipped.
fn find_prefix_match<'a>(
    slots: &'a [PluginSlot],
    query: &str,
) -> Option<(&'a Arc<dyn Plugin>, &'a str)> {
    let mut best: Option<(&Arc<dyn Plugin>, &str)> = None;
    let mut best_len = 0;

    for slot in slots {
        if !slot.is_active() {
            continue;
        }
        for prefix in slot.plugin.search_prefixes() {
            if prefix.len() > best_len && query.starts_with(prefix.as_str()) {
                best = Some((&slot.plugin, prefix.as_str()));
                best_len = prefix.len();
            }
        }
    }

    best
}

// =========================================================
// Helpers
// =========================================================

/// Show the launcher and emit `activate-plugin-custom-ui` so the
/// frontend switches to the plugin's view.
fn show_launcher_with_plugin(
    app: &tauri::AppHandle,
    plugin_id: &str,
    view: &str,
    data: Option<serde_json::Value>,
) {
    let Some(layout) = app.state::<crate::LauncherLayoutState>().get().copied() else {
        eprintln!("shortcut: launcher layout not yet received, ignoring");
        return;
    };
    crate::position_launcher_on_cursor_monitor(app, &layout);

    if let Err(e) = PlatformLauncherPanel::show(app) {
        eprintln!("shortcut: failed to show launcher: {e:#}");
        return;
    }

    if let Err(e) = app.emit(
        "activate-plugin-custom-ui",
        ActivatePluginPayload {
            plugin_id: plugin_id.to_string(),
            view: view.to_string(),
            data,
        },
    ) {
        eprintln!("shortcut: failed to emit activate-plugin-custom-ui: {e:#}");
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::types::CatalogEntry;

    // -------------------------------------------------------
    // Mock Plugin
    //
    // Configurable stub implementing `Plugin` for unit tests.
    // Each field controls a specific trait method's return
    // value. Defaults produce an empty, enabled, prefix-free
    // plugin.
    // -------------------------------------------------------

    struct MockPlugin {
        id: String,
        enabled: bool,
        prefixes: Vec<String>,
        catalog_entries: Vec<CatalogEntry>,
        search_response: Option<PluginResponse>,
    }

    impl MockPlugin {
        fn new(id: &str) -> Self {
            Self {
                id: id.to_string(),
                enabled: true,
                prefixes: vec![],
                catalog_entries: vec![],
                search_response: None,
            }
        }

        fn with_prefixes(mut self, prefixes: &[&str]) -> Self {
            self.prefixes = prefixes.iter().map(|s| s.to_string()).collect();
            self
        }

        fn with_enabled(mut self, enabled: bool) -> Self {
            self.enabled = enabled;
            self
        }

        fn with_search_response(mut self, response: PluginResponse) -> Self {
            self.search_response = Some(response);
            self
        }

    }

    impl Plugin for MockPlugin {
        fn id(&self) -> &str {
            &self.id
        }

        fn search_prefixes(&self) -> &[String] {
            &self.prefixes
        }

        fn entries(&self) -> Vec<CatalogEntry> {
            self.catalog_entries.clone()
        }

        fn search(
            &self,
            _query: &str,
            _matched_prefix: Option<&str>,
        ) -> Option<PluginResponse> {
            self.search_response.clone()
        }

        fn execute(
            &self,
            _entry_id: &str,
            _action_id: &ActionId,
            _app: &tauri::AppHandle,
        ) -> anyhow::Result<PostAction> {
            Ok(PostAction::Nothing)
        }
    }

    /// Helper to build a `ScoredEntry` with minimal boilerplate.
    fn scored_entry(id: &str, score: u32) -> ScoredEntry {
        ScoredEntry {
            id: id.to_string(),
            title: id.to_string(),
            subtitle: None,
            icon: None,
            score,
            title_positions: Utf16Positions::empty(),
            subtitle_positions: Utf16Positions::empty(),
            actions: vec![],
        }
    }

    /// Helper to wrap mock plugins in `PluginSlot`.
    fn plugin_slots(plugins: Vec<MockPlugin>) -> Vec<PluginSlot> {
        plugins
            .into_iter()
            .map(|p| {
                let enabled = p.enabled;
                let slot = PluginSlot::new(Arc::new(p));
                slot.enabled.store(enabled, Ordering::Relaxed);
                slot
            })
            .collect()
    }

    // =======================================================
    // Plugin::search() contract tests
    // =======================================================

    #[test]
    fn default_search_returns_none() {
        let plugin = MockPlugin::new("empty");
        assert!(plugin.search("anything", None).is_none());
    }

    #[test]
    fn search_returns_configured_response() {
        let plugin = MockPlugin::new("test")
            .with_search_response(PluginResponse::Results(vec![scored_entry("r1", 100)]));

        let result = plugin.search("query", None);
        assert!(result.is_some());

        match result.unwrap() {
            PluginResponse::Results(entries) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].id, "r1");
            }
            _ => panic!("expected Results variant"),
        }
    }

    #[test]
    fn search_returns_custom_ui() {
        let plugin = MockPlugin::new("test").with_search_response(PluginResponse::CustomUI {
            view: "history".into(),
            data: Some(serde_json::json!({"key": "value"})),
            results: vec![scored_entry("h1", 50)],
        });

        let result = plugin.search("=2+2", Some("="));
        match result.unwrap() {
            PluginResponse::CustomUI { view, data, results } => {
                assert_eq!(view, "history");
                assert!(data.is_some());
                assert_eq!(results.len(), 1);
            }
            _ => panic!("expected CustomUI variant"),
        }
    }

    #[test]
    fn search_returns_inline_ui() {
        let plugin = MockPlugin::new("test").with_search_response(PluginResponse::InlineUI {
            view: "result".into(),
            data: None,
            results: vec![],
        });

        let result = plugin.search("42", None);
        match result.unwrap() {
            PluginResponse::InlineUI { view, .. } => {
                assert_eq!(view, "result");
            }
            _ => panic!("expected InlineUI variant"),
        }
    }

    // =======================================================
    // search_prefixes() contract tests
    // =======================================================

    #[test]
    fn default_prefixes_are_empty() {
        let plugin = MockPlugin::new("no-prefix");
        assert!(plugin.search_prefixes().is_empty());
    }

    #[test]
    fn configured_prefixes_returned() {
        let plugin = MockPlugin::new("calc").with_prefixes(&["=", "calc "]);
        let prefixes = plugin.search_prefixes();
        assert_eq!(prefixes.len(), 2);
        assert_eq!(prefixes[0], "=");
        assert_eq!(prefixes[1], "calc ");
    }

    // =======================================================
    // find_prefix_match() tests
    // =======================================================

    #[test]
    fn no_plugins_no_match() {
        let plugins = plugin_slots(vec![]);
        assert!(find_prefix_match(&plugins, "=2+2").is_none());
    }

    #[test]
    fn no_prefix_plugins_no_match() {
        let plugins = plugin_slots(vec![
            MockPlugin::new("a"),
            MockPlugin::new("b"),
        ]);
        assert!(find_prefix_match(&plugins, "hello").is_none());
    }

    #[test]
    fn single_prefix_match() {
        let plugins = plugin_slots(vec![
            MockPlugin::new("calc").with_prefixes(&["="]),
        ]);
        let (plugin, prefix) = find_prefix_match(&plugins, "=2+2").unwrap();
        assert_eq!(plugin.id(), "calc");
        assert_eq!(prefix, "=");
    }

    #[test]
    fn longest_prefix_wins() {
        let plugins = plugin_slots(vec![
            MockPlugin::new("short").with_prefixes(&["!"]),
            MockPlugin::new("long").with_prefixes(&["!g"]),
        ]);

        // "!google" matches both "!" and "!g" — longest wins.
        let (plugin, prefix) = find_prefix_match(&plugins, "!google").unwrap();
        assert_eq!(plugin.id(), "long");
        assert_eq!(prefix, "!g");
    }

    #[test]
    fn prefix_must_be_at_start() {
        let plugins = plugin_slots(vec![
            MockPlugin::new("calc").with_prefixes(&["="]),
        ]);
        // "hello =" doesn't start with "=".
        assert!(find_prefix_match(&plugins, "hello =").is_none());
    }

    #[test]
    fn disabled_plugin_prefix_skipped() {
        let plugins = plugin_slots(vec![
            MockPlugin::new("calc")
                .with_prefixes(&["="])
                .with_enabled(false),
        ]);
        assert!(find_prefix_match(&plugins, "=2+2").is_none());
    }

    #[test]
    fn disabled_plugin_skipped_fallback_to_shorter() {
        let plugins = plugin_slots(vec![
            MockPlugin::new("disabled-long")
                .with_prefixes(&["!g"])
                .with_enabled(false),
            MockPlugin::new("enabled-short")
                .with_prefixes(&["!"]),
        ]);

        let (plugin, prefix) = find_prefix_match(&plugins, "!google").unwrap();
        assert_eq!(plugin.id(), "enabled-short");
        assert_eq!(prefix, "!");
    }

    #[test]
    fn multi_char_prefix() {
        let plugins = plugin_slots(vec![
            MockPlugin::new("emoji").with_prefixes(&[":"]),
            MockPlugin::new("http").with_prefixes(&["http://", "https://"]),
        ]);

        let (plugin, prefix) = find_prefix_match(&plugins, "https://example.com").unwrap();
        assert_eq!(plugin.id(), "http");
        assert_eq!(prefix, "https://");
    }

    #[test]
    fn exact_prefix_query() {
        // Query is exactly the prefix with nothing after it.
        let plugins = plugin_slots(vec![
            MockPlugin::new("emoji").with_prefixes(&[":"]),
        ]);
        let (plugin, prefix) = find_prefix_match(&plugins, ":").unwrap();
        assert_eq!(plugin.id(), "emoji");
        assert_eq!(prefix, ":");
    }

    #[test]
    fn multiple_prefixes_same_plugin() {
        let plugins = plugin_slots(vec![
            MockPlugin::new("multi").with_prefixes(&["http://", "https://"]),
        ]);

        let (_, prefix) = find_prefix_match(&plugins, "http://foo.com").unwrap();
        assert_eq!(prefix, "http://");

        let (_, prefix) = find_prefix_match(&plugins, "https://foo.com").unwrap();
        assert_eq!(prefix, "https://");
    }

    // =======================================================
    // process_plugin_response() tests
    // =======================================================

    #[test]
    fn results_response_extracts_entries() {
        let response = PluginResponse::Results(vec![
            scored_entry("a", 100),
            scored_entry("b", 50),
        ]);
        let (view, entries) = process_plugin_response(response, "test-plugin", false);
        assert!(view.is_none());
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].inner.id, "a");
        assert_eq!(entries[1].inner.id, "b");
        assert_eq!(entries[0].source, "test-plugin");
    }

    #[test]
    fn empty_results_yields_empty_entries() {
        let response = PluginResponse::Results(vec![]);
        let (view, entries) = process_plugin_response(response, "p", false);
        assert!(view.is_none());
        assert!(entries.is_empty());
    }

    #[test]
    fn custom_ui_allowed_in_prefix_mode() {
        let response = PluginResponse::CustomUI {
            view: "history".into(),
            data: Some(serde_json::json!({"x": 1})),
            results: vec![scored_entry("h1", 10)],
        };
        let (view, entries) = process_plugin_response(response, "calc", true);
        let (kind, vr) = view.unwrap();
        assert!(matches!(kind, ViewKind::Custom));
        assert_eq!(vr.plugin_id, "calc");
        assert_eq!(vr.view, "history");
        assert!(vr.data.is_some());
        // Entries are still extracted alongside the view.
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn custom_ui_downgraded_outside_prefix_mode() {
        let response = PluginResponse::CustomUI {
            view: "picker".into(),
            data: None,
            results: vec![scored_entry("e1", 20), scored_entry("e2", 10)],
        };
        let (view, entries) = process_plugin_response(response, "emoji", false);
        // View is dropped (not allowed outside prefix mode).
        assert!(view.is_none());
        // Entries are still extracted from the CustomUI response.
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn inline_ui_produces_view_ref() {
        let response = PluginResponse::InlineUI {
            view: "result".into(),
            data: Some(serde_json::json!({"result": "42"})),
            results: vec![],
        };
        let (view, entries) = process_plugin_response(response, "calc", false);
        let (kind, vr) = view.unwrap();
        assert!(matches!(kind, ViewKind::Inline));
        assert_eq!(vr.view, "result");
        assert!(entries.is_empty());
    }

    #[test]
    fn inline_ui_allowed_in_both_modes() {
        // InlineUI should work regardless of prefix/non-prefix mode.
        for allow_custom in [true, false] {
            let response = PluginResponse::InlineUI {
                view: "v".into(),
                data: None,
                results: vec![],
            };
            let (view, _) = process_plugin_response(response, "p", allow_custom);
            assert!(view.is_some(), "InlineUI should produce view ref with allow_custom={allow_custom}");
        }
    }

    #[test]
    fn custom_ui_with_no_results_prefix_mode() {
        let response = PluginResponse::CustomUI {
            view: "picker".into(),
            data: None,
            results: vec![],
        };
        let (view, entries) = process_plugin_response(response, "emoji", true);
        assert!(view.is_some());
        assert!(entries.is_empty());
    }

    #[test]
    fn source_id_propagated_to_entries() {
        let response = PluginResponse::Results(vec![
            scored_entry("x", 1),
        ]);
        let (_, entries) = process_plugin_response(response, "my-plugin", false);
        assert_eq!(entries[0].source, "my-plugin");
    }
}
