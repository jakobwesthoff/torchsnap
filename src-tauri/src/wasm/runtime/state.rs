// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Per-Gadget Store State
//
// `GadgetState` is the `T` in `Store<T>`. It holds the WASI
// context, foundational per-gadget state (gadget id, log
// sender, span registry), and an optional `WasmGadgetCaps`
// bundle that is present only during an enable lifetime.
//
// Host imports access per-capability state through a single
// `caps()` chokepoint that returns a uniform error when the
// gadget is not enabled.
// =========================================================

use std::sync::Arc;

use wasmtime::component::ResourceTable;
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

use crate::wasm::logging::channel::LogSender;
use crate::wasm::logging::spans::SpanRegistry;

use super::caps::WasmGadgetCaps;

pub struct GadgetState {
    pub(crate) gadget_id: String,
    pub(crate) wasi: WasiCtx,
    pub(crate) wasi_table: ResourceTable,
    pub(crate) log_sender: LogSender,
    pub(crate) span_registry: Arc<SpanRegistry>,
    /// Per-enable-cycle capability bundle. `None` outside an
    /// enable lifetime — the `caps()` chokepoint returns a
    /// uniform error in that case.
    pub(crate) caps: Option<WasmGadgetCaps>,
}

impl GadgetState {
    /// Build a fresh `GadgetState` with capabilities absent.
    /// Called from `WasmRuntime::instantiate`. The bridge
    /// populates `caps` via `WasmGadgetInstance::set_caps()`
    /// on `enable()`.
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
            caps: None,
        }
    }

    /// Single chokepoint for host imports that need per-enable
    /// capability state. Returns a uniform error when the
    /// gadget is not enabled — replaces 14 scattered
    /// None-guards across individual host import files.
    pub(crate) fn caps(&mut self) -> Result<&mut WasmGadgetCaps, String> {
        self.caps
            .as_mut()
            .ok_or_else(|| "capability accessed outside enable lifetime".into())
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
    /// Construct a `GadgetState` with capabilities absent.
    /// Tests override specific fields with struct update
    /// syntax (`..GadgetState::default_for_test()`).
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
