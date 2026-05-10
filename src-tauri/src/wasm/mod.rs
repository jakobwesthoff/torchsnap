// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WASM Gadget System
//
// Self-contained gadget modules loaded at runtime from
// `.torchsnap` zip archives or plain directories (during
// development). Each gadget carries a WASM component
// (backend) and optional frontend bundles.
//
// This module provides:
// - `manifest`    — typed representation of `manifest.toml`
// - `source`      — abstraction over how gadget files are read
// - `discovery`   — enumerates gadget search roots and entries
// - `path_safety` — canonical-under-root path resolution
// =========================================================

pub mod argv_matcher;
mod bindings;
pub mod bridge;
pub mod discovery;
pub mod interface_gate;
pub mod logging;
pub mod manifest;
pub mod path_safety;
pub mod protocol;
pub mod runtime;
pub mod source;
