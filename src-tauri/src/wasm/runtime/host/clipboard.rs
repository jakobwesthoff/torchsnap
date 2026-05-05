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
// interface in `torchsnap-plugin.wit`.
// =========================================================

use crate::wasm::bindings;

use super::super::{GadgetState, WasmGadgetInstance};

/// Closure type for the clipboard write capability.
///
/// Boxed and stored on `GadgetState` instead of holding a
/// `tauri::AppHandle` directly so the runtime layer stays
/// decoupled from Tauri-specific types. The bridge
/// constructs the closure from its own `AppHandle` and
/// stashes it on `enable()`.
pub type ClipboardWriter = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// Clipboard state — currently a single closure. Wrapped in
/// a struct for symmetry with the other capabilities so a
/// future `clipboard::read-text` (or any other clipboard
/// capability) lands as a new field rather than a separate
/// flat field on `GadgetState`.
#[derive(Default)]
pub(crate) struct ClipboardState {
    /// Closure that writes a string to the system clipboard.
    /// Stashed by the bridge from the `tauri::AppHandle` on
    /// `enable()` so the `clipboard::write-text` host import
    /// can resolve without `GadgetState` itself depending on
    /// the Tauri AppHandle type. `None` between enable
    /// cycles.
    pub(crate) writer: Option<ClipboardWriter>,
}

impl bindings::torchsnap::plugin::clipboard::Host for GadgetState {
    fn write_text(&mut self, text: String) -> Result<(), String> {
        let writer = self.clipboard.writer.as_ref().ok_or_else(|| {
            "clipboard writer not initialized — clipboard::write-text called outside enable lifetime"
                .to_string()
        })?;
        writer(&text)
    }
}

impl WasmGadgetInstance {
    /// Install a closure that writes a string to the system
    /// clipboard. Called by the bridge on `enable()` from a
    /// closure that captures the `tauri::AppHandle`.
    pub fn set_clipboard_writer(&self, writer: ClipboardWriter) {
        self.with_state_mut(|state| state.clipboard.writer = Some(writer));
    }

    /// Drop the stashed clipboard writer on `disable()` so
    /// any post-disable `clipboard::write-text` call (which
    /// shouldn't happen) errors loudly instead of silently
    /// using a stale closure.
    pub fn clear_clipboard_writer(&self) {
        self.with_state_mut(|state| state.clipboard.writer = None);
    }
}
