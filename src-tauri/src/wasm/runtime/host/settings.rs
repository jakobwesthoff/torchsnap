// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Settings host import
//
// Routes guest `settings::get(key)` calls through the
// per-plugin `GadgetSettings` handle stashed on
// `GadgetState`. The handle is bridge-stashed at `enable()`;
// reads outside an enable lifetime degrade gracefully to
// `None` rather than trapping.
// =========================================================

use crate::settings::GadgetSettings;
use crate::wasm::bindings;

use super::super::{GadgetState, WasmGadgetInstance};

impl bindings::torchsnap::plugin::settings::Host for GadgetState {
    fn get(&mut self, key: String) -> Option<String> {
        // Debug builds catch the lifecycle invariant violation
        // ("settings host import called before enable") if it
        // ever changes; in release builds we still
        // gracefully degrade to "unset" so a stale
        // `settings::get` (e.g. between disable and a
        // re-enable cycle that hasn't restashed yet)
        // returns `None` instead of panicking the guest.
        debug_assert!(
            self.settings.is_some(),
            "settings::get called before bridge stashed GadgetSettings — lifecycle invariant broken",
        );
        let settings = self.settings.as_ref()?;
        settings.get_raw(&key)
    }
}

impl WasmGadgetInstance {
    /// Stash a per-plugin `GadgetSettings` handle on the
    /// store data so the `settings::get` host import can
    /// resolve reads. Called by the bridge from `enable()`
    /// before the guest's own `enable()` runs.
    pub fn set_settings(&self, settings: GadgetSettings) {
        self.with_state_mut(|state| state.settings = Some(settings));
    }

    /// Drop the stashed settings handle on `disable()`.
    pub fn clear_settings(&self) {
        self.with_state_mut(|state| state.settings = None);
    }
}
