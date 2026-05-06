// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Per-Gadget Store State
//
// `GadgetState` is the `T` in `Store<T>`. It holds the WASI
// context, foundational per-gadget state (gadget id, log
// sender, span registry, settings/frecency/source/path
// handles), and the per-capability sub-structs that each
// host import owns.
//
// The composition pattern: capability sub-structs live in
// `host/<capability>.rs` next to their Host impl, and this
// file imports them and stitches them into GadgetState.
// Adding a new capability means editing one capability file
// plus three lines here (add the field, add the
// `Default::default()` to the constructor, add the field
// to `default_for_test`).
// =========================================================

use std::sync::Arc;

use wasmtime::component::ResourceTable;
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

use crate::frecency::GadgetFrecency;
use crate::settings::GadgetSettings;
use crate::wasm::logging::channel::LogSender;
use crate::wasm::logging::spans::SpanRegistry;
use crate::wasm::permission_vars::PathContext;
use crate::wasm::source::GadgetSource;

use super::host::clipboard::ClipboardState;
use super::host::command::CommandState;
use super::host::fs::FsState;
use super::host::http::HttpState;
use super::host::opener::OpenerState;
use super::host::sql::SqlState;
use super::host::website_metadata::WebsiteMetadataState;

pub struct GadgetState {
    pub(crate) gadget_id: String,
    pub(crate) wasi: WasiCtx,
    pub(crate) wasi_table: ResourceTable,
    pub(crate) log_sender: LogSender,
    pub(crate) span_registry: Arc<SpanRegistry>,
    /// Per-gadget namespaced settings reader. `None` until the
    /// bridge stashes the `GadgetContext.settings` handle on
    /// `enable()`. The settings host import (`settings::get`)
    /// errors gracefully if accessed before that happens —
    /// which it shouldn't, since the host always calls
    /// `enable()` before any guest code runs.
    pub(crate) settings: Option<GadgetSettings>,
    /// Per-gadget namespaced frecency reader. Same lifecycle
    /// as `settings`: stashed by the bridge on `enable()` from
    /// the `GadgetContext.frecency` handle and cleared on
    /// `disable()`. The host automatically records selections
    /// before `execute()` and applies score bonuses after
    /// `search()` — this handle is only for gadgets that
    /// need to read frecency state directly (e.g. to drive an
    /// empty-query browse mode).
    pub(crate) frecency: Option<GadgetFrecency>,
    /// The gadget's own source handle, stashed by the bridge
    /// on `enable()` so the `assets::read` / `assets::exists`
    /// host imports can read files bundled inside the gadget
    /// archive (or development directory) without re-opening
    /// it. `None` outside an enable lifetime — the host
    /// imports return an `io-error` in that case, matching
    /// the contract of the other capability stashes.
    pub(crate) gadget_source: Option<Arc<dyn GadgetSource + Send + Sync>>,
    /// Resolved `${...}` substitution variables for this gadget
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

impl GadgetState {
    /// Build a fresh `GadgetState` with all capability fields
    /// in their "disabled" defaults. Called from
    /// `WasmRuntime::instantiate`. The bridge populates each
    /// capability via the corresponding
    /// `WasmGadgetInstance::set_*` setter on `enable()`.
    pub(crate) fn new(
        gadget_id: String,
        wasi: WasiCtx,
        log_sender: LogSender,
        span_registry: Arc<SpanRegistry>,
    ) -> Self {
        Self {
            gadget_id,
            wasi,
            wasi_table: ResourceTable::new(),
            log_sender,
            span_registry,
            settings: None,
            frecency: None,
            gadget_source: None,
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
impl WasiView for GadgetState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.wasi,
            table: &mut self.wasi_table,
        }
    }
}

#[cfg(test)]
impl GadgetState {
    /// Construct a `GadgetState` with all capability fields set
    /// to their "absent" defaults. Tests override specific
    /// fields with struct update syntax
    /// (`..GadgetState::default_for_test()`).
    pub(crate) fn default_for_test() -> Self {
        use wasmtime_wasi::WasiCtxBuilder;

        let wasi = WasiCtxBuilder::new().build();
        GadgetState::new(
            "test-gadget".to_string(),
            wasi,
            LogSender::test_sender(),
            Arc::new(SpanRegistry::new()),
        )
    }
}
