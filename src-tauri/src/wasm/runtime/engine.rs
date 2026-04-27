// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WasmRuntime — shared across all plugins
//
// Holds the wasmtime `Engine` (one per process) plus a
// per-plugin compiled-component cache. The
// compile/instantiate split lets the expensive step run
// once per plugin while instantiation stays cheap enough
// to repeat on demand. See ADR 0033.
// =========================================================

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use wasmtime::component::{Component, HasSelf, Linker};
use wasmtime::{Config, Engine, Store};
use wasmtime_wasi::WasiCtxBuilder;

use crate::wasm::bindings;
use crate::wasm::logging::channel::LogSender;
use crate::wasm::logging::spans::{Logger, SpanRegistry};
use crate::wasm::logging::LogSource;

use super::instance::WasmPluginInstance;
use super::state::PluginState;

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
    pub(crate) components: Mutex<HashMap<String, Component>>,
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

        let state = PluginState::new(
            plugin_id.to_string(),
            wasi,
            self.log_sender.clone(),
            Arc::clone(&self.span_registry),
        );

        let mut store = Store::new(&self.engine, state);

        // Instantiate the component and get the typed bindings.
        let plugin = bindings::Plugin::instantiate(&mut store, &component, &linker)
            .map_err(|e| anyhow::anyhow!("instantiating WASM plugin: {e}"))?;

        Ok(WasmPluginInstance::from_parts(store, plugin, logger))
    }
}
