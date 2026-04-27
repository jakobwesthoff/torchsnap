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
//   WasmRuntime (one per app, owns Engine + Component cache)
//    ├── compile(plugin_id, wasm_bytes)
//    └── instantiate(plugin_id) → WasmPluginInstance
//
//   WasmPluginInstance (one per plugin, owns Store + Plugin)
//    ├── enable() / disable()
//    ├── entries() → Vec<CatalogEntry>
//    └── execute(entry_id, action_id) → PostAction
//
// The compile/instantiate split lets the expensive step run
// once per plugin while instantiation stays cheap enough to
// repeat on demand. See ADR 0033.
// =========================================================

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

pub mod host;

pub(crate) use host::clipboard::ClipboardState;
pub(crate) use host::command::CommandState;
pub(crate) use host::http::HttpState;
pub(crate) use host::opener::OpenerState;
pub use host::sql::{SqlConfig, SqlHandleEntry};
pub(crate) use host::sql::SqlState;

use super::bindings;
use super::logging::channel::LogSender;
use super::logging::spans::{Logger, SpanRegistry};
use super::logging::LogSource;
use crate::frecency::PluginFrecency;
use crate::settings::PluginSettings;

// =========================================================
// Per-Plugin Store State
//
// This is the `T` in `Store<T>`. It holds the WASI context
// and any per-plugin state the host imports need access to
// (e.g., the plugin ID for log tagging, the log sender).
// =========================================================

pub struct PluginState {
    pub(crate) plugin_id: String,
    pub(crate) wasi: WasiCtx,
    pub(crate) wasi_table: ResourceTable,
    pub(crate) log_sender: LogSender,
    pub(crate) span_registry: Arc<SpanRegistry>,
    /// Per-plugin namespaced settings reader. `None` until the
    /// bridge stashes the `PluginContext.settings` handle on
    /// `enable()`. The settings host import (`settings::get`)
    /// errors gracefully if accessed before that happens —
    /// which it shouldn't, since the host always calls
    /// `enable()` before any guest code runs.
    pub(crate) settings: Option<PluginSettings>,
    /// Per-plugin namespaced frecency reader. Same lifecycle
    /// as `settings`: stashed by the bridge on `enable()` from
    /// the `PluginContext.frecency` handle and cleared on
    /// `disable()`. The host automatically records selections
    /// before `execute()` and applies score bonuses after
    /// `search()` — this handle is only for plugins that
    /// need to read frecency state directly (e.g. to drive an
    /// empty-query browse mode).
    pub(crate) frecency: Option<PluginFrecency>,
    /// The plugin's own source handle, stashed by the bridge
    /// on `enable()` so the `assets::read` / `assets::exists`
    /// host imports can read files bundled inside the plugin
    /// archive (or development directory) without re-opening
    /// it. `None` outside an enable lifetime — the host
    /// imports return an `io-error` in that case, matching
    /// the contract of the other capability stashes.
    pub(crate) plugin_source: Option<Arc<dyn super::source::PluginSource + Send + Sync>>,
    /// Resolved `${...}` substitution variables for this plugin
    /// instance. Populated by the bridge on `enable()`; consumed
    /// by `paths::resolve` and by `command::run` rule
    /// compilation. `None` outside an enable lifetime —
    /// `paths::resolve` returns `unterminated` in that case as
    /// a placeholder for "interface not initialized" since the
    /// variant has no dedicated "uninitialized" arm.
    pub(crate) path_context: Option<super::permission_vars::PathContext>,
    /// Per-capability state grouped by host import. Each
    /// sub-struct owns the bridge-stashed data plus any
    /// permission flags / allowlists for one WIT interface.
    /// See the per-struct docs for the lifecycle contract.
    pub(crate) sql: SqlState,
    pub(crate) clipboard: ClipboardState,
    pub(crate) opener: OpenerState,
    pub(crate) http: HttpState,
    pub(crate) command: CommandState,
}

// =========================================================
// Per-capability state sub-structs
//
// One struct per host capability that owns multi-field
// state. Single-field capabilities (`settings`, `frecency`,
// `plugin_source`, `path_context`) stay flat on
// `PluginState` — wrapping a single `Option` adds ceremony
// without cohesion benefit.
//
// Each sub-struct has a `Default` impl producing the
// disabled / empty state. The bridge's `enable()` path
// populates fields directly; `disable()` resets via
// `*self = Self::default()`.
// =========================================================

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
// implement on PluginState. The trait impls live in
// `host/<capability>.rs`; this module re-exports nothing
// from them — the impls flow into the wasmtime linker via
// the bindgen-generated `add_to_linker` regardless of which
// file they're declared in.
// =========================================================

// =========================================================
// Opener host import
//
// Enforces the per-plugin scheme allowlist declared in
// `[permissions.opener]` before delegating to the
// `UrlOpenerFn` closure the bridge stashes on `enable()`.
//
// `check_opener_scheme` is a pure free function so the
// permission logic can be unit-tested without a running
// wasmtime instance.
// =========================================================


// =========================================================
// Assets host import
//
// Routes guest `assets::read` / `assets::exists` calls through
// the plugin's own `PluginSource` (stashed on the bridge on
// `enable`). Path validation runs on the host side BEFORE
// touching the source, producing the `InvalidPath` variant
// directly; that lets us skip error-string matching on the
// `anyhow::Error` the trait returns.
//
// `NotFound` is surfaced differently in each direction:
// - `read`: pre-probe with `file_exists`. Only call `read_file`
//   on hit, so the "missing" path is reported structurally.
// - `exists`: the source's `file_exists` returns `Ok(false)` on
//   miss, which maps directly to `Ok(false)` in WIT.
//
// The `read` pre-probe has a benign TOCTOU: a file that
// passes `file_exists` can be unlinked between the two calls
// (realistic only for `DirectorySource` during development;
// archive entries never race). In that narrow case the guest
// sees `IoError` rather than `NotFound`. Accept this; the
// fix would require a richer trait error enum.
//
// Any remaining error from the source — filesystem, archive
// lookup, unexpected zip variant — collapses to `IoError`.
// =========================================================

/// Map a `PluginSource` error to the `assets::io-error`
/// variant. Pure function, unit-testable without wasmtime.
// =========================================================
// WasmRuntime — shared across all plugins
// =========================================================

/// Shared WASM runtime holding the wasmtime Engine and a
/// per-plugin compiled-component cache.
///
/// One instance per application. A cached `Component` is
/// small relative to the `Store` that backs a live instance
/// — it holds native code for the guest's imports/exports
/// but no linear memory, resource tables, or per-enable
/// state — so keeping compiled components around for
/// disabled plugins is cheap.
pub struct WasmRuntime {
    engine: Engine,
    log_sender: LogSender,
    span_registry: Arc<SpanRegistry>,
    /// Keyed by plugin ID. Populated by `compile()`,
    /// consumed by `instantiate()`. Re-compiling the same
    /// ID replaces the existing entry — belt-and-suspenders
    /// for a future hot-reload path, not wired up by any
    /// current code path.
    components: Mutex<HashMap<String, Component>>,
}

impl WasmRuntime {
    /// Create a new runtime with default configuration.
    ///
    /// Returns `Arc<Self>` so every bridge can hold a cheap
    /// clone for on-demand instantiation without borrowing
    /// from the call stack that loaded it.
    pub fn new(
        log_sender: LogSender,
        span_registry: Arc<SpanRegistry>,
    ) -> anyhow::Result<Arc<Self>> {
        let mut config = Config::new();
        config.wasm_component_model(true);

        let engine =
            Engine::new(&config).map_err(|e| anyhow::anyhow!("creating wasmtime engine: {e}"))?;
        Ok(Arc::new(Self {
            engine,
            log_sender,
            span_registry,
            components: Mutex::new(HashMap::new()),
        }))
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

    /// Compile a WASM component's bytes and cache the
    /// resulting `Component` under `plugin_id`.
    ///
    /// Must be called before the first `instantiate()` for
    /// that ID. Compilation is the expensive step (parse +
    /// validate + codegen), so running it up front surfaces
    /// broken WASM as a load error and keeps `instantiate()`
    /// cheap enough to repeat on demand.
    ///
    /// Re-compiling an existing entry replaces the cached
    /// `Component` — no current code path triggers this,
    /// but the branch is kept for a future hot-reload path.
    pub fn compile(&self, plugin_id: &str, wasm_bytes: &[u8]) -> anyhow::Result<()> {
        let logger = self.logger_for(plugin_id);
        let _compile_span = logger.span("compile").meta("plugin_id", plugin_id).start();

        let component = Component::new(&self.engine, wasm_bytes)
            .map_err(|e| anyhow::anyhow!("compiling WASM component: {e}"))?;

        self.components
            .lock()
            .expect("wasm component cache not poisoned")
            .insert(plugin_id.to_string(), component);

        Ok(())
    }

    /// Instantiate a previously-compiled WASM plugin.
    ///
    /// Builds a fresh linker + `WasiCtx` + `PluginState` +
    /// `Store` against the cached `Component` and returns a
    /// ready-to-call instance. The linker is rebuilt on
    /// every call because it references `PluginState`, but
    /// linker construction is just map inserts (no WASM
    /// compilation) and `instantiate` is not in a hot path.
    ///
    /// Errors if `plugin_id` has not been compiled — that
    /// is an internal lifecycle bug, not a user-facing
    /// failure mode.
    pub fn instantiate(&self, plugin_id: &str) -> anyhow::Result<WasmPluginInstance> {
        let logger = self.logger_for(plugin_id);
        let _instantiate_span = logger
            .span("instantiate")
            .meta("plugin_id", plugin_id)
            .start();

        // Clone the `Component` out of the cache under the
        // lock (wasmtime's `Component` is an `Arc` internally
        // so cloning is cheap) and drop the guard before the
        // linker/store work, so concurrent plugin loads don't
        // block on each other.
        let component = {
            let cache = self
                .components
                .lock()
                .expect("wasm component cache not poisoned");
            cache.get(plugin_id).cloned().ok_or_else(|| {
                anyhow::anyhow!(
                    "plugin `{plugin_id}` has not been compiled — call WasmRuntime::compile first"
                )
            })?
        };

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
            // Stashed by the bridge on `enable()` via
            // `WasmPluginInstance::set_frecency`. The
            // `frecency::*` host imports gracefully degrade
            // to "disabled / empty" when the handle is
            // missing — same contract as `settings`.
            frecency: None,
            // Stashed by the bridge on `enable()` via
            // `WasmPluginInstance::set_plugin_source`. The
            // `assets::*` host imports return `io-error`
            // until then, matching the capability-stash
            // contract of the per-capability sub-structs.
            plugin_source: None,
            path_context: None,
            // Per-capability sub-structs default to their
            // disabled state. The bridge populates each one
            // on `enable()` via the corresponding
            // `WasmPluginInstance::set_*` setters; permission
            // lists are empty (deny-all) and writer / client
            // closures are `None` outside an enable lifetime,
            // so calls degrade gracefully.
            sql: SqlState::default(),
            clipboard: ClipboardState::default(),
            opener: OpenerState::default(),
            http: HttpState::default(),
            command: CommandState::default(),
        };

        let mut store = Store::new(&self.engine, state);

        // Instantiate the component and get the typed bindings.
        let plugin = bindings::Plugin::instantiate(&mut store, &component, &linker)
            .map_err(|e| anyhow::anyhow!("instantiating WASM plugin: {e}"))?;

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
    pub(crate) store: Mutex<Store<PluginState>>,
    pub(crate) plugin: bindings::Plugin,
    pub(crate) logger: Logger,
}

impl WasmPluginInstance {
    /// Apply a closure to a mutable reference to the plugin's
    /// `PluginState`, holding the store lock for its duration.
    /// The single chokepoint every capability setter routes
    /// through, so the lock-acquire / `data_mut()` pattern
    /// lives in one place rather than 30 setters.
    pub(crate) fn with_state_mut<R>(&self, f: impl FnOnce(&mut PluginState) -> R) -> R {
        let mut store = self.store.lock().expect("store not poisoned");
        f(store.data_mut())
    }
}

impl WasmPluginInstance {
    /// Stash the resolved `${...}` substitution context.
    /// Called by the bridge at `enable()` after computing the
    /// per-plugin paths.
    pub fn set_path_context(&self, ctx: super::permission_vars::PathContext) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().path_context = Some(ctx);
    }

    /// Drop the substitution context on `disable()`.
    pub fn clear_path_context(&self) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().path_context = None;
    }

    /// Stash the origin allowlist for `http::fetch`. Called by
    /// the bridge at `enable()` from the manifest's
    /// `[permissions.http].origins` list (already normalized).

    /// Install the HTTP client. Called by the bridge at
    /// `enable()`.

    /// Drop the HTTP client on `disable()` so the connection
    /// pool is released between enable cycles.

    /// Stash the plugin's own `PluginSource` handle. Called
    /// by the bridge on `enable()`. The `assets::*` host
    /// imports use this Arc to read the plugin's bundled
    /// files on demand. Same lifecycle contract as
    /// `set_http_client`.
    pub fn set_plugin_source(&self, source: Arc<dyn super::source::PluginSource + Send + Sync>) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().plugin_source = Some(source);
    }

    /// Drop the plugin source on `disable()`. Eager release
    /// so the underlying `ArchiveSource` file handle (or
    /// the `DirectorySource` path) doesn't linger across
    /// enable cycles.
    pub fn clear_plugin_source(&self) {
        let mut store = self.store.lock().expect("store not poisoned");
        store.data_mut().plugin_source = None;
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

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
impl PluginState {
    /// Construct a `PluginState` with all capability fields set
    /// to their "absent" defaults. Tests override specific fields
    /// with struct update syntax (`..PluginState::default_for_test()`).
    fn default_for_test() -> Self {
        let wasi = WasiCtxBuilder::new().build();
        PluginState {
            plugin_id: "test-plugin".to_string(),
            wasi,
            wasi_table: ResourceTable::new(),
            log_sender: LogSender::test_sender(),
            span_registry: Arc::new(SpanRegistry::new()),
            settings: None,
            frecency: None,
            plugin_source: None,
            path_context: None,
            sql: SqlState::default(),
            clipboard: ClipboardState::default(),
            opener: OpenerState::default(),
            http: HttpState::default(),
            command: CommandState::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes of the committed `minimal-plugin` fixture. The
    /// fixture is a standalone `wasm32-wasip2` crate under
    /// `src-tauri/tests/fixtures/minimal-plugin/` whose guest
    /// implements every WIT export as a no-op. See that
    /// directory's README for rebuild instructions.
    const MINIMAL_PLUGIN_WASM: &[u8] =
        include_bytes!("../../../tests/fixtures/minimal-plugin/minimal_plugin.wasm");

    /// Construct a bare `WasmRuntime` for tests. Uses a
    /// discarding `LogSender` and a fresh `SpanRegistry` so
    /// tests do not depend on a running logging task.
    fn test_runtime() -> Arc<WasmRuntime> {
        WasmRuntime::new(LogSender::test_sender(), Arc::new(SpanRegistry::new()))
            .expect("WasmRuntime::new should succeed with default config")
    }

    #[test]
    fn compile_then_instantiate_succeeds() {
        let runtime = test_runtime();
        runtime
            .compile("minimal", MINIMAL_PLUGIN_WASM)
            .expect("compile valid fixture");
        let instance = runtime.instantiate("minimal").expect("instantiate");
        instance.enable().expect("guest enable no-op");
    }

    #[test]
    fn compile_caches_component() {
        // No direct recompilation counter to assert on; we
        // verify the cache retains the entry after two
        // successful instantiates instead.
        let runtime = test_runtime();
        runtime
            .compile("minimal", MINIMAL_PLUGIN_WASM)
            .expect("compile");
        let _first = runtime.instantiate("minimal").expect("first instantiate");
        let _second = runtime.instantiate("minimal").expect("second instantiate");

        assert!(
            runtime
                .components
                .lock()
                .expect("cache lock")
                .contains_key("minimal")
        );
    }

    #[test]
    fn instantiate_without_compile_errors() {
        let runtime = test_runtime();
        let err = runtime
            .instantiate("never-compiled")
            .err()
            .expect("instantiate returns Err for unknown id");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("never-compiled") && msg.contains("compile"),
            "error should name the missing plugin and hint at `compile` (got: {msg})"
        );
    }

    #[test]
    fn compile_invalid_bytes_errors() {
        // Both checks matter: the return is an `Err` AND the
        // cache stays empty, so a subsequent `instantiate`
        // cannot silently succeed against a partial entry.
        let runtime = test_runtime();
        assert!(
            runtime
                .compile("garbage", b"not a wasm component at all")
                .is_err()
        );
        assert!(
            !runtime
                .components
                .lock()
                .expect("cache lock")
                .contains_key("garbage")
        );
        assert!(runtime.instantiate("garbage").is_err());
    }

    #[test]
    fn compile_replaces_existing_entry() {
        // Pins the "re-compile replaces" semantic so a future
        // refactor that accidentally rejects duplicate compiles
        // would trip this assertion.
        let runtime = test_runtime();
        runtime
            .compile("minimal", MINIMAL_PLUGIN_WASM)
            .expect("first compile");
        runtime
            .compile("minimal", MINIMAL_PLUGIN_WASM)
            .expect("second compile replaces");
        runtime.instantiate("minimal").expect("instantiate");
    }

    #[test]
    fn instances_are_independent() {
        // Two instances from the same cached Component must
        // have disjoint PluginStates: operating on one must
        // not disturb the other.
        let runtime = test_runtime();
        runtime
            .compile("minimal", MINIMAL_PLUGIN_WASM)
            .expect("compile");

        let first = runtime.instantiate("minimal").expect("first");
        let second = runtime.instantiate("minimal").expect("second");

        first.clear_sql_storage();
        second
            .enable()
            .expect("second instance untouched by operations on the first");
    }

    /// Bytes of the committed `opener-http-plugin` fixture.
    /// Exercises the `opener` and `http` host interfaces via
    /// `messaging::handle-message` dispatch.
    const OPENER_HTTP_PLUGIN_WASM: &[u8] =
        include_bytes!("../../../tests/fixtures/opener-http-plugin/opener_http_plugin.wasm");

    /// Bytes of the committed `assets-plugin` fixture.
    /// Exercises the `assets` host interface via
    /// `messaging::handle-message` dispatch.
    const ASSETS_PLUGIN_WASM: &[u8] =
        include_bytes!("../../../tests/fixtures/assets-plugin/assets_plugin.wasm");

    /// Bytes of the committed `command-plugin` fixture.
    /// Exercises the `command` host interface via
    /// `messaging::handle-message` dispatch.
    #[cfg(unix)]
    const COMMAND_PLUGIN_WASM: &[u8] =
        include_bytes!("../../../tests/fixtures/command-plugin/command_plugin.wasm");

    // =========================================================
    // Unit tests for opener/http pure functions
    //
    // These functions have no WASM or tokio dependency so the
    // tests are fast synchronous assertions.
    // =========================================================

    fn strs(ss: &[&str]) -> Vec<String> {
        ss.iter().map(|s| s.to_string()).collect()
    }

    // ---- check_opener_scheme --------------------------------

    use super::host::opener::{check_opener_scheme, OpenerSchemeCheckError};

    #[test]
    fn opener_permitted_scheme_passes() {
        assert!(check_opener_scheme(&strs(&["https"]), "https://example.com").is_ok());
    }

    #[test]
    fn opener_forbidden_scheme_blocked() {
        let err = check_opener_scheme(&strs(&["https"]), "ftp://example.com").unwrap_err();
        match err {
            OpenerSchemeCheckError::SchemeNotPermitted(scheme) => assert_eq!(scheme, "ftp"),
            other => panic!("expected SchemeNotPermitted, got {other:?}"),
        }
    }

    #[test]
    fn opener_empty_allowlist_denies_everything() {
        assert!(check_opener_scheme(&[], "https://example.com").is_err());
    }

    #[test]
    fn opener_unparseable_url_returns_error() {
        assert!(check_opener_scheme(&strs(&["https"]), "not-a-url").is_err());
    }

    #[test]
    fn opener_scheme_check_is_case_insensitive() {
        // The `url` crate normalizes schemes to lowercase, so an
        // uppercase scheme in the URL still matches the allowlist.
        assert!(check_opener_scheme(&strs(&["https"]), "HTTPS://example.com").is_ok());
    }

    #[test]
    fn opener_multiple_schemes_second_matches() {
        assert!(check_opener_scheme(&strs(&["https", "mailto"]), "mailto:user@x.com").is_ok());
    }

    // ---- check_http_origin ----------------------------------

    use super::host::http::{check_http_origin, wit_method_to_reqwest, WasmHttpError};

    #[test]
    fn http_exact_origin_match_passes() {
        assert!(
            check_http_origin(&strs(&["https://example.com"]), "https://example.com/path").is_ok()
        );
    }

    #[test]
    fn http_non_matching_origin_denied() {
        let err =
            check_http_origin(&strs(&["https://example.com"]), "https://other.com/x").unwrap_err();
        assert!(matches!(err, WasmHttpError::PermissionDenied(_)));
    }

    #[test]
    fn http_wildcard_allows_any_origin() {
        assert!(check_http_origin(&strs(&["*"]), "https://any-host.example/path").is_ok());
    }

    #[test]
    fn http_empty_origins_denies_everything() {
        assert!(check_http_origin(&[], "https://example.com").is_err());
    }

    #[test]
    fn http_non_default_port_included_in_origin() {
        assert!(
            check_http_origin(
                &strs(&["https://example.com:8443"]),
                "https://example.com:8443/resource"
            )
            .is_ok()
        );
    }

    #[test]
    fn http_default_port_stripped_from_origin() {
        // https:443 normalizes to the same origin as https (no port).
        assert!(
            check_http_origin(
                &strs(&["https://example.com"]),
                "https://example.com:443/resource"
            )
            .is_ok()
        );
    }

    #[test]
    fn http_wrong_port_is_different_origin() {
        let err = check_http_origin(
            &strs(&["https://example.com"]),
            "https://example.com:8080/x",
        )
        .unwrap_err();
        assert!(matches!(err, WasmHttpError::PermissionDenied(_)));
    }

    #[test]
    fn http_unparseable_url_returns_network_error() {
        let err = check_http_origin(&strs(&["https://example.com"]), "not-a-url").unwrap_err();
        assert!(matches!(err, WasmHttpError::Network(_)));
    }

    #[test]
    fn http_wildcard_short_circuits_before_url_parse() {
        // The wildcard check happens before `url::Url::parse`, so an
        // unparseable URL is still allowed when the list contains `"*"`.
        assert!(check_http_origin(&strs(&["*"]), "not-a-url").is_ok());
    }

    // ---- wit_method_to_reqwest ------------------------------

    #[test]
    fn wit_method_named_variants_map_correctly() {
        use bindings::torchsnap::plugin::http::HttpMethod;
        let cases = [
            (HttpMethod::Get, reqwest::Method::GET),
            (HttpMethod::Post, reqwest::Method::POST),
            (HttpMethod::Put, reqwest::Method::PUT),
            (HttpMethod::Patch, reqwest::Method::PATCH),
            (HttpMethod::Delete, reqwest::Method::DELETE),
            (HttpMethod::Head, reqwest::Method::HEAD),
        ];
        for (wit, expected) in cases {
            assert_eq!(wit_method_to_reqwest(wit).unwrap(), expected,);
        }
    }

    #[test]
    fn wit_method_other_valid_string() {
        use bindings::torchsnap::plugin::http::HttpMethod;
        let method = wit_method_to_reqwest(HttpMethod::Other("PROPFIND".into())).unwrap();
        assert_eq!(method.as_str(), "PROPFIND");
    }

    #[test]
    fn wit_method_other_invalid_string_returns_network_error() {
        use bindings::torchsnap::plugin::http::HttpMethod;
        let err = wit_method_to_reqwest(HttpMethod::Other("has space".into())).unwrap_err();
        assert!(matches!(err, WasmHttpError::Network(_)));
    }

    // ---- WasmHttpError → HttpError conversion ---------------

    #[test]
    fn wasm_http_error_conversion_all_variants() {
        use bindings::torchsnap::plugin::http::HttpError;
        let cases: Vec<(WasmHttpError, HttpError)> = vec![
            (
                WasmHttpError::PermissionDenied("origin".into()),
                HttpError::PermissionDenied("origin".into()),
            ),
            (WasmHttpError::Timeout, HttpError::Timeout),
            (
                WasmHttpError::Network("oops".into()),
                HttpError::Network("oops".into()),
            ),
        ];
        for (input, expected) in cases {
            let converted: HttpError = input.into();
            assert!(
                std::mem::discriminant(&converted) == std::mem::discriminant(&expected),
                "discriminant mismatch"
            );
        }
    }

    // ---- httpmock integration tests for http::fetch ---------
    //
    // These tests exercise the full `Http` builder pipeline in
    // `http::Host::fetch` — specifically that headers, body,
    // status, and response headers are forwarded correctly.
    // They don't need a live WASM instance; they call the
    // private pieces directly.

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_forwards_request_headers() {
        use httpmock::MockServer;
        use httpmock::prelude::*;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/test")
                .header("x-custom", "value123");
            then.status(200).body("ok");
        });

        let client = crate::network::Http::new();
        let request = bindings::torchsnap::plugin::http::HttpRequest {
            url: server.url("/test"),
            method: bindings::torchsnap::plugin::http::HttpMethod::Get,
            headers: vec![("x-custom".into(), "value123".into())],
            body: None,
            timeout_ms: None,
            max_body_size: None,
        };

        let mut state = PluginState {
            http: HttpState {
                origins: vec!["*".into()],
                client: Some(Arc::new(client)),
            },
            ..PluginState::default_for_test()
        };

        use bindings::torchsnap::plugin::http::Host;
        let resp = state.fetch(request).expect("fetch should succeed");
        assert_eq!(resp.status, 200);
        mock.assert();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_forwards_request_body() {
        use httpmock::MockServer;
        use httpmock::prelude::*;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/data").body("hello");
            then.status(201);
        });

        let client = crate::network::Http::new();
        let request = bindings::torchsnap::plugin::http::HttpRequest {
            url: server.url("/data"),
            method: bindings::torchsnap::plugin::http::HttpMethod::Post,
            headers: vec![],
            body: Some(b"hello".to_vec()),
            timeout_ms: None,
            max_body_size: None,
        };

        let mut state = PluginState {
            http: HttpState {
                origins: vec!["*".into()],
                client: Some(Arc::new(client)),
            },
            ..PluginState::default_for_test()
        };

        use bindings::torchsnap::plugin::http::Host;
        let resp = state.fetch(request).expect("fetch should succeed");
        assert_eq!(resp.status, 201);
        mock.assert();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_returns_response_status_and_headers() {
        use httpmock::MockServer;
        use httpmock::prelude::*;

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/head-test");
            then.status(404).header("x-resp-header", "present");
        });

        let client = crate::network::Http::new();
        let request = bindings::torchsnap::plugin::http::HttpRequest {
            url: server.url("/head-test"),
            method: bindings::torchsnap::plugin::http::HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout_ms: None,
            max_body_size: None,
        };

        let mut state = PluginState {
            http: HttpState {
                origins: vec!["*".into()],
                client: Some(Arc::new(client)),
            },
            ..PluginState::default_for_test()
        };

        use bindings::torchsnap::plugin::http::Host;
        let resp = state.fetch(request).expect("fetch should succeed");
        assert_eq!(resp.status, 404);
        let has_header = resp
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("x-resp-header") && v == "present");
        assert!(has_header, "expected x-resp-header in response headers");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_returns_response_body() {
        use httpmock::MockServer;
        use httpmock::prelude::*;

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/body");
            then.status(200).body(b"hello body".as_slice());
        });

        let client = crate::network::Http::new();
        let request = bindings::torchsnap::plugin::http::HttpRequest {
            url: server.url("/body"),
            method: bindings::torchsnap::plugin::http::HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout_ms: None,
            max_body_size: None,
        };

        let mut state = PluginState {
            http: HttpState {
                origins: vec!["*".into()],
                client: Some(Arc::new(client)),
            },
            ..PluginState::default_for_test()
        };

        use bindings::torchsnap::plugin::http::Host;
        let resp = state.fetch(request).expect("fetch should succeed");
        assert_eq!(resp.body, b"hello body");
    }

    // =========================================================
    // WASM integration tests for opener/http via fixture plugin
    //
    // These tests run the actual `opener-http-plugin` fixture
    // WASM and exercise `opener::open-url` and `http::fetch`
    // end-to-end through `handle_message` dispatch, with a mock
    // opener closure and a `httpmock` HTTP server respectively.
    // =========================================================

    fn compile_opener_http_fixture() -> (Arc<WasmRuntime>, WasmPluginInstance) {
        let runtime = test_runtime();
        runtime
            .compile("opener-http-plugin", OPENER_HTTP_PLUGIN_WASM)
            .expect("compile opener-http fixture");
        let instance = runtime
            .instantiate("opener-http-plugin")
            .expect("instantiate opener-http fixture");
        (runtime, instance)
    }

    #[test]
    fn opener_permitted_scheme_calls_writer() {
        let (_runtime, instance) = compile_opener_http_fixture();

        let called_url: std::sync::Arc<Mutex<Option<String>>> =
            std::sync::Arc::new(Mutex::new(None));
        let called_url_clone = called_url.clone();

        instance.set_opener_schemes(vec!["https".into()]);
        instance.set_opener_writer(Box::new(move |url: &str| {
            *called_url_clone.lock().expect("not poisoned") = Some(url.to_string());
            Ok(())
        }));

        instance.enable().expect("enable");

        let result = instance
            .handle_message("opener.open-url", "https://example.com")
            .expect("handle_message call succeeded")
            .expect("guest returned Ok");

        assert_eq!(result, "ok");
        assert_eq!(
            called_url.lock().expect("not poisoned").as_deref(),
            Some("https://example.com")
        );
    }

    #[test]
    fn opener_forbidden_scheme_returns_error() {
        let (_runtime, instance) = compile_opener_http_fixture();

        instance.set_opener_schemes(vec!["https".into()]);
        instance.set_opener_writer(Box::new(|_: &str| Ok(())));

        instance.enable().expect("enable");

        let result = instance
            .handle_message("opener.open-url", "ftp://example.com")
            .expect("handle_message call succeeded");

        let err = result.expect_err("guest should return Err for ftp://");
        assert!(
            err.contains("scheme not permitted: ftp"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn opener_writer_not_set_returns_error() {
        let (_runtime, instance) = compile_opener_http_fixture();

        instance.set_opener_schemes(vec!["https".into()]);
        // Intentionally omit set_opener_writer.

        instance.enable().expect("enable");

        let result = instance
            .handle_message("opener.open-url", "https://example.com")
            .expect("handle_message call succeeded");

        let err = result.expect_err("guest should return Err when writer missing");
        assert!(
            err.contains("opener not initialized"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn http_permitted_origin_proceeds() {
        use httpmock::MockServer;
        use httpmock::prelude::*;

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/ping");
            then.status(200).body("pong");
        });

        let (_runtime, instance) = compile_opener_http_fixture();
        instance.set_http_origins(vec!["*".into()]);
        instance.set_http_client(Arc::new(crate::network::Http::new()));

        instance.enable().expect("enable");

        let result = instance
            .handle_message("http.get", &server.url("/ping"))
            .expect("handle_message call succeeded")
            .expect("guest returned Ok");

        assert_eq!(result, "status:200");
    }

    #[test]
    fn http_blocked_origin_returns_permission_denied() {
        let (_runtime, instance) = compile_opener_http_fixture();
        // Empty origins = deny all.
        instance.set_http_origins(vec![]);
        instance.set_http_client(Arc::new(crate::network::Http::new()));

        instance.enable().expect("enable");

        let result = instance
            .handle_message("http.get", "https://example.com/test")
            .expect("handle_message call succeeded");

        let err = result.expect_err("guest should return Err for blocked origin");
        assert!(err.contains("PermissionDenied"), "unexpected error: {err}");
    }

    // =========================================================
    // Assets host import — unit tests
    //
    // These drive the `assets::Host` impl directly against a
    // `PluginState` built with `default_for_test` and a
    // `DirectorySource` stashed on `plugin_source`. They cover
    // each variant of `AssetsError` plus the happy paths for
    // both `read` and `exists`. The fixture-driven integration
    // tests (step 1f) exercise the same paths through a real
    // WASM guest call.
    // =========================================================

    fn make_plugin_source_dir() -> (
        tempfile::TempDir,
        Arc<dyn super::super::source::PluginSource + Send + Sync>,
    ) {
        use super::super::source::DirectorySource;

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();

        std::fs::write(
            root.join("manifest.toml"),
            r#"
[plugin]
id = "assets-unit-plugin"
name = "Assets Unit Plugin"
description = "Fixture for runtime unit tests"
version = "0.0.0"
wasm = "plugin.wasm"
icon = "heroicons:beaker"
"#,
        )
        .expect("write manifest");
        std::fs::write(root.join("plugin.wasm"), b"wasm").expect("write wasm");
        std::fs::write(root.join("greeting.txt"), b"hello from the fixture\n")
            .expect("write greeting");
        std::fs::create_dir_all(root.join("data")).expect("mkdir data");
        std::fs::write(root.join("data/payload.bin"), [0u8, 1, 2, 3, 255]).expect("write payload");

        let src: Arc<dyn super::super::source::PluginSource + Send + Sync> =
            Arc::new(DirectorySource::open(root).expect("open"));
        (dir, src)
    }

    fn state_with_source(
        src: Arc<dyn super::super::source::PluginSource + Send + Sync>,
    ) -> PluginState {
        PluginState {
            plugin_source: Some(src),
            ..PluginState::default_for_test()
        }
    }

    #[test]
    fn assets_read_returns_bytes_for_existing_file() {
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let bytes = state
            .read("greeting.txt".to_string())
            .expect("read succeeds");
        assert_eq!(bytes, b"hello from the fixture\n");
    }

    #[test]
    fn assets_read_preserves_binary_content() {
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let bytes = state
            .read("data/payload.bin".to_string())
            .expect("read succeeds");
        assert_eq!(bytes, [0u8, 1, 2, 3, 255]);
    }

    #[test]
    fn assets_read_returns_not_found_for_missing_file() {
        use bindings::torchsnap::plugin::assets::AssetsError;
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let err = state
            .read("not-here.json".to_string())
            .expect_err("missing file returns Err");
        assert!(
            matches!(err, AssetsError::NotFound),
            "expected NotFound, got {err:?}"
        );
    }

    #[test]
    fn assets_read_rejects_traversal_path() {
        use bindings::torchsnap::plugin::assets::AssetsError;
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let err = state
            .read("../../../etc/passwd".to_string())
            .expect_err("traversal rejected");
        match err {
            AssetsError::InvalidPath(msg) => assert!(
                msg.contains("escapes"),
                "InvalidPath message should mention escape (got: {msg})"
            ),
            other => panic!("expected InvalidPath, got {other:?}"),
        }
    }

    #[test]
    fn assets_read_rejects_absolute_path() {
        use bindings::torchsnap::plugin::assets::AssetsError;
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let err = state
            .read("/etc/passwd".to_string())
            .expect_err("absolute rejected");
        match err {
            AssetsError::InvalidPath(msg) => assert!(
                msg.contains("relative"),
                "InvalidPath message should mention relative (got: {msg})"
            ),
            other => panic!("expected InvalidPath, got {other:?}"),
        }
    }

    #[test]
    fn assets_read_rejects_empty_path() {
        use bindings::torchsnap::plugin::assets::AssetsError;
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let err = state.read("".to_string()).expect_err("empty rejected");
        assert!(matches!(err, AssetsError::InvalidPath(_)));
    }

    #[test]
    fn assets_read_rejects_nul_byte_path() {
        use bindings::torchsnap::plugin::assets::AssetsError;
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let err = state
            .read("data\0hidden".to_string())
            .expect_err("NUL rejected");
        assert!(matches!(err, AssetsError::InvalidPath(_)));
    }

    #[test]
    fn assets_read_without_plugin_source_returns_io_error() {
        use bindings::torchsnap::plugin::assets::AssetsError;
        use bindings::torchsnap::plugin::assets::Host;

        // Default state has `plugin_source: None` — mirrors
        // calling an asset import outside an enable lifetime.
        let mut state = PluginState::default_for_test();
        let err = state
            .read("greeting.txt".to_string())
            .expect_err("uninitialized returns Err");
        match err {
            AssetsError::IoError(msg) => assert!(
                msg.contains("not initialized"),
                "expected `not initialized` message, got: {msg}"
            ),
            other => panic!("expected IoError, got {other:?}"),
        }
    }

    #[test]
    fn assets_exists_returns_true_when_present() {
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        assert!(
            state
                .exists("greeting.txt".to_string())
                .expect("probe succeeds")
        );
    }

    #[test]
    fn assets_exists_returns_false_when_absent() {
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        assert!(
            !state
                .exists("not-here.json".to_string())
                .expect("probe succeeds")
        );
    }

    #[test]
    fn assets_exists_rejects_traversal_path() {
        use bindings::torchsnap::plugin::assets::AssetsError;
        use bindings::torchsnap::plugin::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let err = state
            .exists("../../../etc/passwd".to_string())
            .expect_err("traversal rejected");
        assert!(matches!(err, AssetsError::InvalidPath(_)));
    }

    #[test]
    fn assets_exists_without_plugin_source_returns_io_error() {
        use bindings::torchsnap::plugin::assets::AssetsError;
        use bindings::torchsnap::plugin::assets::Host;

        let mut state = PluginState::default_for_test();
        let err = state
            .exists("greeting.txt".to_string())
            .expect_err("uninitialized returns Err");
        assert!(matches!(err, AssetsError::IoError(_)));
    }

    #[test]
    fn assets_into_io_error_includes_full_chain() {
        // `anyhow::Error::chain` — make sure the `{e:#}`
        // formatting captures the `.context()` prefix so
        // host logs stay diagnostic.
        let err = anyhow::anyhow!("root cause").context("while doing X");
        let mapped = host::assets::into_assets_io_error(err);
        match mapped {
            bindings::torchsnap::plugin::assets::AssetsError::IoError(msg) => {
                assert!(msg.contains("while doing X"), "missing context: {msg}");
                assert!(msg.contains("root cause"), "missing root: {msg}");
            }
            other => panic!("expected IoError, got {other:?}"),
        }
    }

    // =========================================================
    // WASM integration tests for assets via fixture plugin
    //
    // The assets fixture directory contains `greeting.txt` at
    // the root and `data/payload.bin` nested. These tests
    // compile the fixture, wire it to a `DirectorySource`
    // pointing at the fixture directory, and exercise
    // `assets::read` / `assets::exists` end-to-end through
    // `handle_message` dispatch.
    // =========================================================

    fn compile_assets_fixture() -> (Arc<WasmRuntime>, WasmPluginInstance) {
        let runtime = test_runtime();
        runtime
            .compile("assets-plugin", ASSETS_PLUGIN_WASM)
            .expect("compile assets fixture");
        let instance = runtime
            .instantiate("assets-plugin")
            .expect("instantiate assets fixture");
        (runtime, instance)
    }

    /// Build a `PluginSource` pointing at the committed
    /// assets-plugin fixture directory. This mirrors what
    /// the bridge does in production: `DirectorySource::open`
    /// on the plugin root, wrapped in an `Arc`, then stashed
    /// on the instance via `set_plugin_source`.
    fn assets_fixture_source() -> Arc<dyn super::super::source::PluginSource + Send + Sync> {
        use super::super::source::DirectorySource;
        let fixture_root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/assets-plugin");
        Arc::new(DirectorySource::open(fixture_root).expect("open assets fixture"))
    }

    #[test]
    fn wasm_assets_read_returns_bundled_file_contents() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.read", "greeting.txt")
            .expect("dispatch succeeded")
            .expect("guest returned Ok");

        assert!(
            result.contains("Hello from the assets fixture"),
            "unexpected body: {result}"
        );
    }

    #[test]
    fn wasm_assets_read_binary_preserves_byte_count() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        // `data/payload.bin` was written with a known
        // length; verify the full byte count crosses the
        // WIT boundary without truncation.
        let result = instance
            .handle_message("assets.read-len", "data/payload.bin")
            .expect("dispatch succeeded")
            .expect("guest returned Ok");

        let expected_len = std::fs::metadata(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/assets-plugin/data/payload.bin"),
        )
        .expect("stat fixture")
        .len();

        assert_eq!(result, format!("len:{expected_len}"));
    }

    #[test]
    fn wasm_assets_exists_true_for_bundled_file() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.exists", "greeting.txt")
            .expect("dispatch succeeded")
            .expect("guest returned Ok");
        assert_eq!(result, "true");
    }

    #[test]
    fn wasm_assets_exists_false_for_missing_file() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.exists", "not-here.txt")
            .expect("dispatch succeeded")
            .expect("guest returned Ok");
        assert_eq!(result, "false");
    }

    #[test]
    fn wasm_assets_exists_false_for_nested_missing_file() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.exists", "data/not-here.txt")
            .expect("dispatch succeeded")
            .expect("guest returned Ok");
        assert_eq!(result, "false");
    }

    #[test]
    fn wasm_assets_read_rejects_traversal() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.read", "../../../etc/passwd")
            .expect("dispatch succeeded");
        let err = result.expect_err("traversal must error");
        assert!(
            err.contains("InvalidPath"),
            "expected InvalidPath variant in guest debug output, got: {err}"
        );
    }

    #[test]
    fn wasm_assets_read_rejects_absolute_path() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.read", "/etc/passwd")
            .expect("dispatch succeeded");
        let err = result.expect_err("absolute must error");
        assert!(
            err.contains("InvalidPath"),
            "expected InvalidPath, got: {err}"
        );
    }

    #[test]
    fn wasm_assets_read_returns_not_found_for_missing_file() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.read", "not-present.json")
            .expect("dispatch succeeded");
        let err = result.expect_err("missing must error");
        assert!(
            err.contains("NotFound"),
            "expected NotFound variant in guest debug output, got: {err}"
        );
    }

    #[test]
    fn wasm_assets_read_succeeds_on_nested_path() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.read", "data/payload.bin")
            .expect("dispatch succeeded")
            .expect("guest returned Ok");
        assert!(
            result.contains("nested binary data for tests"),
            "unexpected body: {result}"
        );
    }

    #[test]
    fn wasm_assets_calls_without_plugin_source_return_io_error() {
        // Skip `set_plugin_source` — same shape the bridge
        // would produce if it forgot to stash the source
        // on enable. The host returns `IoError` and the
        // guest bubbles the debug form up to our caller.
        let (_runtime, instance) = compile_assets_fixture();
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.read", "greeting.txt")
            .expect("dispatch succeeded");
        let err = result.expect_err("uninit must error");
        assert!(err.contains("IoError"), "expected IoError, got: {err}");
    }

    // =========================================================
    // Command host import — integration tests
    //
    // These drive the full `command::run` impl through the
    // `command-plugin` fixture, which dispatches the test
    // scenarios from its `messaging::handle-message` impl.
    // Unix-only: the fixture's manifest grants `/bin/echo` and
    // `/bin/sh -c <script>` rules, both of which require Unix
    // utilities. A Windows port would need its own fixture
    // with rules over `cmd.exe` / PowerShell.
    // =========================================================

    #[cfg(unix)]
    fn compile_command_fixture() -> (Arc<WasmRuntime>, WasmPluginInstance, tempfile::TempDir) {
        use super::super::argv_matcher::{CompiledArgvConstraint, CompiledCommandRule};
        use super::super::permission_vars::PathContext;

        let runtime = test_runtime();
        runtime
            .compile("command-plugin", COMMAND_PLUGIN_WASM)
            .expect("compile command fixture");
        let instance = runtime
            .instantiate("command-plugin")
            .expect("instantiate command fixture");

        // Stash a path context so the default cwd resolution
        // (`<plugin-data>/exec-cwd/`) has somewhere real to
        // create. The tempdir lives on so the test's child
        // process actually has a valid cwd at spawn time.
        let scratch = tempfile::TempDir::new().expect("scratch tempdir");
        instance.set_path_context(PathContext {
            plugin_data: scratch.path().to_path_buf(),
            plugin_archive: scratch.path().to_path_buf(),
            home: scratch.path().to_path_buf(),
            xdg_config: scratch.path().to_path_buf(),
            xdg_data: scratch.path().to_path_buf(),
        });

        // Compile rules matching the fixture's manifest:
        //   /bin/echo with [{any-string}]
        //   /bin/sh   with [{literal:"-c"}, {any-string}]
        // The fixture's `handle-message` dispatches into
        // these via the four test methods.
        instance.set_command_rules(vec![
            CompiledCommandRule {
                binary: "/bin/echo".to_string(),
                argv: vec![CompiledArgvConstraint::AnyString],
            },
            CompiledCommandRule {
                binary: "/bin/sh".to_string(),
                argv: vec![
                    CompiledArgvConstraint::Literal("-c".to_string()),
                    CompiledArgvConstraint::AnyString,
                ],
            },
        ]);

        (runtime, instance, scratch)
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn command_allowed_call_returns_stdout() {
        let (_runtime, instance, _scratch) = compile_command_fixture();
        instance.enable().expect("enable");

        let result = instance
            .handle_message("echo", "hello")
            .expect("handle_message call succeeded")
            .expect("guest returned Ok");

        // /bin/echo emits the argument plus a trailing newline.
        assert_eq!(result.trim(), "hello");
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn command_denied_binary_returns_permission_denied() {
        let (_runtime, instance, _scratch) = compile_command_fixture();
        instance.enable().expect("enable");

        let result = instance
            .handle_message("deny:/bin/cat", "")
            .expect("handle_message call succeeded");

        let err = result.expect_err("guest should return Err for unlisted binary");
        assert!(
            err.contains("PermissionDenied"),
            "expected permission-denied, got: {err}"
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn command_timeout_returns_timeout_variant() {
        let (_runtime, instance, _scratch) = compile_command_fixture();
        instance.enable().expect("enable");

        // Fixture sends `/bin/sh -c "sleep 5"` with a 150ms
        // timeout. The host should kill the child via
        // SIGTERM/SIGKILL on the process group and surface
        // `Timeout`. Anything longer than ~1s would mean the
        // grace period failed.
        let started = std::time::Instant::now();
        let result = instance
            .handle_message("sleep:5", "")
            .expect("handle_message call succeeded");
        let elapsed = started.elapsed();

        let err = result.expect_err("guest should return Err on timeout");
        assert!(
            err.contains("Timeout"),
            "expected Timeout variant, got: {err}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "kill grace took too long: {elapsed:?}"
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn command_output_overflow_returns_too_large() {
        let (_runtime, instance, _scratch) = compile_command_fixture();
        instance.enable().expect("enable");

        // Fixture sends `/bin/sh -c "head -c 1024 /dev/zero"`
        // with a 64-byte output cap. The host should kill the
        // child and surface `OutputTooLarge` carrying the
        // bytes captured up to the cap.
        let result = instance
            .handle_message("flood:1024", "")
            .expect("handle_message call succeeded");

        let err = result.expect_err("guest should return Err on output overflow");
        assert!(
            err.contains("OutputTooLarge"),
            "expected OutputTooLarge variant, got: {err}"
        );
    }
}
