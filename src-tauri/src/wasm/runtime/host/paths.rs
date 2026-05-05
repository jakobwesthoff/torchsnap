// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Paths host import
//
// Resolves `${...}` substitution variables at runtime
// against the per-plugin `PathContext` stashed on
// `GadgetState`. The recognized-variable list and the
// substitution implementation live in
// `crate::wasm::permission_vars`, shared with the manifest-
// time validator so the two cannot drift apart.
//
// No permission gate; the resolved values are
// informational. The host import returns
// `unterminated` when no `PathContext` has been stashed
// yet (a "not initialized" placeholder, since the WIT
// variant has no dedicated arm for it).
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

        let Some(ctx) = self.path_context.as_ref() else {
            // Mirrors the contract of other capability stashes
            // (`http::client`, `clipboard::writer`): if the
            // bridge has not stashed the context yet, surface
            // it as an error rather than panicking.
            return Err(WitResolveError::Unterminated(
                "paths interface not initialized for this plugin instance".into(),
            ));
        };

        substitute_variables(&template, ctx).map_err(|e| match e {
            ResolveError::UnknownVariable(name) => WitResolveError::UnknownVariable(name),
            ResolveError::Unterminated(rest) => WitResolveError::Unterminated(rest),
        })
    }
}
