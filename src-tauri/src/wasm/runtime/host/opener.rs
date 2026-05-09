// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Opener host import
//
// Thin bridge layer converting between WIT types and the
// native `OpenerCap` capability. Permission checking
// happens inside `OpenerCap` — this module only handles
// type conversion and the enable-lifetime guard.
// =========================================================

use crate::caps::OpenerError;
use crate::wasm::bindings;

use super::super::GadgetState;

impl From<OpenerError> for bindings::torchsnap::gadget::opener::OpenerError {
    fn from(e: OpenerError) -> Self {
        match e {
            OpenerError::PermissionDenied(msg) => Self::PermissionDenied(msg),
            OpenerError::InvalidUrl(msg) => Self::InvalidUrl(msg),
            OpenerError::BackendFailure(msg) => Self::BackendFailure(msg),
        }
    }
}

impl bindings::torchsnap::gadget::opener::Host for GadgetState {
    fn open_url(
        &mut self,
        url: String,
    ) -> Result<(), bindings::torchsnap::gadget::opener::OpenerError> {
        self.caps().opener().open_url(&url).map_err(Into::into)
    }

    fn open_path(
        &mut self,
        path: String,
    ) -> Result<(), bindings::torchsnap::gadget::opener::OpenerError> {
        self.caps().opener().open_path(&path).map_err(Into::into)
    }

    fn reveal_path(
        &mut self,
        path: String,
    ) -> Result<(), bindings::torchsnap::gadget::opener::OpenerError> {
        self.caps().opener().reveal_path(&path).map_err(Into::into)
    }
}
