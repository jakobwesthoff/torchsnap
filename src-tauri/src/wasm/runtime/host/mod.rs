// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Host trait impls — one file per WIT host import
//
// Each capability owns its state sub-struct, the
// `bindings::*::Host for PluginState` impl, any helper
// free functions and type aliases used only by that
// capability, and the corresponding setters/clearers as
// extension methods on `WasmPluginInstance`.
//
// Tests for each capability live in the same file as the
// implementation, gated `#[cfg(test)]`. Shared test
// helpers (test runtime constructor, fixture WASM
// constants) live in the parent runtime module.
// =========================================================

pub mod assets;
pub mod clipboard;
pub mod frecency;
pub mod http;
pub mod logging;
pub mod opener;
pub mod paths;
pub mod platform;
pub mod settings;
