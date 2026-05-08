// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Frecency host import
//
// Routes guest `frecency::is-enabled` / `frecency::top-items`
// calls through the per-gadget `GadgetFrecency` handle in
// the caps bundle. Same "degrade gracefully when absent"
// contract as the settings import: an accidental call outside
// an enable lifetime returns empty results rather than
// trapping.
// =========================================================

use crate::wasm::bindings;

use super::super::GadgetState;

impl bindings::torchsnap::gadget::frecency::Host for GadgetState {
    fn is_enabled(&mut self) -> bool {
        self.caps
            .as_ref()
            .and_then(|c| c.frecency.as_ref())
            .is_some_and(|f| f.is_enabled())
    }

    fn top_items(
        &mut self,
        limit: u32,
    ) -> Vec<bindings::torchsnap::gadget::frecency::FrecencyItem> {
        let Some(caps) = self.caps.as_ref() else {
            return Vec::new();
        };
        let Some(frecency) = caps.frecency.as_ref() else {
            return Vec::new();
        };
        frecency
            .top_items(limit as usize)
            .into_iter()
            .map(Into::into)
            .collect()
    }
}
