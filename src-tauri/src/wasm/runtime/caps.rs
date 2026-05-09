// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WasmGadgetCaps — unified capability bundle
//
// All per-enable-cycle state that the bridge stashes on
// `GadgetState` at `enable()` and drops at `disable()`.
// Constructed atomically from `WasmGadgetBridge::enable()`
// and stored as `Option<WasmGadgetCaps>` on `GadgetState`.
//
// Host imports access capabilities through a single
// `GadgetState::caps()` chokepoint that returns `Err` when
// the gadget is not enabled, replacing 14 scattered None-
// guards with one uniform check.
// =========================================================

use std::sync::Arc;

use wasmtime::component::{Resource, ResourceTable};

use crate::paths::GadgetPaths;

use crate::caps::{
    ClipboardCap, CommandCap, FilesystemCap, FrecencyCap, HttpCap, OpenerCap, SettingsCap,
    SqlStorageCap, WebsiteMetadataCap,
};
use super::host::sql::SqlHandleEntry;

pub struct WasmGadgetCaps {
    pub(crate) settings: Option<Arc<SettingsCap>>,
    pub(crate) frecency: Option<Arc<FrecencyCap>>,

    pub(crate) gadget_paths: GadgetPaths,

    // Capability types.
    pub(crate) sql_storage: Option<Arc<SqlStorageCap>>,
    pub(crate) clipboard: Arc<ClipboardCap>,
    pub(crate) opener: Arc<OpenerCap>,
    pub(crate) http: Arc<HttpCap>,
    pub(crate) filesystem: Option<Arc<FilesystemCap>>,
    pub(crate) command: Option<Arc<CommandCap>>,
    pub(crate) website_metadata: Option<Arc<WebsiteMetadataCap>>,
}

impl WasmGadgetCaps {
    /// Drain SQL handle reps from the resource table before
    /// drop, so the rusqlite connection is closed eagerly at
    /// disable time rather than lingering until the
    /// `WasmGadgetInstance` itself is dropped.
    ///
    /// `sql_handle_reps` is passed in from `GadgetState` —
    /// handle lifecycle tracking lives there, not on the caps.
    pub(crate) fn teardown(
        &mut self,
        sql_handle_reps: &mut Vec<u32>,
        wasi_table: &mut ResourceTable,
    ) {
        let reps = std::mem::take(sql_handle_reps);
        for rep in reps {
            let resource: Resource<SqlHandleEntry> = Resource::new_own(rep);
            let _ = wasi_table.delete(resource);
        }
        self.sql_storage = None;
    }
}

#[cfg(test)]
impl WasmGadgetCaps {
    pub(crate) fn default_for_test() -> Self {
        Self {
            settings: None,
            frecency: None,
            gadget_paths: GadgetPaths {
                platform: std::sync::Arc::new(crate::paths::PlatformPaths {
                    home: std::path::PathBuf::from("/tmp/test-home"),
                    xdg_config: std::path::PathBuf::from("/tmp/test-xdg-config"),
                    xdg_data: std::path::PathBuf::from("/tmp/test-xdg-data"),
                }),
                gadget_data: std::path::PathBuf::from("/tmp/test-gadget-data"),
                gadget_archive: std::path::PathBuf::from("/tmp/test-gadget-archive"),
            },
            sql_storage: None,
            clipboard: Arc::new(ClipboardCap::new(Box::new(|_| {
                Err("clipboard not initialized".into())
            }))),
            opener: Arc::new(OpenerCap::from_closures(
                crate::caps::OpenerPermissions {
                    schemes: Vec::new(),
                    open_path: false,
                    reveal_path: false,
                },
                Box::new(|_| Err("opener not initialized".into())),
                Box::new(|_| Err("opener not initialized".into())),
                Box::new(|_| Err("opener not initialized".into())),
            )),
            http: Arc::new(HttpCap::new(Vec::new())),
            filesystem: None,
            command: None,
            website_metadata: None,
        }
    }
}
