// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// FrecencyCap
//
// Type alias for GadgetFrecency. Unit tests require a
// FrecencyStore which needs a tauri_plugin_store::Store<Wry>
// (Tauri runtime dependency) for its settings watch. The
// underlying FrecencyStore and GadgetFrecency APIs are
// exercised through the frecency module's own test suite
// and through the WASM integration tests.
// =========================================================

pub type FrecencyCap = crate::frecency::GadgetFrecency;
