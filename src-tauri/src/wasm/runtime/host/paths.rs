// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Paths host import
//
// Resolves `${...}` substitution variables at runtime
// against the per-gadget `GadgetPaths` in the caps bundle.
// The `PathResolver` trait provides the substitution logic,
// shared with the manifest-time validator so the two cannot
// drift apart.
// =========================================================

use crate::paths::{PathResolver, ResolveError};
use crate::wasm::bindings;

use super::super::GadgetState;

impl From<ResolveError> for bindings::torchsnap::gadget::paths::ResolveError {
    fn from(e: ResolveError) -> Self {
        match e {
            ResolveError::UnknownVariable(name) => Self::UnknownVariable(name),
            ResolveError::Unterminated(rest) => Self::Unterminated(rest),
        }
    }
}

impl bindings::torchsnap::gadget::paths::Host for GadgetState {
    fn resolve(
        &mut self,
        template: String,
    ) -> Result<String, bindings::torchsnap::gadget::paths::ResolveError> {
        let caps = self
            .caps()
            .map_err(bindings::torchsnap::gadget::paths::ResolveError::Unterminated)?;

        caps.gadget_paths
            .substitute_variables(&template)
            .map_err(Into::into)
    }
}
