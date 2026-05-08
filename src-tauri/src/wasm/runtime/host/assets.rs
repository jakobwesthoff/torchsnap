// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Assets host import
//
// Per-gadget asset reads, validated by the same
// `validate_gadget_path` guard that governs every other
// gadget-file read on the host side. No permission section
// in the manifest — the guarantee is spatial: paths are
// confined to the gadget root.
// =========================================================

use crate::wasm::bindings;
use crate::wasm::source;

use super::super::GadgetState;

pub(crate) fn into_assets_io_error(
    e: anyhow::Error,
) -> bindings::torchsnap::gadget::assets::AssetsError {
    bindings::torchsnap::gadget::assets::AssetsError::IoError(format!("{e:#}"))
}

impl bindings::torchsnap::gadget::assets::Host for GadgetState {
    fn read(
        &mut self,
        path: String,
    ) -> Result<Vec<u8>, bindings::torchsnap::gadget::assets::AssetsError> {
        use bindings::torchsnap::gadget::assets::AssetsError;

        if let Err(e) = source::validate_gadget_path(&path) {
            return Err(AssetsError::InvalidPath(format!("{e:#}")));
        }

        let caps = self.caps.as_ref().ok_or_else(|| {
            AssetsError::IoError("capability accessed outside enable lifetime".into())
        })?;
        let gadget_source = caps.gadget_source.as_ref().ok_or_else(|| {
            AssetsError::IoError("assets not initialized".into())
        })?;

        match gadget_source.file_exists(&path) {
            Ok(true) => {}
            Ok(false) => return Err(AssetsError::NotFound),
            Err(e) => return Err(into_assets_io_error(e)),
        }

        gadget_source.read_file(&path).map_err(into_assets_io_error)
    }

    fn exists(
        &mut self,
        path: String,
    ) -> Result<bool, bindings::torchsnap::gadget::assets::AssetsError> {
        use bindings::torchsnap::gadget::assets::AssetsError;

        if let Err(e) = source::validate_gadget_path(&path) {
            return Err(AssetsError::InvalidPath(format!("{e:#}")));
        }

        let caps = self.caps.as_ref().ok_or_else(|| {
            AssetsError::IoError("capability accessed outside enable lifetime".into())
        })?;
        let gadget_source = caps.gadget_source.as_ref().ok_or_else(|| {
            AssetsError::IoError("assets not initialized".into())
        })?;

        gadget_source
            .file_exists(&path)
            .map_err(into_assets_io_error)
    }
}
