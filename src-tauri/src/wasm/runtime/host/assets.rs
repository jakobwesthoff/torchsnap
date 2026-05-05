// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Assets host import
//
// Per-plugin asset reads, validated by the same
// `validate_plugin_path` guard that governs every other
// plugin-file read on the host side. No permission section
// in the manifest — the guarantee is spatial: paths are
// confined to the plugin root.
//
// `read` and `exists` both pre-validate then delegate to
// the `GadgetSource` trait stashed on `GadgetState`. The
// `into_assets_io_error` helper maps the trait's
// `anyhow::Error` into the WIT `assets-error::io-error`
// variant.
// =========================================================

use crate::wasm::bindings;
use crate::wasm::source;

use super::super::GadgetState;

pub(crate) fn into_assets_io_error(
    e: anyhow::Error,
) -> bindings::torchsnap::plugin::assets::AssetsError {
    bindings::torchsnap::plugin::assets::AssetsError::IoError(format!("{e:#}"))
}

impl bindings::torchsnap::plugin::assets::Host for GadgetState {
    fn read(
        &mut self,
        path: String,
    ) -> Result<Vec<u8>, bindings::torchsnap::plugin::assets::AssetsError> {
        use bindings::torchsnap::plugin::assets::AssetsError;

        // Validate first so a structured `InvalidPath`
        // variant is returned without having to grep the
        // trait's `anyhow::Error` for a guard message.
        if let Err(e) = source::validate_plugin_path(&path) {
            return Err(AssetsError::InvalidPath(format!("{e:#}")));
        }

        let plugin_source = self
            .plugin_source
            .as_ref()
            .ok_or_else(|| AssetsError::IoError("assets not initialized".into()))?;

        // Pre-probe so the "missing" case becomes a
        // structural `NotFound` variant; the alternative —
        // attempting the read and matching on the error
        // string — would be fragile across filesystem /
        // archive backends.
        match plugin_source.file_exists(&path) {
            Ok(true) => {}
            Ok(false) => return Err(AssetsError::NotFound),
            Err(e) => return Err(into_assets_io_error(e)),
        }

        plugin_source.read_file(&path).map_err(into_assets_io_error)
    }

    fn exists(
        &mut self,
        path: String,
    ) -> Result<bool, bindings::torchsnap::plugin::assets::AssetsError> {
        use bindings::torchsnap::plugin::assets::AssetsError;

        if let Err(e) = source::validate_plugin_path(&path) {
            return Err(AssetsError::InvalidPath(format!("{e:#}")));
        }

        let plugin_source = self
            .plugin_source
            .as_ref()
            .ok_or_else(|| AssetsError::IoError("assets not initialized".into()))?;

        plugin_source
            .file_exists(&path)
            .map_err(into_assets_io_error)
    }
}
