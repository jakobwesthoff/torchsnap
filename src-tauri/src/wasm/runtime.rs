// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WASM Plugin Runtime
//
// Manages the wasmtime engine (shared across all plugins)
// and provides per-plugin instances that wrap a Store and
// typed component bindings.
//
// Architecture:
//   WasmRuntime (one per app, owns Engine)
//    └── instantiate(wasm_bytes) → WasmPluginInstance
//
//   WasmPluginInstance (one per plugin, owns Store + Plugin)
//    ├── enable() / disable()
//    ├── entries() → Vec<CatalogEntry>
//    └── execute(entry_id, action_id) → PostAction
// =========================================================

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use wasmtime::component::{Component, HasSelf, Linker, Resource, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use super::bindings;
use super::logging::channel::LogSender;
use super::logging::spans::{Logger, SpanRegistry};
use super::logging::{LogItem, LogItemKind, LogLevel, LogSource};
use crate::settings::PluginSettings;
use crate::storage::{SqlStorage, SqlValue as HostSqlValue};

// =========================================================
// Per-Plugin Store State
//
// This is the `T` in `Store<T>`. It holds the WASI context
// and any per-plugin state the host imports need access to
// (e.g., the plugin ID for log tagging, the log sender).
// =========================================================

pub struct PluginState {
    plugin_id: String,
    wasi: WasiCtx,
    wasi_table: ResourceTable,
    log_sender: LogSender,
    span_registry: Arc<SpanRegistry>,
    /// Per-plugin namespaced settings reader. `None` until the
    /// bridge stashes the `PluginContext.settings` handle on
    /// `enable()`. The settings host import (`settings::get`)
    /// errors gracefully if accessed before that happens —
    /// which it shouldn't, since the host always calls
    /// `enable()` before any guest code runs.
    settings: Option<PluginSettings>,
    /// Per-plugin SQL storage configuration. Populated by
    /// the bridge at construction time from the manifest's
    /// `[storage.sql]` block (`SqlConfig::None` when the
    /// plugin declares no SQL storage). The first
    /// `sql::open()` call materializes the actual database
    /// into `sql_storage` below; subsequent calls reuse the
    /// same `Arc<SqlStorage>`.
    sql_config: SqlConfig,
    sql_storage: Option<Arc<SqlStorage>>,
    /// Resource reps for every `SqlHandleEntry` currently
    /// live in `wasi_table`. Pushed on `sql::open()`,
    /// removed on the WIT-driven `drop()` of an individual
    /// handle, and drained-and-deleted on
    /// `clear_sql_storage()` so that disable cleanly
    /// releases every outstanding `Arc<SqlStorage>`
    /// reference. Without this list, a plugin that opened
    /// a handle and never explicitly dropped it would leak
    /// the rusqlite `Connection` until the
    /// `WasmPluginInstance` itself is dropped (i.e. until
    /// app shutdown).
    sql_handle_reps: Vec<u32>,
    /// Closure that writes a string to the system clipboard.
    /// Stashed by the bridge from the `tauri::AppHandle` on
    /// `enable()` so the `clipboard::write-text` host import
    /// can resolve without `PluginState` itself depending on
    /// the Tauri AppHandle type. `None` between enable
    /// cycles; the host import returns an error if accessed
    /// outside an enable lifetime (which should never
    /// happen — every guest call runs inside one).
    clipboard_writer: Option<ClipboardWriter>,
}

/// Closure type for the clipboard write capability.
///
/// Boxed and stored on `PluginState` instead of holding a
/// `tauri::AppHandle` directly so the runtime layer stays
/// decoupled from Tauri-specific types. The bridge
/// constructs the closure from its own `AppHandle` and
/// stashes it on `enable()`.
pub type ClipboardWriter = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// Whether and how the plugin's SQL storage is configured.
///
/// Materialized at bridge construction so the wasmtime host
/// import can resolve `sql::open()` synchronously without
/// touching disk on every call. Migration files are read
/// once via `PluginSource::read_file` at load time.
#[derive(Clone)]
pub enum SqlConfig {
    /// Plugin did not declare a `[storage.sql]` block in its
    /// manifest. `sql::open()` returns a clear error.
    None,
    /// Plugin opted into SQL storage. The migration strings
    /// are pre-loaded; the database file is created on
    /// first `sql::open()` call.
    Configured {
        db_path: PathBuf,
        migrations: Arc<Vec<String>>,
    },
}

/// Internal entry stored in the wasmtime `ResourceTable`
/// behind every `Resource<SqlHandleEntry>` returned to a
/// plugin. Holding the `Arc<SqlStorage>` here lets the WIT
/// resource drop semantics (which run when the plugin lets
/// the handle go out of scope) cleanly release just this
/// reference; the underlying `SqlStorage` stays alive on
/// `PluginState::sql_storage` until the plugin is disabled.
pub struct SqlHandleEntry {
    storage: Arc<SqlStorage>,
}

// The WasiView trait is required by wasmtime-wasi to locate
// the WasiCtx and ResourceTable within our custom state.
// In wasmtime 43, ctx() returns a WasiCtxView that bundles
// both references together.
impl WasiView for PluginState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.wasi_table,
        }
    }
}

// =========================================================
// Host Import Implementations
//
// Each WIT `import` interface generates a trait that we
// implement on PluginState. wasmtime calls these when the
// guest invokes an imported function.
// =========================================================

impl bindings::torchsnap::plugin::logging::Host for PluginState {
    fn log(
        &mut self,
        level: bindings::torchsnap::plugin::logging::LogLevel,
        message: String,
        metadata: Vec<(String, String)>,
        span: Option<u64>,
    ) {
        let log_level = match level {
            bindings::torchsnap::plugin::logging::LogLevel::Trace => LogLevel::Trace,
            bindings::torchsnap::plugin::logging::LogLevel::Debug => LogLevel::Debug,
            bindings::torchsnap::plugin::logging::LogLevel::Info => LogLevel::Info,
            bindings::torchsnap::plugin::logging::LogLevel::Warn => LogLevel::Warn,
            bindings::torchsnap::plugin::logging::LogLevel::Error => LogLevel::Error,
        };

        self.log_sender.send(LogItem {
            seq: 0,
            timestamp: SystemTime::now(),
            source: LogSource::Plugin(self.plugin_id.clone()),
            kind: LogItemKind::Message {
                level: log_level,
                message,
                metadata,
                span_id: span,
            },
        });
    }

    fn span_start(
        &mut self,
        name: String,
        parent: Option<u64>,
        metadata: Vec<(String, String)>,
    ) -> u64 {
        let name_for_item = name.clone();
        let meta_for_item = metadata.clone();

        match self.span_registry.start(
            name,
            parent,
            LogSource::Plugin(self.plugin_id.clone()),
            metadata,
        ) {
            Some((id, depth)) => {
                // Emit span-start log item so the frontend can
                // track open spans in real time.
                self.log_sender.send(LogItem {
                    seq: 0,
                    timestamp: SystemTime::now(),
                    source: LogSource::Plugin(self.plugin_id.clone()),
                    kind: LogItemKind::SpanStart {
                        span_id: id,
                        name: name_for_item,
                        parent_id: parent,
                        depth,
                        metadata: meta_for_item,
                    },
                });
                id
            }
            // If nesting depth exceeded, return 0 as a sentinel.
            // The guest can still pass this to span_end, which
            // will be a no-op (ID not found in registry).
            None => 0,
        }
    }

    fn span_end(&mut self, span_id: u64, metadata: Vec<(String, String)>) {
        if let Some(completed) = self.span_registry.end(span_id, metadata) {
            self.log_sender.send(LogItem {
                seq: 0,
                timestamp: SystemTime::now(),
                source: completed.source.clone(),
                kind: completed.into(),
            });
        }
    }
}

// =========================================================
// Settings host import
//
// Routes guest `settings::get(key)` calls through the
// per-plugin `PluginSettings` handle stashed on
// `PluginState`. The plugin namespace prefix
// (`plugins.<id>.`) is added by `PluginSettings::get_raw`
// itself, so plugins can never escape their own bucket.
// =========================================================

impl bindings::torchsnap::plugin::settings::Host for PluginState {
    fn get(&mut self, key: String) -> Option<String> {
        // If the bridge hasn't stashed a `PluginSettings`
        // yet (e.g. settings::get called before enable()),
        // pretend the key is unset rather than panicking.
        // The plugin can fall back to its hard-coded default
        // exactly as it would if the value were genuinely
        // missing from the store.
        let settings = self.settings.as_ref()?;
        settings.get_raw(&key)
    }
}

// =========================================================
// Clipboard host import
//
// Routes guest `clipboard::write-text(text)` calls through
// the closure stashed by the bridge on `enable()`. The
// closure wraps `tauri_plugin_clipboard_manager` so this
// module never depends on the Tauri AppHandle directly.
//
// Read access is intentionally not exposed by the WIT
// interface — see the doc comment on the `clipboard`
// interface in `torchsnap-plugin.wit`.
// =========================================================

impl bindings::torchsnap::plugin::clipboard::Host for PluginState {
    fn write_text(&mut self, text: String) -> Result<(), String> {
        let writer = self.clipboard_writer.as_ref().ok_or_else(|| {
            "clipboard writer not initialized — clipboard::write-text called outside enable lifetime"
                .to_string()
        })?;
        writer(&text)
    }
}

// =========================================================
// SQL host import
//
// `sql::open()` materializes the per-plugin database lazily
// — no file is created until the first call. Migration
// strings are pre-loaded by the bridge at construction time
// from the manifest's `[storage.sql] migrations = [...]`
// list, so this method only needs to create the file and
// run the migrations once.
//
// Subsequent calls within the same enable lifetime return a
// new `Resource` handle pointing at the cached
// `Arc<SqlStorage>` — the underlying connection (and its
// internal mutex) is shared across every outstanding handle.
// Concurrent host imports are not actually possible because
// the wasmtime store mutex (`WasmPluginInstance::store`)
// already serializes every guest call.
//
// The Bindgen-generated `WitSqlValue` and `HostSqlHandle`
// trait names are used by-path here so the mapping between
// the WIT variant and the host's internal `SqlValue` is
// kept entirely in this file.
// =========================================================

impl bindings::torchsnap::plugin::sql::Host for PluginState {
    fn open(&mut self) -> Result<Resource<SqlHandleEntry>, String> {
        // Disallow opening if the plugin didn't declare any
        // [storage.sql] configuration in its manifest. The
        // alternative — silently opening an empty database —
        // would mask a misconfiguration as a runtime no-op.
        let (db_path, migrations) = match &self.sql_config {
            SqlConfig::None => {
                return Err("plugin has no [storage.sql] declared in manifest.toml".into());
            }
            SqlConfig::Configured {
                db_path,
                migrations,
            } => (db_path.clone(), Arc::clone(migrations)),
        };

        // Materialize the SqlStorage on the first open() and
        // reuse it on every subsequent call. SqlStorage::open
        // creates the parent directory, opens the connection,
        // configures pragmas, and runs `rusqlite_migration`
        // on the supplied migration strings.
        if self.sql_storage.is_none() {
            let migration_strs: Vec<&str> = migrations.iter().map(String::as_str).collect();
            let storage = SqlStorage::open(db_path, &migration_strs)
                .map_err(|e| format!("open SQL storage: {e:#}"))?;
            self.sql_storage = Some(Arc::new(storage));
        }

        // Push a fresh resource entry pointing at the cached
        // master Arc. Wasmtime resources are unique handles —
        // we cannot return the literal same handle twice —
        // but every handle backs onto the same underlying
        // connection so the plugin sees identical semantics.
        let entry = SqlHandleEntry {
            storage: Arc::clone(self.sql_storage.as_ref().expect("just set")),
        };
        let handle = self
            .wasi_table
            .push(entry)
            .map_err(|e| format!("allocate SQL handle: {e}"))?;

        // Track the rep so `clear_sql_storage` can drain
        // every outstanding handle on disable, even if the
        // guest forgot to drop them. Without this list, a
        // leaked handle would keep the rusqlite connection
        // alive until the entire WasmPluginInstance is
        // dropped (effectively until app shutdown).
        self.sql_handle_reps.push(handle.rep());

        Ok(handle)
    }
}

impl bindings::torchsnap::plugin::sql::HostSqlHandle for PluginState {
    fn execute(
        &mut self,
        handle: Resource<SqlHandleEntry>,
        sql: String,
        params: Vec<bindings::torchsnap::plugin::sql::SqlValue>,
    ) -> Result<u64, String> {
        let entry = self
            .wasi_table
            .get(&handle)
            .map_err(|e| format!("resolve SQL handle: {e}"))?;
        let native_params: Vec<HostSqlValue> = params.into_iter().map(Into::into).collect();
        entry
            .storage
            .execute(&sql, &native_params)
            .map(|n| n as u64)
            .map_err(|e| format!("execute: {e:#}"))
    }

    fn query(
        &mut self,
        handle: Resource<SqlHandleEntry>,
        sql: String,
        params: Vec<bindings::torchsnap::plugin::sql::SqlValue>,
    ) -> Result<Vec<Vec<bindings::torchsnap::plugin::sql::SqlValue>>, String> {
        let entry = self
            .wasi_table
            .get(&handle)
            .map_err(|e| format!("resolve SQL handle: {e}"))?;
        let native_params: Vec<HostSqlValue> = params.into_iter().map(Into::into).collect();

        // Re-export every column from every row through the
        // WIT variant. `SqlRow::columns()` borrows the
        // materialized `Vec<SqlValue>` so we can clone the
        // values straight across without going through the
        // typed `FromSqlValue` accessor.
        let rows = entry
            .storage
            .query_map(&sql, &native_params, |row| Ok(row.columns().to_vec()))
            .map_err(|e| format!("query: {e:#}"))?;

        Ok(rows
            .into_iter()
            .map(|row| row.into_iter().map(Into::into).collect())
            .collect())
    }

    fn drop(&mut self, handle: Resource<SqlHandleEntry>) -> wasmtime::Result<()> {
        // Untrack the rep first so `clear_sql_storage`
        // doesn't try to double-delete it on disable. The
        // O(N) `retain` is fine — N is the number of
        // currently-outstanding handles, which for any
        // sensible plugin is a small number.
        let rep = handle.rep();
        self.sql_handle_reps.retain(|&r| r != rep);

        // Removing the entry drops just this resource's
        // clone of the master Arc. The underlying
        // SqlStorage stays alive on PluginState's
        // `sql_storage` field until the plugin is
        // disabled.
        self.wasi_table.delete(handle)?;
        Ok(())
    }
}

// ---------------------------------------------------------
// SqlValue ↔ host SqlValue
// ---------------------------------------------------------

impl From<bindings::torchsnap::plugin::sql::SqlValue> for HostSqlValue {
    fn from(v: bindings::torchsnap::plugin::sql::SqlValue) -> Self {
        use bindings::torchsnap::plugin::sql::SqlValue as Wit;
        match v {
            Wit::Null => HostSqlValue::Null,
            Wit::Integer(i) => HostSqlValue::Integer(i),
            Wit::Real(f) => HostSqlValue::Real(f),
            Wit::Text(s) => HostSqlValue::Text(s),
            Wit::Blob(b) => HostSqlValue::Blob(b),
        }
    }
}

impl From<HostSqlValue> for bindings::torchsnap::plugin::sql::SqlValue {
    fn from(v: HostSqlValue) -> Self {
        use bindings::torchsnap::plugin::sql::SqlValue as Wit;
        match v {
            HostSqlValue::Null => Wit::Null,
            HostSqlValue::Integer(i) => Wit::Integer(i),
            HostSqlValue::Real(f) => Wit::Real(f),
            HostSqlValue::Text(s) => Wit::Text(s),
            HostSqlValue::Blob(b) => Wit::Blob(b),
            // The host's `List` variant is for IN-clause
            // expansion before binding; it never appears in
            // result rows and never crosses the WIT
            // boundary. Plugins expand their own IN clauses
            // per ADR 0031.
            HostSqlValue::List(_) => Wit::Null,
        }
    }
}

// =========================================================
// WasmRuntime — shared across all plugins
// =========================================================

/// Shared WASM runtime that holds the wasmtime Engine.
///
/// One instance per application. The Engine caches compiled
/// code and shares configuration, so all plugins benefit
/// from a single Engine.
pub struct WasmRuntime {
    engine: Engine,
    log_sender: LogSender,
    span_registry: Arc<SpanRegistry>,
}

impl WasmRuntime {
    /// Create a new runtime with default configuration.
    pub fn new(log_sender: LogSender, span_registry: Arc<SpanRegistry>) -> anyhow::Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);

        let engine =
            Engine::new(&config).map_err(|e| anyhow::anyhow!("creating wasmtime engine: {e}"))?;
        Ok(Self {
            engine,
            log_sender,
            span_registry,
        })
    }

    /// Create a Logger for a specific plugin, used for
    /// host-side span creation around guest calls.
    fn logger_for(&self, plugin_id: &str) -> Logger {
        Logger::new(
            self.log_sender.clone(),
            Arc::clone(&self.span_registry),
            LogSource::Plugin(plugin_id.to_string()),
        )
    }

    /// Instantiate a WASM plugin from raw component bytes.
    ///
    /// Compiles the component, links host imports (WASI +
    /// plugin interfaces), and creates a ready-to-call
    /// instance. Compilation and instantiation are timed
    /// via the span system.
    pub fn instantiate(
        &self,
        plugin_id: &str,
        wasm_bytes: &[u8],
    ) -> anyhow::Result<WasmPluginInstance> {
        let logger = self.logger_for(plugin_id);

        // Top-level load span encompassing compile + instantiate.
        let load_span = logger.span("load").meta("plugin_id", plugin_id).start();

        // Compile the WASM component.
        let compile_span = load_span.as_ref().and_then(|s| s.child("compile").start());
        let component = Component::new(&self.engine, wasm_bytes)
            .map_err(|e| anyhow::anyhow!("compiling WASM component: {e}"))?;
        drop(compile_span);

        // Set up the linker with all host imports.
        let mut linker = Linker::<PluginState>::new(&self.engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)
            .map_err(|e| anyhow::anyhow!("linking WASI imports: {e}"))?;
        bindings::Plugin::add_to_linker::<PluginState, HasSelf<PluginState>>(
            &mut linker,
            |state| state,
        )
        .map_err(|e| anyhow::anyhow!("linking plugin imports: {e}"))?;

        // Build the WASI context. Minimal sandbox: no filesystem,
        // no network, no env vars. Stdout/stderr are inherited
        // for now (plugin println! goes to the host's terminal).
        // TODO: Redirect stdout/stderr to the logging system.
        let wasi = WasiCtxBuilder::new()
            .inherit_stdout()
            .inherit_stderr()
            .build();

        let state = PluginState {
            plugin_id: plugin_id.to_string(),
            wasi,
            wasi_table: ResourceTable::new(),
            log_sender: self.log_sender.clone(),
            span_registry: Arc::clone(&self.span_registry),
            // Stashed by the bridge on `enable()` via
            // `WasmPluginInstance::set_settings`. The
            // `settings::get` host import is a no-op until
            // then; the bridge always sets it before the
            // first guest call into `enable()`.
            settings: None,
            // Populated by the bridge at construction time
            // via `WasmPluginInstance::set_sql_config` once
            // the manifest's `[storage.sql]` block (if any)
            // has been parsed and the migration files have
            // been read from the plugin source.
            sql_config: SqlConfig::None,
            sql_storage: None,
            sql_handle_reps: Vec::new(),
            // Stashed by the bridge on `enable()` via
            // `WasmPluginInstance::set_clipboard_writer`,
            // built from the AppHandle. `None` outside an
            // enable lifetime; the host import returns an
            // error in that case.
            clipboard_writer: None,
        };

        let mut store = Store::new(&self.engine, state);

        // Instantiate the component and get the typed bindings.
        let instantiate_span = load_span
            .as_ref()
            .and_then(|s| s.child("instantiate").start());
        let plugin = bindings::Plugin::instantiate(&mut store, &component, &linker)
            .map_err(|e| anyhow::anyhow!("instantiating WASM plugin: {e}"))?;
        drop(instantiate_span);

        // End the top-level load span.
        drop(load_span);

        Ok(WasmPluginInstance {
            store: Mutex::new(store),
            plugin,
            logger,
        })
    }
}

// =========================================================
// WasmPluginInstance — one per loaded plugin
// =========================================================

/// A loaded WASM plugin instance.
///
/// Wraps the wasmtime Store and typed Plugin bindings. All
/// guest calls go through the Mutex-protected Store to
/// satisfy `Send + Sync` requirements.
pub struct WasmPluginInstance {
    store: Mutex<Store<PluginState>>,
    plugin: bindings::Plugin,
    logger: Logger,
}

impl WasmPluginInstance {
    /// Stash a per-plugin `PluginSettings` handle on the
    /// store data so the `settings::get` host import can
    /// resolve reads. Called by the bridge from `enable()`
    /// before the guest's own `enable()` runs.
    ///
    /// Replacing an existing handle is allowed (re-enable
    /// after disable hands in a fresh `PluginContext`).
    pub fn set_settings(&self, settings: PluginSettings) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().settings = Some(settings);
    }

    /// Drop the stashed `PluginSettings` handle. Called by
    /// the bridge from `disable()` so the host import
    /// reverts to "unset" between enable cycles.
    pub fn clear_settings(&self) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().settings = None;
    }

    /// Install the SQL storage configuration on the store
    /// data. Called once by the bridge at construction time
    /// (before any guest call) — the migration strings have
    /// already been read from the plugin source by the
    /// caller, so this is a pure metadata stash with no
    /// I/O.
    pub fn set_sql_config(&self, config: SqlConfig) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().sql_config = config;
    }

    /// Drop every outstanding SQL handle and the cached
    /// `Arc<SqlStorage>` so the rusqlite `Connection` is
    /// fully closed on disable.
    ///
    /// Drain order matters: we delete the handle entries
    /// from `wasi_table` first (each delete drops one
    /// `Arc<SqlStorage>` clone), then clear the master
    /// `sql_storage` reference. After both steps the strong
    /// count is zero and `SqlStorage::Drop` runs, closing
    /// the connection and the SQLite file. Without this
    /// drain, a plugin that opened a handle and never
    /// explicitly dropped it would leak the connection
    /// until the entire `WasmPluginInstance` is dropped
    /// (effectively until app shutdown).
    ///
    /// `wasi_table.delete` errors (e.g., a stale rep that
    /// was already deleted) are ignored — the goal is best-
    /// effort reclaim, not strict accounting.
    pub fn clear_sql_storage(&self) {
        let mut store = self.store.lock().expect("store not poisoned");
        let data = store.data_mut();
        let reps = std::mem::take(&mut data.sql_handle_reps);
        for rep in reps {
            // `Resource::new_own(rep)` reconstructs an owned
            // resource handle from the raw rep so we can
            // hand it to `wasi_table.delete`. The original
            // `Resource` returned to the guest is gone (or
            // we wouldn't be in clear-on-disable territory),
            // but the rep alone is sufficient for the
            // table to look up and remove the entry.
            let resource: Resource<SqlHandleEntry> = Resource::new_own(rep);
            let _ = data.wasi_table.delete(resource);
        }
        data.sql_storage = None;
    }

    /// Install a closure that writes a string to the system
    /// clipboard. Called by the bridge on `enable()` from a
    /// closure that captures the `tauri::AppHandle`.
    pub fn set_clipboard_writer(&self, writer: ClipboardWriter) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().clipboard_writer = Some(writer);
    }

    /// Drop the stashed clipboard writer on `disable()` so
    /// any post-disable `clipboard::write-text` call (which
    /// shouldn't happen) errors loudly instead of silently
    /// using a stale closure.
    pub fn clear_clipboard_writer(&self) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().clipboard_writer = None;
    }

    /// Call the guest's `enable` export.
    pub fn enable(&self) -> anyhow::Result<()> {
        let _span = self.logger.span("enable").start();
        let mut store = self.store.lock().expect("store not poisoned");
        self.plugin
            .torchsnap_plugin_lifecycle()
            .call_enable(&mut *store)
            .map_err(|e| anyhow::anyhow!("calling plugin enable(): {e}"))
    }

    /// Call the guest's `on-setting-changed` export. Used by
    /// the bridge's `setting_changed` trait override after the
    /// host's `CoalescingDispatcher` has deduplicated rapid
    /// writes to the same key.
    pub fn on_setting_changed(&self, key: &str, value: &str) -> anyhow::Result<()> {
        let _span = self
            .logger
            .span("on_setting_changed")
            .meta("key", key)
            .start();
        let mut store = self.store.lock().expect("store not poisoned");
        self.plugin
            .torchsnap_plugin_lifecycle()
            .call_on_setting_changed(&mut *store, key, value)
            .map_err(|e| anyhow::anyhow!("calling plugin on_setting_changed(): {e}"))
    }

    /// Call the guest's `tasks::run-task` export.
    ///
    /// Used by the per-plugin scheduler loop to fire a
    /// scheduled task. The `task_id` matches a `[[tasks]]`
    /// entry from the manifest. The plugin dispatches by
    /// name and runs whatever work the task is supposed to
    /// do.
    ///
    /// The outer `Result` is for wasmtime trap /
    /// serialization errors; the inner
    /// `Result<(), String>` is the plugin's own
    /// success/error arm. Returning `Err(string)` is
    /// logged by the bridge — it does not auto-disable the
    /// plugin.
    #[allow(clippy::type_complexity)]
    pub fn run_task(&self, task_id: &str) -> anyhow::Result<Result<(), String>> {
        let _span = self
            .logger
            .span("run_task")
            .meta("task_id", task_id)
            .start();
        let mut store = self.store.lock().expect("store not poisoned");
        self.plugin
            .torchsnap_plugin_tasks()
            .call_run_task(&mut *store, task_id)
            .map_err(|e| anyhow::anyhow!("calling plugin run_task(): {e}"))
    }

    /// Call the guest's `messaging::handle-message` export.
    ///
    /// `payload` is a JSON-encoded string (the plugin parses
    /// it on its side). The returned outer `Result` is for
    /// wasmtime trap / serialization errors; the inner
    /// `Result<String, String>` is the plugin's own
    /// success/error arm. The success arm is the
    /// JSON-encoded response string.
    #[allow(clippy::type_complexity)]
    pub fn handle_message(
        &self,
        method: &str,
        payload: &str,
    ) -> anyhow::Result<Result<String, String>> {
        let _span = self
            .logger
            .span("handle_message")
            .meta("method", method)
            .start();
        let mut store = self.store.lock().expect("store not poisoned");
        self.plugin
            .torchsnap_plugin_messaging()
            .call_handle_message(&mut *store, method, payload)
            .map_err(|e| anyhow::anyhow!("calling plugin handle_message(): {e}"))
    }

    /// Call the guest's `disable` export.
    pub fn disable(&self) -> anyhow::Result<()> {
        let _span = self.logger.span("disable").start();
        let mut store = self.store.lock().expect("store not poisoned");
        self.plugin
            .torchsnap_plugin_lifecycle()
            .call_disable(&mut *store)
            .map_err(|e| anyhow::anyhow!("calling plugin disable(): {e}"))
    }

    /// Call the guest's `entries` export and convert to native types.
    pub fn entries(&self) -> anyhow::Result<Vec<crate::search::types::CatalogEntry>> {
        let _span = self.logger.span("entries").start();
        let mut store = self.store.lock().expect("store not poisoned");
        let wit_entries = self
            .plugin
            .torchsnap_plugin_search()
            .call_entries(&mut *store)
            .map_err(|e| anyhow::anyhow!("calling plugin entries(): {e}"))?;

        Ok(wit_entries.into_iter().map(Into::into).collect())
    }

    /// Call the guest's `search` export and convert to native types.
    pub fn search(
        &self,
        query: &str,
        matched_prefix: Option<&str>,
    ) -> anyhow::Result<crate::search::types::PluginResponse> {
        let _span = self.logger.span("search").meta("query", query).start();
        let mut store = self.store.lock().expect("store not poisoned");
        let response = self
            .plugin
            .torchsnap_plugin_search()
            .call_search(&mut *store, query, matched_prefix)
            .map_err(|e| anyhow::anyhow!("calling plugin search(): {e}"))?;

        Ok(response.into())
    }

    /// Call the guest's `execute` export and convert to native types.
    pub fn execute(
        &self,
        entry_id: &str,
        action_id: &crate::search::types::ActionId,
    ) -> anyhow::Result<crate::search::types::PostAction> {
        let _span = self
            .logger
            .span("execute")
            .meta("entry_id", entry_id)
            .start();
        let mut store = self.store.lock().expect("store not poisoned");

        let wit_action_id: bindings::exports::torchsnap::plugin::search::ActionId =
            action_id.clone().into();

        let result = self
            .plugin
            .torchsnap_plugin_search()
            .call_execute(&mut *store, entry_id, &wit_action_id)
            .map_err(|e| anyhow::anyhow!("calling plugin execute(): {e}"))?;

        match result {
            Ok(post_action) => Ok(post_action.into()),
            Err(msg) => anyhow::bail!("plugin execute() returned error: {msg}"),
        }
    }
}
