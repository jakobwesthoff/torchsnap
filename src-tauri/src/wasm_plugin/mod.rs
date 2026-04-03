// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WASM Plugin System
//
// Self-contained plugin modules loaded at runtime from
// `.torchsnap` zip archives or plain directories (during
// development). Each plugin carries a WASM component
// (backend) and optional frontend bundles.
//
// This module provides:
// - `manifest` — typed representation of `manifest.toml`
// - `source`   — abstraction over how plugin files are read
// =========================================================

pub mod manifest;
pub mod source;
