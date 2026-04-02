// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Plugin Host
//
// Central authority for the plugin lifecycle. Owns all plugin
// Arc references and is the single entry point for:
//
// - Registration (register)
// - Settings initialization (Phase 1: synchronous defaults)
// - Parallel plugin setup (Phase 2: tokio spawn_blocking)
// - Global shortcut registration and reactive re-registration
// - Search routing (nucleo + prefix matching)
// - Action execution and message routing
// - Teardown
//
// Managed as `Arc<PluginHost>` in Tauri state — no Mutex needed
// since all fields are either immutable after init or use
// interior mutability (AtomicBool, watch channels).
// =========================================================

use std::collections::HashSet;
use std::sync::Arc;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_store::Store;
use tokio::sync::mpsc;

use crate::frecency::{FrecencyStore, PluginFrecency};
use crate::platform::{LauncherPanel as _, PlatformLauncherPanel};
use crate::plugins::{Plugin, PluginContext, PluginShortcut};
use crate::search::types::{
    ActionId, CancellationToken, PluginResponse, PluginViewRef, PostAction, ResultChannel,
    ScoredEntry, SearchMessage, SourcedEntry,
};
use crate::settings::{PluginSettings, SettingsInit};
use crate::settings_notifier::{PluginSettingsNotifier, SettingsNotifier};
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
// PluginHost
// =========================================================

pub struct PluginHost {
    plugins: Vec<Arc<dyn Plugin>>,
    store: Arc<Store<tauri::Wry>>,
    notifier: Arc<SettingsNotifier>,
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
        notifier: Arc<SettingsNotifier>,
        frecency: Arc<FrecencyStore>,
    ) -> Self {
        let (tx, rx) = mpsc::channel(1);
        Self {
            plugins: Vec::new(),
            store,
            notifier,
            frecency,
            watched_keys: HashSet::new(),
            shortcut_signal_tx: tx,
            shortcut_signal_rx: std::sync::Mutex::new(Some(rx)),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        self.plugins.push(Arc::from(plugin));
    }

    // =========================================================
    // Initialization
    // =========================================================

    /// Initialize settings, register shortcuts, start plugin setup,
    /// and spawn the shortcut reactor.
    ///
    /// Must be called exactly once after all plugins are registered
    /// and before `app.manage()` stores the host.
    pub fn initialize_and_start(&mut self, app: &tauri::AppHandle) {
        // -------------------------------------------------------
        // Phase 1: Initialize plugin settings defaults (synchronous)
        // -------------------------------------------------------
        for p in &self.plugins {
            let prefix = format!("plugins.{}.", p.id());
            let current = SettingsInit::from_store(&self.store, &prefix);
            let initialized = p.initialize_settings(current);
            initialized.apply(&self.store, &prefix);
        }

        // -------------------------------------------------------
        // Collect watched keys for reactive re-registration
        // -------------------------------------------------------
        self.watched_keys.insert("globalShortcut".to_string());

        // Collect keys into a temp vec to avoid borrowing &self and
        // &mut self.watched_keys simultaneously.
        let mut keys_to_watch = Vec::new();
        for p in &self.plugins {
            collect_watched_keys_into(
                p.id(),
                p.enabled_settings_key(),
                &p.shortcuts(),
                &mut keys_to_watch,
            );
        }
        self.watched_keys.extend(keys_to_watch);

        // -------------------------------------------------------
        // Register initial shortcuts
        // -------------------------------------------------------
        self.register_all_shortcuts(app);

        // -------------------------------------------------------
        // Phase 2: Parallel plugin setup (background)
        //
        // Each plugin's setup() runs on a Tokio spawn_blocking
        // thread. This gives plugins access to the Tokio runtime
        // (e.g. for Http requests via block_on) while keeping
        // setup parallelism.
        // -------------------------------------------------------
        let handle = app.clone();
        for p in &self.plugins {
            let p = Arc::clone(p);
            let h = handle.clone();
            let ctx = PluginContext {
                settings: PluginSettings::new(Arc::clone(&self.store), p.id()),
                notifier: PluginSettingsNotifier::new(
                    Arc::clone(&self.notifier),
                    Arc::clone(&self.store),
                    p.id(),
                ),
                frecency: PluginFrecency::new(Arc::clone(&self.frecency), p.id()),
            };
            tokio::task::spawn_blocking(move || p.setup(&h, &ctx));
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
        for plugin in &self.plugins {
            if !plugin.is_enabled() {
                continue;
            }
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

        let cancel = CancellationToken::new();

        // =======================================================
        // Prefix routing: longest match wins. Exclusive — only
        // the matched plugin runs, no catalogs, no streaming.
        // =======================================================

        if let Some((plugin, prefix)) = self.find_prefix_match(query) {
            let stripped = query[prefix.len()..].to_string();
            let source = plugin.id().to_string();
            let prefix_owned = prefix.to_string();
            let plugin = Arc::clone(plugin);
            let cancel = cancel.clone();

            let (tx, mut rx) = mpsc::channel::<(String, PluginResponse)>(4);
            let rc = ResultChannel::new(source.clone(), tx);

            let prefix_for_search = prefix_owned.clone();
            tokio::task::spawn_blocking(move || {
                plugin.search(&stripped, Some(&prefix_for_search), &rc, &cancel);
            });

            // The prefix path produces at most one response.
            // Translate it into a single SearchResults message.
            let mut custom_plugin_view = None;
            let mut inline_plugin_view = None;
            let mut entries = Vec::new();

            while let Some((source, response)) = rx.recv().await {
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
        let plugins = self.plugins.clone();
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

        // Phase 2: query plugins — spawn concurrently, stream
        // results as they arrive through a single shared channel.
        // Each plugin gets a `ResultChannel` bound to its source ID
        // that sends into a shared `mpsc` sender. The host awaits
        // on the single receiver — no spin-polling needed.
        let (tx, mut rx) = mpsc::channel::<(String, PluginResponse)>(4);

        for plugin in &self.plugins {
            if !plugin.is_enabled() {
                continue;
            }

            let source = plugin.id().to_string();
            let plugin = Arc::clone(plugin);
            let query = query_owned.clone();
            let cancel = cancel.clone();

            let rc = ResultChannel::new(source, tx.clone());

            tokio::task::spawn_blocking(move || {
                plugin.search(&query, None, &rc, &cancel);
            });
        }

        // Drop the original sender so `rx` closes once all plugin
        // tasks (each holding a cloned sender via ResultChannel)
        // have finished and dropped their senders.
        drop(tx);

        // Receive results as they arrive from any plugin. The
        // channel closes naturally when all senders are dropped.
        let mut inline_claimed = false;

        while let Some((source, response)) = rx.recv().await {
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

    /// Process a `PluginResponse` into scored entries and an
    /// optional view reference. Factored out to avoid duplication
    /// between prefix and non-prefix paths.
    fn process_plugin_response(
        &self,
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

    fn find_prefix_match<'a>(&'a self, query: &str) -> Option<(&'a Arc<dyn Plugin>, &'a str)> {
        let mut best: Option<(&Arc<dyn Plugin>, &str)> = None;
        let mut best_len = 0;

        for plugin in &self.plugins {
            if !plugin.is_enabled() {
                continue;
            }
            for &prefix in plugin.search_prefixes() {
                if prefix.len() > best_len && query.starts_with(prefix) {
                    best = Some((plugin, prefix));
                    best_len = prefix.len();
                }
            }
        }

        best
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
            // Skip disabled plugins — the host gates search results.
            if !plugin.is_enabled() {
                continue;
            }

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

        if let Some(plugin) = self.plugins.iter().find(|p| p.id() == source) {
            return plugin.execute(entry_id, action_id, app);
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
        if let Some(plugin) = self.plugins.iter().find(|p| p.id() == source) {
            return plugin.handle_message(method, payload, channel);
        }
        anyhow::bail!("unknown plugin source: {source}");
    }

    // =========================================================
    // Teardown
    // =========================================================

    pub fn teardown_all(&self) {
        for p in &self.plugins {
            p.teardown();
        }
    }
}

// =========================================================
// Helpers
// =========================================================

/// Collect settings keys that the host should watch for a single
/// plugin (enabled key + shortcut keys).
fn collect_watched_keys_into(
    plugin_id: &str,
    enabled_key: Option<&str>,
    shortcuts: &[PluginShortcut],
    out: &mut Vec<String>,
) {
    if let Some(key) = enabled_key {
        out.push(format!("plugins.{plugin_id}.{key}"));
    }
    for s in shortcuts {
        out.push(format!("plugins.{plugin_id}.{}", s.settings_key));
    }
}

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
