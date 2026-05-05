// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Frecency host import
//
// Routes guest `frecency::is-enabled` / `frecency::top-items`
// calls through the per-plugin `GadgetFrecency` handle
// stashed on `GadgetState`. Same "degrade gracefully when
// the handle is missing" contract as the settings import:
// an accidental call outside an enable lifetime returns
// empty results rather than trapping.
//
// Record and boost are intentionally NOT exposed here —
// the host already records selections before `execute()`
// dispatches and applies score bonuses to `search()` results
// before they reach the frontend, so plugins never need to
// touch those paths directly.
// =========================================================

use crate::frecency::GadgetFrecency;
use crate::wasm::bindings;

use super::super::{GadgetState, WasmGadgetInstance};

impl bindings::torchsnap::plugin::frecency::Host for GadgetState {
    fn is_enabled(&mut self) -> bool {
        self.frecency.as_ref().is_some_and(|f| f.is_enabled())
    }

    fn top_items(
        &mut self,
        limit: u32,
    ) -> Vec<bindings::torchsnap::plugin::frecency::FrecencyItem> {
        let Some(frecency) = self.frecency.as_ref() else {
            return Vec::new();
        };
        frecency
            .top_items(limit as usize)
            .into_iter()
            .map(Into::into)
            .collect()
    }
}

impl WasmGadgetInstance {
    /// Stash a per-plugin `GadgetFrecency` handle on the store
    /// data so the `frecency::*` host imports can resolve
    /// reads. Called by the bridge from `enable()` before the
    /// guest's own `enable()` runs — same contract as
    /// `set_settings`.
    pub fn set_frecency(&self, frecency: GadgetFrecency) {
        self.with_state_mut(|state| state.frecency = Some(frecency));
    }

    /// Drop the stashed frecency handle on `disable()`.
    pub fn clear_frecency(&self) {
        self.with_state_mut(|state| state.frecency = None);
    }
}
