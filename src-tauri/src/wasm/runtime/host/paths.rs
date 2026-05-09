// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Paths host import
//
// Resolves `${...}` substitution variables at runtime
// through `PathResolverCap`, which wraps the shared
// `PathResolver` trait impl. The same trait is used at
// manifest-validation time so the two cannot drift apart.
// =========================================================

use crate::paths::ResolveError;
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
        self.caps()
            .path_resolver()
            .substitute_variables(&template)
            .map_err(Into::into)
    }
}
