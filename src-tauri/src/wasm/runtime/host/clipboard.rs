// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Clipboard host import
//
// Routes guest `clipboard::write-text(text)` calls through
// the closure stashed by the bridge on `enable()`. The
// closure wraps `tauri_plugin_clipboard_manager` so this
// module never depends on the Tauri AppHandle directly.
//
// Read access is intentionally not exposed by the WIT
// interface — see the doc comment on the `clipboard`
// interface in `torchsnap-gadget.wit`.
// =========================================================

use crate::wasm::bindings;

use super::super::GadgetState;

/// Closure type for the clipboard write capability.
///
/// Boxed and stored on `ClipboardState` instead of holding a
/// `tauri::AppHandle` directly so the runtime layer stays
/// decoupled from Tauri-specific types. The bridge
/// constructs the closure from its own `AppHandle` and
/// stashes it via `WasmGadgetCaps`.
pub type ClipboardWriter = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// Clipboard state — currently a single closure. Wrapped in
/// a struct for symmetry with the other capabilities so a
/// future `clipboard::read-text` (or any other clipboard
/// capability) lands as a new field rather than a separate
/// flat field on the caps struct.
#[derive(Default)]
pub(crate) struct ClipboardState {
    pub(crate) writer: Option<ClipboardWriter>,
}

impl bindings::torchsnap::gadget::clipboard::Host for GadgetState {
    fn write_text(&mut self, text: String) -> Result<(), String> {
        let caps = self.caps()?;
        let writer = caps.clipboard.writer.as_ref().ok_or_else(|| {
            "clipboard writer not initialized".to_string()
        })?;
        writer(&text)
    }
}
