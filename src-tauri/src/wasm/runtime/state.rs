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

use crate::caps::ProvisionedCaps;

pub struct GadgetState {
    pub(crate) gadget_id: String,
    pub(crate) wasi: WasiCtx,
    pub(crate) wasi_table: ResourceTable,
    pub(crate) log_sender: LogSender,
    pub(crate) span_registry: Arc<SpanRegistry>,
    /// Host-built capability bundle, injected at instance
    /// construction via `instantiate()`. All host imports
    /// access caps through this field.
    pub(crate) caps: Arc<ProvisionedCaps>,
    /// Gadget source for asset resolution. Set by the bridge in
    /// enable(), cleared in clear_caps(). Lives here (not on
    /// caps) because it's a WASM bridge concern, not a capability.
    pub(crate) gadget_source: Option<Arc<dyn crate::wasm::source::GadgetSource + Send + Sync>>,
    /// WASM resource table handle reps for open SQL connections.
    /// Tracked here (not on caps) because this is wasmtime
    /// resource lifecycle, not a capability concern. Drained
    /// during `clear_caps()`.
    pub(crate) sql_handle_reps: Vec<u32>,
}

impl GadgetState {
    /// Build a fresh `GadgetState` with the given caps.
    /// Called from `WasmRuntime::instantiate` and populated
    /// by the bridge at `enable()`.
    pub(crate) fn new(
        gadget_id: String,
        wasi: WasiCtx,
        log_sender: LogSender,
        span_registry: Arc<SpanRegistry>,
        caps: Arc<ProvisionedCaps>,
    ) -> Self {
        Self {
            gadget_id,
            wasi,
            wasi_table: ResourceTable::new(),
            log_sender,
            span_registry,
            caps,
            gadget_source: None,
            sql_handle_reps: Vec::new(),
        }
    }

    /// Access the provisioned capabilities. Always available —
    /// if the instance exists, caps exist.
    pub(crate) fn caps(&self) -> &Arc<ProvisionedCaps> {
        &self.caps
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
    /// Construct a test `GadgetState` with default caps.
    /// Tests override specific fields with struct update
    /// syntax (`..GadgetState::default_for_test()`).
    pub(crate) fn default_for_test() -> Self {
        use wasmtime_wasi::WasiCtxBuilder;

        let wasi = WasiCtxBuilder::new().build();
        let caps = Arc::new(ProvisionedCaps {
            opener: Some(Arc::new(crate::caps::OpenerCap::from_closures(
                crate::caps::OpenerPermissions {
                    schemes: Vec::new(),
                    open_path: false,
                    reveal_path: false,
                },
                Box::new(|_| Err("opener not initialized".into())),
                Box::new(|_| Err("opener not initialized".into())),
                Box::new(|_| Err("opener not initialized".into())),
            ))),
            http: Some(Arc::new(crate::caps::HttpCap::new(Vec::new()))),
            filesystem: None,
            command: None,
            clipboard: Some(Arc::new(crate::caps::ClipboardCap::new(Box::new(|_| {
                Err("clipboard not initialized".into())
            })))),
            sql_storage: None,
            website_metadata: None,
            icon_cache: None,
            settings: None,
            frecency: None,
            path_resolver: Some(Arc::new(crate::caps::PathResolverCap::new(Arc::new(
                crate::paths::GadgetPaths {
                    platform: Arc::new(crate::paths::PlatformPaths {
                        home: std::path::PathBuf::from("/tmp/test-home"),
                        xdg_config: std::path::PathBuf::from("/tmp/test-xdg-config"),
                        xdg_data: std::path::PathBuf::from("/tmp/test-xdg-data"),
                    }),
                    gadget_data: std::path::PathBuf::from("/tmp/test-gadget-data"),
                    gadget_archive: std::path::PathBuf::from("/tmp/test-gadget-archive"),
                },
            )))),
        });
        GadgetState::new(
            "test-gadget".to_string(),
            wasi,
            LogSender::test_sender(),
            Arc::new(SpanRegistry::new()),
            caps,
        )
    }
}
