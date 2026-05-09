// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// SettingsCap
//
// Type alias for GadgetSettings. Unit tests require a
// tauri_plugin_store::Store<Wry> which needs a running Tauri
// runtime — not available in plain `cargo test`. The
// underlying GadgetSettings API (get, get_raw) is exercised
// indirectly through the WASM integration tests and through
// native gadgets that use it (ClipboardGadget).
// =========================================================

pub type SettingsCap = crate::settings::GadgetSettings;
