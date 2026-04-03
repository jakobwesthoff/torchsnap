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

use std::sync::Mutex;
use std::time::{Instant, SystemTime};

use wasmtime::component::{Component, HasSelf, Linker, ResourceTable};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

use super::bindings;
use super::logging::channel::LogSender;
use super::logging::{LogEntry, LogLevel, LogSource};

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
    ) {
        let log_level = match level {
            bindings::torchsnap::plugin::logging::LogLevel::Trace => LogLevel::Trace,
            bindings::torchsnap::plugin::logging::LogLevel::Debug => LogLevel::Debug,
            bindings::torchsnap::plugin::logging::LogLevel::Info => LogLevel::Info,
            bindings::torchsnap::plugin::logging::LogLevel::Warn => LogLevel::Warn,
            bindings::torchsnap::plugin::logging::LogLevel::Error => LogLevel::Error,
        };

        self.log_sender.send(LogEntry {
            seq: 0,
            timestamp: SystemTime::now(),
            level: log_level,
            source: LogSource::Plugin(self.plugin_id.clone()),
            message,
            metadata: vec![],
            span_id: None,
            span: None,
        });
    }
}

// The `types` interface is imported but contains only type
// definitions (records, enums, variants) — no functions.
// wasmtime still generates a Host trait for it, but the
// implementation is empty.
impl bindings::torchsnap::plugin::types::Host for PluginState {}

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
}

impl WasmRuntime {
    /// Create a new runtime with default configuration.
    pub fn new(log_sender: LogSender) -> anyhow::Result<Self> {
        let mut config = Config::new();
        config.wasm_component_model(true);

        let engine =
            Engine::new(&config).map_err(|e| anyhow::anyhow!("creating wasmtime engine: {e}"))?;
        Ok(Self { engine, log_sender })
    }

    /// Emit a host-level log entry for WASM runtime operations
    /// (compilation, instantiation, loading).
    fn log(&self, level: LogLevel, plugin_id: &str, message: String) {
        self.log_sender.send(LogEntry {
            seq: 0,
            timestamp: SystemTime::now(),
            level,
            source: LogSource::Plugin(plugin_id.to_string()),
            message,
            metadata: vec![],
            span_id: None,
            span: None,
        });
    }

    /// Instantiate a WASM plugin from raw component bytes.
    ///
    /// Compiles the component, links host imports (WASI +
    /// plugin interfaces), and creates a ready-to-call
    /// instance.
    pub fn instantiate(
        &self,
        plugin_id: &str,
        wasm_bytes: &[u8],
    ) -> anyhow::Result<WasmPluginInstance> {
        let total_start = Instant::now();

        // Compile the WASM component.
        let compile_start = Instant::now();
        let component = Component::new(&self.engine, wasm_bytes)
            .map_err(|e| anyhow::anyhow!("compiling WASM component: {e}"))?;
        self.log(
            LogLevel::Debug,
            plugin_id,
            format!(
                "compile took {:.2}ms",
                compile_start.elapsed().as_secs_f64() * 1000.0
            ),
        );

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
        };

        let mut store = Store::new(&self.engine, state);

        // Instantiate the component and get the typed bindings.
        let instantiate_start = Instant::now();
        let plugin = bindings::Plugin::instantiate(&mut store, &component, &linker)
            .map_err(|e| anyhow::anyhow!("instantiating WASM plugin: {e}"))?;
        self.log(
            LogLevel::Debug,
            plugin_id,
            format!(
                "instantiate took {:.2}ms",
                instantiate_start.elapsed().as_secs_f64() * 1000.0
            ),
        );
        self.log(
            LogLevel::Debug,
            plugin_id,
            format!(
                "total load took {:.2}ms",
                total_start.elapsed().as_secs_f64() * 1000.0
            ),
        );

        Ok(WasmPluginInstance {
            store: Mutex::new(store),
            plugin,
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
}

impl WasmPluginInstance {
    /// Emit a debug-level timing log entry for a guest call.
    fn log_timing(store: &Store<PluginState>, method: &str, start: Instant) {
        let state = store.data();
        state.log_sender.send(LogEntry {
            seq: 0,
            timestamp: SystemTime::now(),
            level: LogLevel::Debug,
            source: LogSource::Plugin(state.plugin_id.clone()),
            message: format!(
                "{method}() took {:.2}ms",
                start.elapsed().as_secs_f64() * 1000.0
            ),
            metadata: vec![
                ("method".to_string(), method.to_string()),
                (
                    "duration_ms".to_string(),
                    format!("{:.2}", start.elapsed().as_secs_f64() * 1000.0),
                ),
            ],
            span_id: None,
            span: None,
        });
    }

    /// Call the guest's `enable` export.
    pub fn enable(&self) -> anyhow::Result<()> {
        let mut store = self.store.lock().expect("store not poisoned");
        let start = Instant::now();
        let result = self
            .plugin
            .torchsnap_plugin_lifecycle()
            .call_enable(&mut *store)
            .map_err(|e| anyhow::anyhow!("calling plugin enable(): {e}"));
        Self::log_timing(&store, "enable", start);
        result
    }

    /// Call the guest's `disable` export.
    pub fn disable(&self) -> anyhow::Result<()> {
        let mut store = self.store.lock().expect("store not poisoned");
        let start = Instant::now();
        let result = self
            .plugin
            .torchsnap_plugin_lifecycle()
            .call_disable(&mut *store)
            .map_err(|e| anyhow::anyhow!("calling plugin disable(): {e}"));
        Self::log_timing(&store, "disable", start);
        result
    }

    /// Call the guest's `entries` export and convert to native types.
    pub fn entries(&self) -> anyhow::Result<Vec<crate::search::types::CatalogEntry>> {
        let mut store = self.store.lock().expect("store not poisoned");
        let start = Instant::now();
        let wit_entries = self
            .plugin
            .torchsnap_plugin_search()
            .call_entries(&mut *store)
            .map_err(|e| anyhow::anyhow!("calling plugin entries(): {e}"))?;
        Self::log_timing(&store, "entries", start);

        Ok(wit_entries.into_iter().map(Into::into).collect())
    }

    /// Call the guest's `search` export and convert to native types.
    pub fn search(
        &self,
        query: &str,
        matched_prefix: Option<&str>,
    ) -> anyhow::Result<crate::search::types::PluginResponse> {
        let mut store = self.store.lock().expect("store not poisoned");
        let start = Instant::now();
        let response = self
            .plugin
            .torchsnap_plugin_search()
            .call_search(&mut *store, query, matched_prefix)
            .map_err(|e| anyhow::anyhow!("calling plugin search(): {e}"))?;
        Self::log_timing(&store, "search", start);

        Ok(response.into())
    }

    /// Call the guest's `execute` export and convert to native types.
    pub fn execute(
        &self,
        entry_id: &str,
        action_id: &crate::search::types::ActionId,
    ) -> anyhow::Result<crate::search::types::PostAction> {
        let mut store = self.store.lock().expect("store not poisoned");
        let start = Instant::now();

        let wit_action_id: bindings::torchsnap::plugin::types::ActionId =
            action_id.clone().into();

        let result = self
            .plugin
            .torchsnap_plugin_search()
            .call_execute(&mut *store, entry_id, &wit_action_id)
            .map_err(|e| anyhow::anyhow!("calling plugin execute(): {e}"))?;
        Self::log_timing(&store, "execute", start);

        match result {
            Ok(post_action) => Ok(post_action.into()),
            Err(msg) => anyhow::bail!("plugin execute() returned error: {msg}"),
        }
    }
}
