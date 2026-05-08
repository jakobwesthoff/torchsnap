// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Paths host import
//
// Resolves `${...}` substitution variables at runtime
// against the per-gadget `PathContext` in the caps bundle.
// The recognized-variable list and the substitution
// implementation live in `crate::wasm::permission_vars`,
// shared with the manifest-time validator so the two
// cannot drift apart.
// =========================================================

use crate::wasm::bindings;
use crate::wasm::permission_vars::{ResolveError, substitute_variables};

use super::super::GadgetState;

impl bindings::torchsnap::gadget::paths::Host for GadgetState {
    fn resolve(
        &mut self,
        template: String,
    ) -> Result<String, bindings::torchsnap::gadget::paths::ResolveError> {
        use bindings::torchsnap::gadget::paths::ResolveError as WitResolveError;

        let caps = self.caps().map_err(|e| WitResolveError::Unterminated(e))?;

        substitute_variables(&template, &caps.path_context).map_err(|e| match e {
            ResolveError::UnknownVariable(name) => WitResolveError::UnknownVariable(name),
            ResolveError::Unterminated(rest) => WitResolveError::Unterminated(rest),
        })
    }
}
