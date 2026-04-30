// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Per-Plugin Store State
//
// `PluginState` is the `T` in `Store<T>`. It holds the WASI
// context, foundational per-plugin state (plugin id, log
// sender, span registry, settings/frecency/source/path
// handles), and the per-capability sub-structs that each
// host import owns.
//
// The composition pattern: capability sub-structs live in
// `host/<capability>.rs` next to their Host impl, and this
// file imports them and stitches them into PluginState.
// Adding a new capability means editing one capability file
// plus three lines here (add the field, add the
// `Default::default()` to the constructor, add the field
// to `default_for_test`).
// =========================================================

use std::sync::Arc;

use wasmtime::component::ResourceTable;
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

use crate::frecency::PluginFrecency;
use crate::settings::PluginSettings;
use crate::wasm::logging::channel::LogSender;
use crate::wasm::logging::spans::SpanRegistry;
use crate::wasm::permission_vars::PathContext;
use crate::wasm::source::PluginSource;

use super::host::clipboard::ClipboardState;
use super::host::command::CommandState;
use super::host::fs::FsState;
use super::host::http::HttpState;
use super::host::opener::OpenerState;
use super::host::sql::SqlState;
use super::host::website_metadata::WebsiteMetadataState;

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
    pub(crate) plugin_source: Option<Arc<dyn PluginSource + Send + Sync>>,
    /// Resolved `${...}` substitution variables for this plugin
    /// instance. Populated by the bridge on `enable()`; consumed
    /// by `paths::resolve` and by `command::run` rule
    /// compilation. `None` outside an enable lifetime —
    /// `paths::resolve` returns `unterminated` in that case as
    /// a placeholder for "interface not initialized" since the
    /// variant has no dedicated "uninitialized" arm.
    pub(crate) path_context: Option<PathContext>,
    /// Per-capability state grouped by host import. Each
    /// sub-struct owns the bridge-stashed data plus any
    /// permission flags / allowlists for one WIT interface.
    /// See the per-struct docs for the lifecycle contract.
    pub(crate) sql: SqlState,
    pub(crate) clipboard: ClipboardState,
    pub(crate) opener: OpenerState,
    pub(crate) http: HttpState,
    pub(crate) fs: FsState,
    pub(crate) command: CommandState,
    pub(crate) website_metadata: WebsiteMetadataState,
}

impl PluginState {
    /// Build a fresh `PluginState` with all capability fields
    /// in their "disabled" defaults. Called from
    /// `WasmRuntime::instantiate`. The bridge populates each
    /// capability via the corresponding
    /// `WasmPluginInstance::set_*` setter on `enable()`.
    pub(crate) fn new(
        plugin_id: String,
        wasi: WasiCtx,
        log_sender: LogSender,
        span_registry: Arc<SpanRegistry>,
    ) -> Self {
        Self {
            plugin_id,
            wasi,
            wasi_table: ResourceTable::new(),
            log_sender,
            span_registry,
            settings: None,
            frecency: None,
            plugin_source: None,
            path_context: None,
            sql: SqlState::default(),
            clipboard: ClipboardState::default(),
            opener: OpenerState::default(),
            http: HttpState::default(),
            fs: FsState::default(),
            command: CommandState::default(),
            website_metadata: WebsiteMetadataState::default(),
        }
    }
}

// `WasiView` lets `wasmtime-wasi` locate the WasiCtx and
// ResourceTable inside our custom store data. In wasmtime
// 43, `ctx()` returns a `WasiCtxView` that bundles both
// references together.
impl WasiView for PluginState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.wasi_table,
        }
    }
}

#[cfg(test)]
impl PluginState {
    /// Construct a `PluginState` with all capability fields set
    /// to their "absent" defaults. Tests override specific
    /// fields with struct update syntax
    /// (`..PluginState::default_for_test()`).
    pub(crate) fn default_for_test() -> Self {
        use wasmtime_wasi::WasiCtxBuilder;

        let wasi = WasiCtxBuilder::new().build();
        PluginState::new(
            "test-plugin".to_string(),
            wasi,
            LogSender::test_sender(),
            Arc::new(SpanRegistry::new()),
        )
    }
}
