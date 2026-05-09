// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Clipboard host import
//
// Thin bridge layer converting between WIT types and the
// native `ClipboardCap`. Read access is intentionally not
// exposed — see the `clipboard` interface doc in the WIT.
// =========================================================

use crate::caps::ClipboardError;
use crate::wasm::bindings;

use super::super::GadgetState;

impl From<ClipboardError> for bindings::torchsnap::gadget::clipboard::ClipboardError {
    fn from(e: ClipboardError) -> Self {
        match e {
            ClipboardError::BackendFailure(msg) => Self::BackendFailure(msg),
        }
    }
}

impl bindings::torchsnap::gadget::clipboard::Host for GadgetState {
    fn write_text(
        &mut self,
        text: String,
    ) -> Result<(), bindings::torchsnap::gadget::clipboard::ClipboardError> {
        let caps = self
            .caps()
            .map_err(|e| bindings::torchsnap::gadget::clipboard::ClipboardError::BackendFailure(e))?;
        caps.clipboard.write_text(&text).map_err(Into::into)
    }
}
