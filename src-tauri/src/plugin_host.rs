// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Plugin Host
//
// Central authority for the plugin lifecycle. Owns all plugin
// Arc references and is the single entry point for:
//
// - Registration (register / register_query)
// - Settings initialization (Phase 1: synchronous defaults)
// - Parallel plugin setup (Phase 2: rayon background pool)
// - Global shortcut registration and reactive re-registration
// - Search routing (nucleo + prefix matching)
// - Action execution and message routing
// - Teardown
//
// Replaces the former CatalogRegistry + ShortcutManager split.
// Managed as `Arc<PluginHost>` in Tauri state — no Mutex needed
// since all fields are either immutable after init or use
// interior mutability (AtomicBool, watch channels).
// =========================================================

use std::collections::HashSet;
use std::sync::Arc;
use std::thread;

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use rayon::prelude::*;
use tauri::Emitter;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_store::Store;
use tokio::sync::mpsc;

use crate::platform::{LauncherPanel as _, PlatformLauncherPanel};
use crate::plugins::{CatalogPlugin, PluginContext, PluginShortcut, QueryPlugin};
use crate::search::types::{ActionId, PostAction, ScoredEntry, SearchResult};
use crate::settings::{PluginSettings, SettingsInit};
use crate::settings_notifier::{PluginSettingsNotifier, SettingsNotifier};

// =========================================================
// Shortcut Types
// =========================================================

/// A registered shortcut with enough context to route the
/// activation back to the owning plugin.
enum ShortcutOwner {
    Catalog(Arc<dyn CatalogPlugin>),
    Query(Arc<dyn QueryPlugin>),
}

struct RegisteredShortcut {
    shortcut: Shortcut,
    plugin_id: String,
    shortcut_id: String,
    owner: ShortcutOwner,
}

/// Payload emitted with the `activate-plugin-custom-ui` event.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivatePluginPayload {
    plugin_id: String,
}

// =========================================================
// PluginHost
// =========================================================

pub struct PluginHost {
    catalog_plugins: Vec<Arc<dyn CatalogPlugin>>,
    query_plugins: Vec<Arc<dyn QueryPlugin>>,
    store: Arc<Store<tauri::Wry>>,
    notifier: Arc<SettingsNotifier>,

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
    pub fn new(store: Arc<Store<tauri::Wry>>, notifier: Arc<SettingsNotifier>) -> Self {
        let (tx, rx) = mpsc::channel(1);
        Self {
            catalog_plugins: Vec::new(),
            query_plugins: Vec::new(),
            store,
            notifier,
            watched_keys: HashSet::new(),
            shortcut_signal_tx: tx,
            shortcut_signal_rx: std::sync::Mutex::new(Some(rx)),
        }
    }

    pub fn register(&mut self, plugin: Box<dyn CatalogPlugin>) {
        self.catalog_plugins.push(Arc::from(plugin));
    }

    pub fn register_query(&mut self, plugin: Box<dyn QueryPlugin>) {
        self.query_plugins.push(Arc::from(plugin));
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
        for p in &self.catalog_plugins {
            let prefix = format!("plugins.{}.", p.id());
            let current = SettingsInit::from_store(&self.store, &prefix);
            let initialized = p.initialize_settings(current);
            initialized.apply(&self.store, &prefix);
        }
        for p in &self.query_plugins {
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
        for p in &self.catalog_plugins {
            collect_watched_keys_into(
                p.id(),
                p.enabled_settings_key(),
                &p.shortcuts(),
                &mut keys_to_watch,
            );
        }
        for p in &self.query_plugins {
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
        // -------------------------------------------------------
        let mut setup_fns: Vec<Box<dyn FnOnce() + Send>> = Vec::new();

        let handle = app.clone();
        for p in &self.catalog_plugins {
            let p = Arc::clone(p);
            let h = handle.clone();
            let ctx = PluginContext {
                settings: PluginSettings::new(Arc::clone(&self.store), p.id()),
                notifier: PluginSettingsNotifier::new(
                    Arc::clone(&self.notifier),
                    Arc::clone(&self.store),
                    p.id(),
                ),
            };
            setup_fns.push(Box::new(move || p.setup(&h, &ctx)));
        }
        for p in &self.query_plugins {
            let p = Arc::clone(p);
            let h = handle.clone();
            let ctx = PluginContext {
                settings: PluginSettings::new(Arc::clone(&self.store), p.id()),
                notifier: PluginSettingsNotifier::new(
                    Arc::clone(&self.notifier),
                    Arc::clone(&self.store),
                    p.id(),
                ),
            };
            setup_fns.push(Box::new(move || p.setup(&h, &ctx)));
        }

        thread::spawn(move || {
            let cores = thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4);
            let num_threads = (cores * 7 / 10).max(1);

            let pool = rayon::ThreadPoolBuilder::new()
                .num_threads(num_threads)
                .build()
                .expect("rayon setup thread pool");

            pool.install(|| {
                setup_fns.into_par_iter().for_each(|f| f());
            });
        });
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

        // Collect shortcuts from enabled catalog plugins.
        for plugin in &self.catalog_plugins {
            if !plugin.is_enabled() {
                continue;
            }
            let plugin_id = plugin.id().to_string();
            for decl in plugin.shortcuts() {
                if let Some(r) = self.resolve_shortcut(
                    &plugin_id,
                    &decl,
                    ShortcutOwner::Catalog(Arc::clone(plugin)),
                ) {
                    registered.push(r);
                }
            }
        }

        // Collect shortcuts from enabled query plugins.
        for plugin in &self.query_plugins {
            if !plugin.is_enabled() {
                continue;
            }
            let plugin_id = plugin.id().to_string();
            for decl in plugin.shortcuts() {
                if let Some(r) = self.resolve_shortcut(
                    &plugin_id,
                    &decl,
                    ShortcutOwner::Query(Arc::clone(plugin)),
                ) {
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

                    let result = match &r.owner {
                        ShortcutOwner::Catalog(p) => p.handle_shortcut(&r.shortcut_id, &handle),
                        ShortcutOwner::Query(p) => p.handle_shortcut(&r.shortcut_id, &handle),
                    };

                    match result {
                        Ok(PostAction::ShowCustomUI) => {
                            show_launcher_with_plugin(&handle, &r.plugin_id);
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
        owner: ShortcutOwner,
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

    /// Search all plugins against the given query.
    pub fn search(&self, query: &str) -> SearchResult {
        if query.is_empty() {
            return SearchResult::empty();
        }

        // Prefix routing: longest match wins.
        if let Some((plugin, prefix)) = self.find_prefix_match(query) {
            let stripped = &query[prefix.len()..];
            let source = plugin.id().to_string();
            let response = plugin.search(stripped, Some(prefix));

            let custom_plugin_view = if response.is_custom_ui() {
                Some(source.clone())
            } else {
                None
            };

            let entries = response
                .into_results()
                .into_iter()
                .map(|r| r.into_scored_entry(source.clone()))
                .collect();

            return SearchResult {
                entries,
                custom_plugin_view,
                matched_prefix: Some(prefix.to_string()),
            };
        }

        // No prefix: nucleo over catalog entries + always-on query plugins.
        let mut results = self.search_catalogs(query);

        for plugin in &self.query_plugins {
            if !plugin.is_enabled() || !plugin.prefixes().is_empty() {
                continue;
            }
            let source = plugin.id().to_string();
            for result in plugin.search(query, None).into_results() {
                results.push(result.into_scored_entry(source.clone()));
            }
        }

        results.sort_by(|a, b| b.score.cmp(&a.score));
        SearchResult {
            entries: results,
            custom_plugin_view: None,
            matched_prefix: None,
        }
    }

    fn find_prefix_match<'a>(&'a self, query: &str) -> Option<(&'a Arc<dyn QueryPlugin>, &'a str)> {
        let mut best: Option<(&Arc<dyn QueryPlugin>, &str)> = None;
        let mut best_len = 0;

        for plugin in &self.query_plugins {
            if !plugin.is_enabled() {
                continue;
            }
            for &prefix in plugin.prefixes() {
                if prefix.len() > best_len && query.starts_with(prefix) {
                    best = Some((plugin, prefix));
                    best_len = prefix.len();
                }
            }
        }

        best
    }

    fn search_catalogs(&self, query: &str) -> Vec<ScoredEntry> {
        let mut matcher = Matcher::new(Config::DEFAULT);
        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);

        let mut results = Vec::new();
        let mut char_buf = Vec::new();
        let mut title_indices = Vec::new();

        for plugin in &self.catalog_plugins {
            // Skip disabled plugins — the host gates search results.
            if !plugin.is_enabled() {
                continue;
            }

            let source = plugin.id().to_string();

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

                    results.push(ScoredEntry {
                        id: entry.id,
                        title: entry.title,
                        subtitle: entry.subtitle,
                        icon: entry.icon,
                        score,
                        title_positions: title_indices.clone(),
                        subtitle_positions: vec![],
                        source: source.clone(),
                        actions: entry.actions,
                    });
                }
            }
        }

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
        if let Some(plugin) = self.catalog_plugins.iter().find(|p| p.id() == source) {
            return plugin.execute(entry_id, action_id, app);
        }
        if let Some(plugin) = self.query_plugins.iter().find(|p| p.id() == source) {
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
        if let Some(plugin) = self.catalog_plugins.iter().find(|p| p.id() == source) {
            return plugin.handle_message(method, payload, channel);
        }
        if let Some(plugin) = self.query_plugins.iter().find(|p| p.id() == source) {
            return plugin.handle_message(method, payload, channel);
        }
        anyhow::bail!("unknown plugin source: {source}");
    }

    // =========================================================
    // Teardown
    // =========================================================

    pub fn teardown_all(&self) {
        for p in &self.catalog_plugins {
            p.teardown();
        }
        for p in &self.query_plugins {
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
fn show_launcher_with_plugin(app: &tauri::AppHandle, plugin_id: &str) {
    crate::position_launcher_on_cursor_monitor(app);

    if let Err(e) = PlatformLauncherPanel::show(app) {
        eprintln!("shortcut: failed to show launcher: {e:#}");
        return;
    }

    if let Err(e) = app.emit(
        "activate-plugin-custom-ui",
        ActivatePluginPayload {
            plugin_id: plugin_id.to_string(),
        },
    ) {
        eprintln!("shortcut: failed to emit activate-plugin-custom-ui: {e:#}");
    }
}
