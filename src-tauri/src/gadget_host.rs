// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Gadget Host
//
// Central authority for the gadget lifecycle. Owns all gadget
// references and is the single entry point for:
//
// - Registration (register)
// - Settings initialization (Phase 1: synchronous defaults)
// - Parallel gadget enable (Phase 2: tokio spawn_blocking)
// - Host-managed enable/disable via `enabled.<id>` keys
// - Settings change dispatch via CoalescingDispatcher
// - Global shortcut registration and reactive re-registration
// - Search routing (nucleo + prefix matching)
// - Action execution and message routing
// - Shutdown (disable all gadgets)
//
// Managed as `Arc<GadgetHost>` in Tauri state — no Mutex needed
// since all fields are either immutable after init or use
// interior mutability (AtomicBool, watch channels).
// =========================================================

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use tauri::{Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_store::Store;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::commands::types::{
    ActionId, GadgetResponse, GadgetViewRef, PostAction, ResultSource, ScoredEntry, SearchMessage,
    SourcedEntry,
};
use crate::entry_store::EntryStore;
use crate::frecency::FrecencyStore;
use crate::gadgets::{Gadget, GadgetShortcut};
use crate::icons::IconCache;
use crate::network::website_metadata::WebsiteMetadataService;
use crate::settings::SettingsInit;
use crate::settings::coalescing_dispatcher::CoalescingDispatcher;
use crate::unicode::Utf16Positions;
use crate::wasm::source::GadgetSourceKind;

// =========================================================
// ProvisioningContext — host-internal
// =========================================================

/// Bundled runtime context for building gadget capabilities.
///
/// Host-internal — gadgets never see this struct. Constructed
/// once in `lib.rs::setup` and passed to `register_with_caps`
/// and `initialize_and_start`.
pub(crate) struct ProvisioningContext {
    pub app: tauri::AppHandle,
    pub store: Arc<Store<tauri::Wry>>,
    pub frecency: Arc<FrecencyStore>,
    pub icon_cache: Arc<IconCache>,
    pub metadata_service: Arc<WebsiteMetadataService>,
}

impl Clone for ProvisioningContext {
    fn clone(&self) -> Self {
        Self {
            app: self.app.clone(),
            store: Arc::clone(&self.store),
            frecency: Arc::clone(&self.frecency),
            icon_cache: Arc::clone(&self.icon_cache),
            metadata_service: Arc::clone(&self.metadata_service),
        }
    }
}

// =========================================================
// Internal Helpers
// =========================================================

/// Distinguishes CustomUI from InlineUI in `process_gadget_response`.
enum ViewKind {
    Custom,
    Inline,
}

// =========================================================
// Shortcut Types
// =========================================================

/// A registered shortcut with enough context to route the
/// activation back to the owning gadget.
struct RegisteredShortcut {
    shortcut: Shortcut,
    gadget_id: String,
    shortcut_id: String,
    owner: Arc<dyn Gadget>,
}

/// Payload emitted with the `activate-gadget-custom-ui` event.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivateGadgetPayload {
    gadget_id: String,
    view: String,
    data: Option<serde_json::Value>,
}

// =========================================================
// GadgetSlot — per-gadget state managed by the host
// =========================================================

/// Wraps a gadget with host-managed lifecycle state. The host
/// owns the enabled flag and the settings dispatcher — gadgets
/// never manage their own enabled state.
struct GadgetSlot {
    gadget: Arc<dyn Gadget>,

    /// Where this gadget was loaded from. Surfaced to the
    /// frontend so the Gadgets settings panel can badge each
    /// entry and gate uninstall to `User` only.
    source_kind: GadgetSourceKind,

    /// Host-owned enabled flag. Checked before including the
    /// gadget in search results, shortcut registration, etc.
    /// Updated by the host when `enabled.<id>` changes in the
    /// settings store. Shared via Arc so `enable()` tasks can
    /// self-disable on failure without blocking the host.
    enabled: Arc<AtomicBool>,

    /// Serializes and deduplicates settings change dispatch for
    /// this gadget. Both `enabled.<id>` changes and
    /// `gadgets.<id>.*` changes are funneled through here.
    dispatcher: CoalescingDispatcher,
}

impl GadgetSlot {
    fn new(gadget: Arc<dyn Gadget>, source_kind: GadgetSourceKind) -> Self {
        Self {
            gadget,
            source_kind,
            enabled: Arc::new(AtomicBool::new(true)),
            dispatcher: CoalescingDispatcher::new(),
        }
    }

    /// Whether this gadget is currently active. The host owns
    /// this flag — gadgets never manage their own enabled state.
    fn is_active(&self) -> bool {
        self.enabled.load(Ordering::Relaxed)
    }
}

// =========================================================
// GadgetHost
// =========================================================

pub struct GadgetHost {
    slots: Vec<GadgetSlot>,
    store: Arc<Store<tauri::Wry>>,
    frecency: Arc<FrecencyStore>,
    entry_store: EntryStore,

    watched_keys: HashSet<String>,
    shortcut_signal_tx: mpsc::Sender<()>,
    shortcut_signal_rx: std::sync::Mutex<Option<mpsc::Receiver<()>>>,
}

impl GadgetHost {
    pub fn new(store: Arc<Store<tauri::Wry>>, frecency: Arc<FrecencyStore>) -> Self {
        let (tx, rx) = mpsc::channel(1);
        Self {
            slots: Vec::new(),
            store,
            frecency,
            entry_store: EntryStore::new(),
            watched_keys: HashSet::new(),
            shortcut_signal_tx: tx,
            shortcut_signal_rx: std::sync::Mutex::new(Some(rx)),
        }
    }

    // =========================================================
    // Capability provisioning
    // =========================================================

    /// Build `ProvisionedCaps` from a list of capability requests.
    ///
    /// The host is the single authority: it reads the requests,
    /// constructs each cap from the provisioning context, and
    /// returns the bundle. Gadgets never build their own caps.
    ///
    /// `source_path` is the gadget's archive/source root —
    /// provided for WASM gadgets, `None` for native gadgets
    /// (native gadgets that request `PathResolver` get a
    /// `gadget_archive` pointing to an empty path).
    pub(crate) fn build_provisioned_caps(
        gadget_id: &str,
        requests: &[crate::caps::CapRequest],
        ctx: &ProvisioningContext,
        source_path: Option<&std::path::Path>,
    ) -> anyhow::Result<Arc<crate::caps::ProvisionedCaps>> {
        use anyhow::Context;
        use std::path::PathBuf;
        use tauri::Manager;

        use crate::caps::*;
        use crate::frecency::GadgetFrecency;
        use crate::paths::{GadgetPaths, PlatformPaths};
        use crate::settings::GadgetSettings;

        let mut caps = ProvisionedCaps {
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
            path_resolver: None,
        };

        // Build GadgetPaths if any request needs path resolution
        // (PathResolver, Filesystem, or Command all require it).
        let needs_paths = requests.iter().any(|r| {
            matches!(
                r,
                CapRequest::PathResolver
                    | CapRequest::Filesystem { .. }
                    | CapRequest::Command { .. }
            )
        });

        let gadget_paths = if needs_paths {
            let path_resolver = ctx.app.path();
            let home = path_resolver.home_dir().context("resolve home directory")?;
            let xdg_config = path_resolver
                .config_dir()
                .context("resolve config directory")?;
            let xdg_data = path_resolver.data_dir().context("resolve data directory")?;

            let app_data_dir = path_resolver
                .app_data_dir()
                .context("resolve app data directory")?;

            let gadget_data = app_data_dir.join("gadget-home").join(gadget_id);
            let gadget_archive = source_path.map(PathBuf::from).unwrap_or_default();

            Some(GadgetPaths {
                platform: Arc::new(PlatformPaths {
                    home,
                    xdg_config,
                    xdg_data,
                }),
                gadget_data,
                gadget_archive,
            })
        } else {
            None
        };

        for request in requests {
            match request {
                CapRequest::Opener { permissions } => {
                    caps.opener = Some(Arc::new(OpenerCap::from_app(
                        &ctx.app,
                        OpenerPermissions {
                            schemes: permissions.schemes.clone(),
                            open_path: permissions.open_path,
                            reveal_path: permissions.reveal_path,
                        },
                    )));
                }
                CapRequest::Http { permissions } => {
                    caps.http = Some(Arc::new(HttpCap::new(permissions.origins.clone())));
                }
                CapRequest::Filesystem { permissions } => {
                    let paths = gadget_paths
                        .as_ref()
                        .expect("GadgetPaths built when Filesystem requested");
                    let fs_cap = FilesystemCap::new(&permissions.read_patterns, paths)
                        .context("compile filesystem patterns")?;
                    caps.filesystem = Some(Arc::new(fs_cap));
                }
                CapRequest::Command { permissions } => {
                    let paths = gadget_paths
                        .as_ref()
                        .expect("GadgetPaths built when Command requested");
                    let cmd_cap =
                        CommandCap::new(&permissions.rules, paths, paths.gadget_data.clone())
                            .context("compile command rules")?;
                    caps.command = Some(Arc::new(cmd_cap));
                }
                CapRequest::SqlStorage { config } => {
                    let app_data_dir = ctx
                        .app
                        .path()
                        .app_data_dir()
                        .context("resolve app data directory for sql")?;
                    let db_path = app_data_dir
                        .join("gadget-home")
                        .join(gadget_id)
                        .join("sql")
                        .join("storage.sqlite3");
                    let migration_strs: Vec<&str> =
                        config.migrations.iter().map(String::as_str).collect();
                    let storage = crate::storage::SqlStorage::open(db_path, &migration_strs)
                        .context("open SQL storage")?;
                    caps.sql_storage = Some(Arc::new(SqlStorageCap::new(Arc::new(storage))));
                }
                CapRequest::Clipboard => {
                    let clipboard_handle = ctx.app.clone();
                    let clipboard_writer = Box::new(move |text: &str| {
                        use tauri_plugin_clipboard_manager::ClipboardExt;
                        clipboard_handle
                            .clipboard()
                            .write_text(text)
                            .map_err(|e| format!("write to clipboard: {e}"))
                    });
                    caps.clipboard = Some(Arc::new(ClipboardCap::new(clipboard_writer)));
                }
                CapRequest::WebsiteMetadata => {
                    caps.website_metadata = Some(Arc::new(WebsiteMetadataCap::new(Arc::clone(
                        &ctx.metadata_service,
                    ))));
                }
                CapRequest::IconCache => {
                    caps.icon_cache = Some(Arc::clone(&ctx.icon_cache));
                }
                CapRequest::Settings => {
                    caps.settings = Some(Arc::new(GadgetSettings::new(
                        Arc::clone(&ctx.store),
                        gadget_id,
                    )));
                }
                CapRequest::Frecency => {
                    caps.frecency = Some(Arc::new(GadgetFrecency::new(
                        Arc::clone(&ctx.frecency),
                        gadget_id,
                    )));
                }
                CapRequest::PathResolver => {
                    let paths = gadget_paths
                        .clone()
                        .expect("GadgetPaths built when PathResolver requested");
                    caps.path_resolver = Some(Arc::new(PathResolverCap::new(Arc::new(paths))));
                }
            }
        }

        Ok(Arc::new(caps))
    }

    /// Register a gadget via the factory pattern. The host builds
    /// `ProvisionedCaps` from the declared `requests`, then calls
    /// the `factory` closure with the caps to construct the gadget.
    /// The gadget receives caps as a plain field at construction.
    ///
    /// `source_path` is the gadget's archive/source root (for WASM
    /// gadgets); `None` for native gadgets.
    pub fn register_with_caps<G, F>(
        &mut self,
        gadget_id: &str,
        requests: Vec<crate::caps::CapRequest>,
        ctx: &ProvisioningContext,
        source_path: Option<&std::path::Path>,
        factory: F,
        source_kind: GadgetSourceKind,
    ) -> anyhow::Result<()>
    where
        G: Gadget + 'static,
        F: FnOnce(Arc<crate::caps::ProvisionedCaps>) -> G,
    {
        let caps = Self::build_provisioned_caps(gadget_id, &requests, ctx, source_path)?;
        let gadget = factory(caps);
        self.slots.push(GadgetSlot::new(
            Arc::new(gadget) as Arc<dyn Gadget>,
            source_kind,
        ));
        Ok(())
    }

    /// Snapshot of the gadget-id → source-kind mapping. Exposed
    /// via the `gadget_sources` Tauri command. The host's slot
    /// list is append-only after setup, so this snapshot is
    /// stable over the process lifetime.
    pub fn gadget_sources(&self) -> std::collections::HashMap<String, GadgetSourceKind> {
        self.slots
            .iter()
            .map(|slot| (slot.gadget.id().to_string(), slot.source_kind))
            .collect()
    }

    // =========================================================
    // Initialization
    // =========================================================

    /// Initialize settings, register shortcuts, enable gadgets,
    /// and spawn the shortcut reactor.
    ///
    /// Must be called exactly once after all gadgets are registered
    /// and before `app.manage()` stores the host.
    pub fn initialize_and_start(&mut self, ctx: ProvisioningContext) {
        // -------------------------------------------------------
        // Phase 1: Initialize gadget settings defaults (synchronous)
        // -------------------------------------------------------
        for slot in &self.slots {
            let id = slot.gadget.id();
            let prefix = format!("gadgets.{id}.");
            let current = SettingsInit::from_store(&self.store, &prefix);
            let initialized = slot.gadget.initialize_settings(current);
            initialized.apply(&self.store, &prefix);

            // Host-managed enabled key: `enabled.<id>`.
            // Defaults to true if no value exists.
            let enabled_key = format!("enabled.{id}");
            if self.store.get(&enabled_key).is_none() {
                self.store
                    .set(enabled_key.clone(), serde_json::Value::Bool(true));
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
            let id = slot.gadget.id();

            // Watch the host-managed enabled key for shortcut reactor.
            keys_to_watch.push(format!("enabled.{id}"));

            // Watch shortcut keys so the reactor re-registers when
            // a user changes a shortcut binding.
            for s in slot.gadget.shortcuts() {
                keys_to_watch.push(format!("gadgets.{id}.{}", s.settings_key));
            }
        }
        self.watched_keys.extend(keys_to_watch);

        // -------------------------------------------------------
        // Register initial shortcuts
        // -------------------------------------------------------
        self.register_all_shortcuts(&ctx.app);

        // -------------------------------------------------------
        // Phase 2: Parallel gadget startup (background)
        //
        // Each gadget's `enable()` runs on a blocking thread.
        // On failure the shared `enabled` flag is set to false
        // so the host stops dispatching to the gadget.
        // -------------------------------------------------------
        let runtime = tauri::async_runtime::handle();
        for slot in &self.slots {
            if !slot.enabled.load(Ordering::Relaxed) {
                continue;
            }

            let gadget = Arc::clone(&slot.gadget);
            let enabled = Arc::clone(&slot.enabled);
            runtime.spawn_blocking(move || {
                if gadget.enable().is_err() {
                    enabled.store(false, Ordering::Relaxed);
                }
            });
        }
    }

    /// Spawn the shortcut reactor task. Called once after the host
    /// is wrapped in `Arc` and managed as Tauri state, so we can
    /// pass `Arc<GadgetHost>` into the async task.
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

        // Collect shortcuts from all enabled gadgets.
        for slot in &self.slots {
            if !slot.is_active() {
                continue;
            }
            let gadget = &slot.gadget;
            let gadget_id = gadget.id().to_string();
            for decl in gadget.shortcuts() {
                if let Some(r) = self.resolve_shortcut(&gadget_id, &decl, Arc::clone(gadget)) {
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

                    // Gadget shortcut routing.
                    let Some(r) = registered.iter().find(|r| r.shortcut == *shortcut) else {
                        return;
                    };

                    let result = r.owner.handle_shortcut(&r.shortcut_id);

                    match result {
                        Ok(PostAction::ShowCustomUI { view, data }) => {
                            show_launcher_with_gadget(&handle, &r.gadget_id, &view, data);
                        }
                        Ok(_) => {}
                        Err(e) => {
                            eprintln!(
                                "shortcut: {}.{} handler failed: {e:#}",
                                r.gadget_id, r.shortcut_id
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
        gadget_id: &str,
        decl: &GadgetShortcut,
        owner: Arc<dyn Gadget>,
    ) -> Option<RegisteredShortcut> {
        let full_key = format!("gadgets.{gadget_id}.{}", decl.settings_key);

        let combo_str = self
            .store
            .get(&full_key)
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| decl.default_shortcut.to_string());

        let shortcut = match combo_str.parse::<Shortcut>() {
            Ok(s) => s,
            Err(e) => {
                eprintln!(
                    "shortcut: invalid combo '{combo_str}' for {gadget_id}.{}: {e}",
                    decl.id
                );
                return None;
            }
        };

        Some(RegisteredShortcut {
            shortcut,
            gadget_id: gadget_id.to_string(),
            shortcut_id: decl.id.to_string(),
            owner,
        })
    }

    // =========================================================
    // Search
    // =========================================================

    /// Search all gadgets against the given query, streaming
    /// results to the frontend as they become available.
    ///
    /// Catalog results are sent first (sync, fast). Query gadgets
    /// are dispatched concurrently on the blocking thread pool and
    /// their results stream to the frontend as each gadget
    /// completes.
    pub async fn search(&self, query: &str, on_results: &tauri::ipc::Channel<SearchMessage>) {
        self.entry_store.clear();

        if query.is_empty() {
            let _ = on_results.send(SearchMessage::Done);
            return;
        }

        // =======================================================
        // Prefix routing: longest match wins. Exclusive — only
        // the matched gadget runs, no catalogs, no fan-out.
        // =======================================================

        if let Some((gadget, prefix)) = self.find_prefix_match(query) {
            // Note: prefix match already checked is_active() internally
            let stripped = query[prefix.len()..].to_string();
            let source = gadget.id().to_string();
            let prefix_owned = prefix.to_string();
            let gadget = Arc::clone(gadget);

            let prefix_for_search = prefix_owned.clone();
            let response = tokio::task::spawn_blocking(move || {
                gadget.search(&stripped, Some(&prefix_for_search))
            })
            .await
            .expect("prefix search task not panicked");

            let mut custom_gadget_view = None;
            let mut inline_gadget_view = None;
            let mut entries = Vec::new();

            if let Some(response) = response {
                let (view_ref, results) = self.process_gadget_response(
                    response, &source, true, // prefix mode — CustomUI allowed
                );

                if let Some((kind, vr)) = view_ref {
                    match kind {
                        ViewKind::Custom => custom_gadget_view = Some(vr),
                        ViewKind::Inline => inline_gadget_view = Some(vr),
                    }
                }

                entries.extend(results);

                // Exactly one gadget responds in prefix mode,
                // so a stable score-descending sort preserves
                // the gadget's intended ordering for equal-
                // score items. The `cmp_sort_key` tiebreaker
                // used on the merging path (`entry.id ASC`)
                // would overwrite that intent.
                self.frecency.apply_scores(&source, &mut entries);
                entries.sort_by_key(|entry| std::cmp::Reverse(entry.inner.score));
            }

            self.store_sourced_entries(&entries);

            let _ = on_results.send(SearchMessage::SearchResults {
                source: ResultSource::Gadget { id: source.clone() },
                entries,
                custom_gadget_view,
                inline_gadget_view,
                matched_prefix: Some(prefix_owned),
            });
            let _ = on_results.send(SearchMessage::Done);
            return;
        }

        // =======================================================
        // No prefix: catalog search + concurrent query gadgets.
        // =======================================================

        // Phase 1: catalog search (sync, CPU-bound). Send results
        // to the frontend immediately.
        let gadgets: Vec<Arc<dyn Gadget>> = self
            .slots
            .iter()
            .filter(|s| s.is_active())
            .map(|s| Arc::clone(&s.gadget))
            .collect();
        let frecency = self.frecency.clone();
        let query_owned = query.to_string();

        let catalog_results = tokio::task::spawn_blocking({
            let query = query_owned.clone();
            let frecency = frecency.clone();
            move || Self::search_catalogs_static(&gadgets, &frecency, &query)
        })
        .await
        .expect("catalog search task not panicked");

        // Always emit — the frontend uses the catalog batch as
        // the canonical "new generation, recompute" signal, so
        // an empty catalog must still cross the wire.
        self.store_sourced_entries(&catalog_results);
        let _ = on_results.send(SearchMessage::SearchResults {
            source: ResultSource::Catalog,
            entries: catalog_results,
            custom_gadget_view: None,
            inline_gadget_view: None,
            matched_prefix: None,
        });

        // Phase 2: query gadgets — spawn concurrently, deliver
        // results to the frontend as each gadget completes.
        let mut join_set = JoinSet::new();

        for slot in &self.slots {
            if !slot.is_active() {
                continue;
            }

            let source = slot.gadget.id().to_string();
            let gadget = Arc::clone(&slot.gadget);
            let query = query_owned.clone();

            join_set.spawn_blocking(move || (source, gadget.search(&query, None)));
        }

        // Drain the JoinSet — each completed task yields one
        // gadget's results, preserving incremental delivery.
        let mut inline_claimed = false;

        while let Some(result) = join_set.join_next().await {
            let (source, response) = result.expect("query search task not panicked");

            let response = match response {
                Some(r) => r,
                None => continue,
            };

            let (view_ref, mut entries) = self.process_gadget_response(
                response, &source, false, // non-prefix — CustomUI downgraded
            );

            self.frecency.apply_scores(&source, &mut entries);
            entries.sort_by(|a, b| a.cmp_sort_key(b));

            let inline_gadget_view = match view_ref {
                Some((ViewKind::Inline, vr)) if !inline_claimed => {
                    inline_claimed = true;
                    Some(vr)
                }
                Some((ViewKind::Inline, _)) => {
                    eprintln!(
                        "search: dropping InlineUI from gadget '{}' — \
                         another gadget already claimed the inline slot",
                        source
                    );
                    None
                }
                _ => None,
            };

            self.store_sourced_entries(&entries);

            // Always emit; a gadget going from results to empty
            // relies on this message to evict its prior entries.
            let _ = on_results.send(SearchMessage::SearchResults {
                source: ResultSource::Gadget { id: source },
                entries,
                custom_gadget_view: None,
                inline_gadget_view,
                matched_prefix: None,
            });
        }

        let _ = on_results.send(SearchMessage::Done);
    }

    /// Insert all entries from a `SourcedEntry` slice into the
    /// entry store, grouped by their `source` field.
    fn store_sourced_entries(&self, entries: &[SourcedEntry]) {
        for entry in entries {
            self.entry_store
                .insert(&entry.source, std::slice::from_ref(&entry.inner));
        }
    }

    /// Delegate to the standalone function for testability.
    fn process_gadget_response(
        &self,
        response: GadgetResponse,
        source: &str,
        allow_custom_ui: bool,
    ) -> (Option<(ViewKind, GadgetViewRef)>, Vec<SourcedEntry>) {
        process_gadget_response(response, source, allow_custom_ui)
    }

    /// Delegate to the standalone function for testability.
    fn find_prefix_match<'a>(&'a self, query: &str) -> Option<(&'a Arc<dyn Gadget>, &'a str)> {
        find_prefix_match(&self.slots, query)
    }

    /// Catalog search as a static method so it can run on
    /// `spawn_blocking` without borrowing `&self`.
    ///
    /// Calls `entries()` on every gadget — catalog-only gadgets
    /// return their entry list, query-only gadgets return the
    /// default empty vec (zero cost).
    fn search_catalogs_static(
        gadgets: &[Arc<dyn Gadget>],
        frecency: &FrecencyStore,
        query: &str,
    ) -> Vec<SourcedEntry> {
        // `prefer_prefix` biases scoring toward matches that
        // start near the beginning of the haystack — the
        // autocompletion feel we want everywhere the user is
        // typing to narrow down a known title.
        let mut config = Config::DEFAULT;
        config.prefer_prefix = true;
        let mut matcher = Matcher::new(config);
        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);

        let mut results = Vec::new();
        let mut char_buf = Vec::new();
        let mut title_indices = Vec::new();

        for gadget in gadgets {
            // Disabled gadgets are already filtered out by the caller.
            let source = gadget.id().to_string();

            // Track where this gadget's results start so we can
            // apply frecency scores to just this slice afterwards.
            let gadget_start = results.len();

            for entry in gadget.entries() {
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
                            data: None,
                        },
                    ));
                }
            }

            // Apply frecency bonuses to this gadget's results.
            frecency.apply_scores(&source, &mut results[gadget_start..]);
        }

        // Sort catalog results by the deterministic composite key
        // before sending to the frontend.
        results.sort_by(|a, b| a.cmp_sort_key(b));

        results
    }

    // =========================================================
    // Execute / Message Routing
    // =========================================================

    /// Dispatch a launcher action to the gadget that owns
    /// `entry_id`.
    ///
    /// Tokio runtime precondition: callers must invoke this from a
    /// thread that has a current Tokio runtime (a worker or a
    /// `spawn_blocking` task on a multi-thread runtime). Gadget
    /// `execute()` implementations can reach the `http::fetch` host
    /// import, which calls `Handle::current()` inside reqwest's
    /// internal machinery. Tauri's synchronous `#[tauri::command]`
    /// dispatches on the IPC blocking thread, which does **not**
    /// satisfy this precondition — Tauri commands routing to this
    /// function must be `async fn` and wrap the call in
    /// `tokio::task::spawn_blocking` (see `search_execute`).
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

        // `OpenSettings` is a host-managed action: regardless of which
        // gadget emitted the entry, navigation to the settings panel is
        // the host's responsibility, and the gadget has nothing useful
        // to do with the action. Short-circuit before dispatching so
        // every gadget gets the behaviour for free without each
        // implementing the same routing.
        if matches!(action_id, ActionId::OpenSettings) {
            if let Err(e) = app.emit(
                "open-gadget-settings",
                serde_json::json!({ "gadgetId": source }),
            ) {
                eprintln!("emit open-gadget-settings failed: {e:#}");
            }
            return Ok(PostAction::Dismiss);
        }

        let Some(entry) = self.entry_store.get(source, entry_id) else {
            eprintln!(
                "execute: entry '{entry_id}' from gadget '{source}' not found in entry store \
                 (bug — the UI should only execute entries from the current search)"
            );
            return Ok(PostAction::Nothing);
        };

        if let Some(slot) = self.slots.iter().find(|s| s.gadget.id() == source) {
            let post_action = slot.gadget.execute(&entry, action_id)?;
            // Host-level PostActions are handled here and mapped to Dismiss
            // before returning, since the frontend has no use for them.
            return Ok(match post_action {
                PostAction::Quit => {
                    app.exit(0);
                    PostAction::Dismiss
                }
                PostAction::ShowSettings => {
                    crate::show_settings_window(app);
                    PostAction::Dismiss
                }
                PostAction::ShowDevtools => {
                    crate::show_devtools_window(app);
                    PostAction::Dismiss
                }
                other => other,
            });
        }
        anyhow::bail!("unknown gadget source: {source}");
    }

    pub fn handle_message(
        &self,
        source: &str,
        method: &str,
        payload: serde_json::Value,
        channel: tauri::ipc::Channel<serde_json::Value>,
    ) -> anyhow::Result<serde_json::Value> {
        if let Some(slot) = self.slots.iter().find(|s| s.gadget.id() == source) {
            return slot.gadget.handle_message(method, payload, channel);
        }
        anyhow::bail!("unknown gadget source: {source}");
    }

    // =========================================================
    // Shutdown
    // =========================================================

    /// Disable all gadgets during app exit.
    pub fn disable_all(&self) {
        for slot in &self.slots {
            slot.gadget.disable();
        }
    }

    // =========================================================
    // Settings Change Dispatch
    // =========================================================

    /// Handle a settings change event from the store. Routes
    /// `enabled.<id>` changes to the host-managed lifecycle and
    /// `gadgets.<id>.*` changes to the gadget's `setting_changed`.
    ///
    /// Called from the `settings-changed` Tauri event listener.
    /// Both paths go through the gadget's `CoalescingDispatcher`
    /// for serialization and dedup.
    pub fn handle_setting_changed(&self, key: &str, value: serde_json::Value) {
        // -------------------------------------------------------
        // Path 1: enabled.<gadget-id>
        // -------------------------------------------------------
        if let Some(gadget_id) = key.strip_prefix("enabled.") {
            let Some(slot) = self.slots.iter().find(|s| s.gadget.id() == gadget_id) else {
                return;
            };

            slot.dispatcher.enqueue(key.to_string(), value);

            let gadget = Arc::clone(&slot.gadget);
            let enabled_flag = &slot.enabled;

            slot.dispatcher.dispatch(|dispatch_key, dispatch_value| {
                let Some(_id) = dispatch_key.strip_prefix("enabled.") else {
                    return;
                };

                let new_enabled = dispatch_value.as_bool().unwrap_or(true);
                let was_enabled = enabled_flag.swap(new_enabled, Ordering::Relaxed);

                if new_enabled && !was_enabled {
                    if gadget.enable().is_err() {
                        enabled_flag.store(false, Ordering::Relaxed);
                    }
                } else if !new_enabled && was_enabled {
                    gadget.disable();
                }
            });

            // Always signal shortcut reactor on enabled changes
            // so shortcuts are re-registered accordingly.
            self.notify_shortcut_change();
            return;
        }

        // -------------------------------------------------------
        // Path 2: gadgets.<gadget-id>.<setting-key>
        // -------------------------------------------------------
        if let Some(rest) = key.strip_prefix("gadgets.") {
            // Split "gadget-id.setting-key" at the first dot.
            let Some(dot_pos) = rest.find('.') else {
                return;
            };
            let gadget_id = &rest[..dot_pos];
            let setting_key = &rest[dot_pos + 1..];

            let Some(slot) = self.slots.iter().find(|s| s.gadget.id() == gadget_id) else {
                return;
            };

            slot.dispatcher.enqueue(setting_key.to_string(), value);

            let gadget = Arc::clone(&slot.gadget);
            slot.dispatcher.dispatch(|k, v| {
                gadget.setting_changed(k, v.clone());
            });
        }
    }
}

// =========================================================
// Search Helpers (standalone for testability)
// =========================================================

/// Process a `GadgetResponse` into scored entries and an
/// optional view reference.
///
/// `allow_custom_ui` controls whether `CustomUI` responses
/// produce a view reference. In non-prefix (always-on) mode,
/// `CustomUI` is downgraded to plain results — entries are
/// still extracted but the custom view is dropped.
fn process_gadget_response(
    response: GadgetResponse,
    source: &str,
    allow_custom_ui: bool,
) -> (Option<(ViewKind, GadgetViewRef)>, Vec<SourcedEntry>) {
    let mut view_ref = None;

    match &response {
        GadgetResponse::CustomUI { view, data, .. } if allow_custom_ui => {
            view_ref = Some((
                ViewKind::Custom,
                GadgetViewRef {
                    gadget_id: source.to_string(),
                    view: view.clone(),
                    data: data.clone(),
                },
            ));
        }
        GadgetResponse::CustomUI { .. } => {
            // CustomUI is only honoured in prefix mode. In non-prefix
            // (always-on) mode we downgrade to plain results so the
            // gadget's entries still appear but without the custom view.
            eprintln!(
                "search: dropping CustomUI from gadget '{}' — \
                 CustomUI is only supported in prefix mode",
                source
            );
        }
        GadgetResponse::InlineUI { view, data, .. } => {
            view_ref = Some((
                ViewKind::Inline,
                GadgetViewRef {
                    gadget_id: source.to_string(),
                    view: view.clone(),
                    data: data.clone(),
                },
            ));
        }
        GadgetResponse::Results(_) => {}
    }

    let entries: Vec<SourcedEntry> = match response {
        GadgetResponse::Results(results) => results,
        GadgetResponse::CustomUI { results, .. } | GadgetResponse::InlineUI { results, .. } => {
            results
        }
    }
    .into_iter()
    .map(|r| SourcedEntry::new(source.to_string(), r))
    .collect();

    (view_ref, entries)
}

/// Find the gadget whose registered prefix is the longest
/// match for `query`. Returns `None` when no prefix matches.
/// Disabled gadgets are skipped.
fn find_prefix_match<'a>(
    slots: &'a [GadgetSlot],
    query: &str,
) -> Option<(&'a Arc<dyn Gadget>, &'a str)> {
    let mut best: Option<(&Arc<dyn Gadget>, &str)> = None;
    let mut best_len = 0;

    for slot in slots {
        if !slot.is_active() {
            continue;
        }
        for prefix in slot.gadget.search_prefixes() {
            if prefix.len() > best_len && query.starts_with(prefix.as_str()) {
                best = Some((&slot.gadget, prefix.as_str()));
                best_len = prefix.len();
            }
        }
    }

    best
}

// =========================================================
// Helpers
// =========================================================

/// Show the launcher and emit `activate-gadget-custom-ui` so the
/// frontend switches to the gadget's view.
fn show_launcher_with_gadget(
    app: &tauri::AppHandle,
    gadget_id: &str,
    view: &str,
    data: Option<serde_json::Value>,
) {
    let Some(layout) = app.state::<crate::LauncherLayoutState>().get().copied() else {
        eprintln!("shortcut: launcher layout not yet received, ignoring");
        return;
    };
    crate::position_launcher_on_cursor_monitor(app, &layout);

    if let Err(e) = crate::show_launcher(app) {
        eprintln!("shortcut: failed to show launcher: {e:#}");
        return;
    }

    if let Err(e) = app.emit(
        "activate-gadget-custom-ui",
        ActivateGadgetPayload {
            gadget_id: gadget_id.to_string(),
            view: view.to_string(),
            data,
        },
    ) {
        eprintln!("shortcut: failed to emit activate-gadget-custom-ui: {e:#}");
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::types::CatalogEntry;

    // -------------------------------------------------------
    // Mock Gadget
    //
    // Configurable stub implementing `Gadget` for unit tests.
    // Each field controls a specific trait method's return
    // value. Defaults produce an empty, enabled, prefix-free
    // gadget.
    // -------------------------------------------------------

    struct MockGadget {
        id: String,
        enabled: bool,
        prefixes: Vec<String>,
        catalog_entries: Vec<CatalogEntry>,
        search_response: Option<GadgetResponse>,
        execute_response: PostAction,
    }

    impl MockGadget {
        fn new(id: &str) -> Self {
            Self {
                id: id.to_string(),
                enabled: true,
                prefixes: vec![],
                catalog_entries: vec![],
                search_response: None,
                execute_response: PostAction::Nothing,
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

        fn with_search_response(mut self, response: GadgetResponse) -> Self {
            self.search_response = Some(response);
            self
        }

        fn with_execute_response(mut self, response: PostAction) -> Self {
            self.execute_response = response;
            self
        }
    }

    impl Gadget for MockGadget {
        fn id(&self) -> &str {
            &self.id
        }

        fn search_prefixes(&self) -> &[String] {
            &self.prefixes
        }

        fn entries(&self) -> Vec<CatalogEntry> {
            self.catalog_entries.clone()
        }

        fn search(&self, _query: &str, _matched_prefix: Option<&str>) -> Option<GadgetResponse> {
            self.search_response.clone()
        }

        fn execute(
            &self,
            _entry: &ScoredEntry,
            _action_id: &ActionId,
        ) -> anyhow::Result<PostAction> {
            Ok(self.execute_response.clone())
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
            data: None,
        }
    }

    /// Helper to wrap mock gadgets in `GadgetSlot`. Tests
    /// default slots to `GadgetSourceKind::Builtin` since
    /// they exercise host routing logic, not source-kind
    /// plumbing — dedicated tests below cover the source-kind
    /// path.
    fn gadget_slots(gadgets: Vec<MockGadget>) -> Vec<GadgetSlot> {
        gadgets
            .into_iter()
            .map(|p| {
                let enabled = p.enabled;
                let slot = GadgetSlot::new(Arc::new(p), GadgetSourceKind::Builtin);
                slot.enabled.store(enabled, Ordering::Relaxed);
                slot
            })
            .collect()
    }

    // =======================================================
    // Gadget::search() contract tests
    // =======================================================

    #[test]
    fn default_search_returns_none() {
        let gadget = MockGadget::new("empty");
        assert!(Gadget::search(&gadget, "anything", None).is_none());
    }

    #[test]
    fn search_returns_configured_response() {
        let gadget = MockGadget::new("test")
            .with_search_response(GadgetResponse::Results(vec![scored_entry("r1", 100)]));

        let result = Gadget::search(&gadget, "query", None);
        assert!(result.is_some());

        match result.unwrap() {
            GadgetResponse::Results(entries) => {
                assert_eq!(entries.len(), 1);
                assert_eq!(entries[0].id, "r1");
            }
            _ => panic!("expected Results variant"),
        }
    }

    #[test]
    fn search_returns_custom_ui() {
        let gadget = MockGadget::new("test").with_search_response(GadgetResponse::CustomUI {
            view: "history".into(),
            data: Some(serde_json::json!({"key": "value"})),
            results: vec![scored_entry("h1", 50)],
        });

        let result = Gadget::search(&gadget, "=2+2", Some("="));
        match result.unwrap() {
            GadgetResponse::CustomUI {
                view,
                data,
                results,
            } => {
                assert_eq!(view, "history");
                assert!(data.is_some());
                assert_eq!(results.len(), 1);
            }
            _ => panic!("expected CustomUI variant"),
        }
    }

    #[test]
    fn search_returns_inline_ui() {
        let gadget = MockGadget::new("test").with_search_response(GadgetResponse::InlineUI {
            view: "result".into(),
            data: None,
            results: vec![],
        });

        let result = Gadget::search(&gadget, "42", None);
        match result.unwrap() {
            GadgetResponse::InlineUI { view, .. } => {
                assert_eq!(view, "result");
            }
            _ => panic!("expected InlineUI variant"),
        }
    }

    // =======================================================
    // Gadget::search_prefixes() contract tests
    // =======================================================

    #[test]
    fn default_prefixes_are_empty() {
        let gadget = MockGadget::new("no-prefix");
        assert!(Gadget::search_prefixes(&gadget).is_empty());
    }

    #[test]
    fn configured_prefixes_returned() {
        let gadget = MockGadget::new("calc").with_prefixes(&["=", "calc "]);
        let prefixes = Gadget::search_prefixes(&gadget);
        assert_eq!(prefixes.len(), 2);
        assert_eq!(prefixes[0], "=");
        assert_eq!(prefixes[1], "calc ");
    }

    // =======================================================
    // find_prefix_match() tests
    // =======================================================

    #[test]
    fn no_gadgets_no_match() {
        let gadgets = gadget_slots(vec![]);
        assert!(find_prefix_match(&gadgets, "=2+2").is_none());
    }

    #[test]
    fn no_prefix_gadgets_no_match() {
        let gadgets = gadget_slots(vec![MockGadget::new("a"), MockGadget::new("b")]);
        assert!(find_prefix_match(&gadgets, "hello").is_none());
    }

    #[test]
    fn single_prefix_match() {
        let gadgets = gadget_slots(vec![MockGadget::new("calc").with_prefixes(&["="])]);
        let (gadget, prefix) = find_prefix_match(&gadgets, "=2+2").unwrap();
        assert_eq!(gadget.id(), "calc");
        assert_eq!(prefix, "=");
    }

    #[test]
    fn longest_prefix_wins() {
        let gadgets = gadget_slots(vec![
            MockGadget::new("short").with_prefixes(&["!"]),
            MockGadget::new("long").with_prefixes(&["!g"]),
        ]);

        // "!google" matches both "!" and "!g" — longest wins.
        let (gadget, prefix) = find_prefix_match(&gadgets, "!google").unwrap();
        assert_eq!(gadget.id(), "long");
        assert_eq!(prefix, "!g");
    }

    #[test]
    fn prefix_must_be_at_start() {
        let gadgets = gadget_slots(vec![MockGadget::new("calc").with_prefixes(&["="])]);
        // "hello =" doesn't start with "=".
        assert!(find_prefix_match(&gadgets, "hello =").is_none());
    }

    #[test]
    fn disabled_gadget_prefix_skipped() {
        let gadgets = gadget_slots(vec![
            MockGadget::new("calc")
                .with_prefixes(&["="])
                .with_enabled(false),
        ]);
        assert!(find_prefix_match(&gadgets, "=2+2").is_none());
    }

    #[test]
    fn disabled_gadget_skipped_fallback_to_shorter() {
        let gadgets = gadget_slots(vec![
            MockGadget::new("disabled-long")
                .with_prefixes(&["!g"])
                .with_enabled(false),
            MockGadget::new("enabled-short").with_prefixes(&["!"]),
        ]);

        let (gadget, prefix) = find_prefix_match(&gadgets, "!google").unwrap();
        assert_eq!(gadget.id(), "enabled-short");
        assert_eq!(prefix, "!");
    }

    #[test]
    fn multi_char_prefix() {
        let gadgets = gadget_slots(vec![
            MockGadget::new("emoji").with_prefixes(&[":"]),
            MockGadget::new("http").with_prefixes(&["http://", "https://"]),
        ]);

        let (gadget, prefix) = find_prefix_match(&gadgets, "https://example.com").unwrap();
        assert_eq!(gadget.id(), "http");
        assert_eq!(prefix, "https://");
    }

    #[test]
    fn exact_prefix_query() {
        // Query is exactly the prefix with nothing after it.
        let gadgets = gadget_slots(vec![MockGadget::new("emoji").with_prefixes(&[":"])]);
        let (gadget, prefix) = find_prefix_match(&gadgets, ":").unwrap();
        assert_eq!(gadget.id(), "emoji");
        assert_eq!(prefix, ":");
    }

    #[test]
    fn multiple_prefixes_same_gadget() {
        let gadgets = gadget_slots(vec![
            MockGadget::new("multi").with_prefixes(&["http://", "https://"]),
        ]);

        let (_, prefix) = find_prefix_match(&gadgets, "http://foo.com").unwrap();
        assert_eq!(prefix, "http://");

        let (_, prefix) = find_prefix_match(&gadgets, "https://foo.com").unwrap();
        assert_eq!(prefix, "https://");
    }

    // =======================================================
    // process_gadget_response() tests
    // =======================================================

    #[test]
    fn results_response_extracts_entries() {
        let response = GadgetResponse::Results(vec![scored_entry("a", 100), scored_entry("b", 50)]);
        let (view, entries) = process_gadget_response(response, "test-gadget", false);
        assert!(view.is_none());
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].inner.id, "a");
        assert_eq!(entries[1].inner.id, "b");
        assert_eq!(entries[0].source, "test-gadget");
    }

    #[test]
    fn empty_results_yields_empty_entries() {
        let response = GadgetResponse::Results(vec![]);
        let (view, entries) = process_gadget_response(response, "p", false);
        assert!(view.is_none());
        assert!(entries.is_empty());
    }

    #[test]
    fn custom_ui_allowed_in_prefix_mode() {
        let response = GadgetResponse::CustomUI {
            view: "history".into(),
            data: Some(serde_json::json!({"x": 1})),
            results: vec![scored_entry("h1", 10)],
        };
        let (view, entries) = process_gadget_response(response, "calc", true);
        let (kind, vr) = view.unwrap();
        assert!(matches!(kind, ViewKind::Custom));
        assert_eq!(vr.gadget_id, "calc");
        assert_eq!(vr.view, "history");
        assert!(vr.data.is_some());
        // Entries are still extracted alongside the view.
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn custom_ui_downgraded_outside_prefix_mode() {
        let response = GadgetResponse::CustomUI {
            view: "picker".into(),
            data: None,
            results: vec![scored_entry("e1", 20), scored_entry("e2", 10)],
        };
        let (view, entries) = process_gadget_response(response, "emoji", false);
        // View is dropped (not allowed outside prefix mode).
        assert!(view.is_none());
        // Entries are still extracted from the CustomUI response.
        assert_eq!(entries.len(), 2);
    }

    #[test]
    fn inline_ui_produces_view_ref() {
        let response = GadgetResponse::InlineUI {
            view: "result".into(),
            data: Some(serde_json::json!({"result": "42"})),
            results: vec![],
        };
        let (view, entries) = process_gadget_response(response, "calc", false);
        let (kind, vr) = view.unwrap();
        assert!(matches!(kind, ViewKind::Inline));
        assert_eq!(vr.view, "result");
        assert!(entries.is_empty());
    }

    #[test]
    fn inline_ui_allowed_in_both_modes() {
        // InlineUI should work regardless of prefix/non-prefix mode.
        for allow_custom in [true, false] {
            let response = GadgetResponse::InlineUI {
                view: "v".into(),
                data: None,
                results: vec![],
            };
            let (view, _) = process_gadget_response(response, "p", allow_custom);
            assert!(
                view.is_some(),
                "InlineUI should produce view ref with allow_custom={allow_custom}"
            );
        }
    }

    #[test]
    fn custom_ui_with_no_results_prefix_mode() {
        let response = GadgetResponse::CustomUI {
            view: "picker".into(),
            data: None,
            results: vec![],
        };
        let (view, entries) = process_gadget_response(response, "emoji", true);
        assert!(view.is_some());
        assert!(entries.is_empty());
    }

    #[test]
    fn source_id_propagated_to_entries() {
        let response = GadgetResponse::Results(vec![scored_entry("x", 1)]);
        let (_, entries) = process_gadget_response(response, "my-gadget", false);
        assert_eq!(entries[0].source, "my-gadget");
    }

    // =======================================================
    // GadgetSourceKind plumbing through the slot
    // =======================================================

    /// Every variant must survive `GadgetSlot::new`. The
    /// field drives the frontend source badge, so a slot
    /// that silently dropped the kind would present the
    /// wrong origin to the user.
    #[test]
    fn slot_preserves_every_source_kind_variant() {
        for kind in [
            GadgetSourceKind::Builtin,
            GadgetSourceKind::System,
            GadgetSourceKind::User,
            GadgetSourceKind::Dev,
        ] {
            let slot = GadgetSlot::new(Arc::new(MockGadget::new("probe")), kind);
            assert_eq!(slot.source_kind, kind);
        }
    }

    /// Aggregation across multiple slots yields the gadget-id →
    /// source-kind map exposed to the frontend. This mirrors
    /// the body of `GadgetHost::gadget_sources`; together with
    /// the per-slot preservation test above it is sufficient
    /// coverage for the command's output without constructing
    /// a full `GadgetHost` (which would require a real
    /// `Store` + `FrecencyStore`).
    // =======================================================
    // MockGadget::execute() contract tests
    // =======================================================

    #[test]
    fn mock_execute_returns_default_nothing() {
        let gadget = MockGadget::new("test");
        // Cannot call execute() directly without an AppHandle,
        // but we can verify the configured response is used via
        // the trait method through a slot. For now, verify the
        // field default.
        assert!(matches!(gadget.execute_response, PostAction::Nothing));
    }

    #[test]
    fn mock_execute_returns_configured_response() {
        let gadget = MockGadget::new("test").with_execute_response(PostAction::Dismiss);
        assert!(matches!(gadget.execute_response, PostAction::Dismiss));
    }

    // =======================================================
    // GadgetSourceKind plumbing through the slot
    // =======================================================

    #[test]
    fn slot_aggregation_produces_expected_source_map() {
        let slots = vec![
            GadgetSlot::new(
                Arc::new(MockGadget::new("builtin-a")),
                GadgetSourceKind::Builtin,
            ),
            GadgetSlot::new(
                Arc::new(MockGadget::new("system-x")),
                GadgetSourceKind::System,
            ),
            GadgetSlot::new(Arc::new(MockGadget::new("user-y")), GadgetSourceKind::User),
            GadgetSlot::new(Arc::new(MockGadget::new("dev-z")), GadgetSourceKind::Dev),
        ];
        let map: std::collections::HashMap<String, GadgetSourceKind> = slots
            .iter()
            .map(|slot| (slot.gadget.id().to_string(), slot.source_kind))
            .collect();

        assert_eq!(map.len(), 4);
        assert_eq!(map["builtin-a"], GadgetSourceKind::Builtin);
        assert_eq!(map["system-x"], GadgetSourceKind::System);
        assert_eq!(map["user-y"], GadgetSourceKind::User);
        assert_eq!(map["dev-z"], GadgetSourceKind::Dev);
    }
}
