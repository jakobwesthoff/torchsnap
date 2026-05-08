// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Settings host import
//
// Routes guest `settings::get(key)` calls through the
// per-gadget `GadgetSettings` handle in the caps bundle.
// Reads outside an enable lifetime degrade gracefully to
// `None` rather than trapping.
// =========================================================

use crate::wasm::bindings;

use super::super::GadgetState;

impl bindings::torchsnap::gadget::settings::Host for GadgetState {
    fn get(&mut self, key: String) -> Option<String> {
        let caps = self.caps.as_ref()?;
        let settings = caps.settings.as_ref()?;
        settings.get_raw(&key)
    }
}
