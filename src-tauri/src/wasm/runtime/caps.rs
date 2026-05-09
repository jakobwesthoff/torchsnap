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

use crate::frecency::GadgetFrecency;
use crate::settings::GadgetSettings;
use crate::paths::GadgetPaths;
use crate::wasm::source::GadgetSource;

use super::host::clipboard::ClipboardState;
use super::host::command::CommandState;
use super::host::fs::FsState;
use super::host::http::HttpState;
use super::host::opener::OpenerState;
use super::host::sql::{SqlHandleEntry, SqlState};
use super::host::website_metadata::WebsiteMetadataState;

pub struct WasmGadgetCaps {
    // Provided by the host — `None` only in unit tests that
    // don't exercise settings/frecency/assets.
    pub(crate) settings: Option<GadgetSettings>,
    pub(crate) frecency: Option<GadgetFrecency>,
    pub(crate) gadget_source: Option<Arc<dyn GadgetSource + Send + Sync>>,

    // Mandatory — construction fails if GadgetPaths cannot
    // be resolved.
    pub(crate) gadget_paths: GadgetPaths,

    // Capability sub-structs.
    pub(crate) sql: SqlState,
    pub(crate) clipboard: ClipboardState,
    pub(crate) opener: OpenerState,
    pub(crate) http: HttpState,
    pub(crate) fs: FsState,
    pub(crate) command: CommandState,
    pub(crate) website_metadata: WebsiteMetadataState,
}

impl WasmGadgetCaps {
    /// Drain SQL handle reps from the resource table before
    /// drop, so the rusqlite connection is closed eagerly at
    /// disable time rather than lingering until the
    /// `WasmGadgetInstance` itself is dropped.
    pub(crate) fn teardown(&mut self, wasi_table: &mut ResourceTable) {
        let reps = std::mem::take(&mut self.sql.handle_reps);
        for rep in reps {
            let resource: Resource<SqlHandleEntry> = Resource::new_own(rep);
            let _ = wasi_table.delete(resource);
        }
        self.sql.storage = None;
    }
}

#[cfg(test)]
impl WasmGadgetCaps {
    /// Construct a `WasmGadgetCaps` with all capability fields
    /// set to their "absent" defaults. Tests override specific
    /// fields with struct update syntax
    /// (`..WasmGadgetCaps::default_for_test()`).
    pub(crate) fn default_for_test() -> Self {
        Self {
            settings: None,
            frecency: None,
            gadget_source: None,
            gadget_paths: GadgetPaths {
                platform: std::sync::Arc::new(crate::paths::PlatformPaths {
                    home: std::path::PathBuf::from("/tmp/test-home"),
                    xdg_config: std::path::PathBuf::from("/tmp/test-xdg-config"),
                    xdg_data: std::path::PathBuf::from("/tmp/test-xdg-data"),
                }),
                gadget_data: std::path::PathBuf::from("/tmp/test-gadget-data"),
                gadget_archive: std::path::PathBuf::from("/tmp/test-gadget-archive"),
            },
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
