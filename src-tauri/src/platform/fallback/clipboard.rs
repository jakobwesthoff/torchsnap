// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Fallback Clipboard Platform
//
// No-op stubs for platforms that don't support clipboard
// introspection (sensitive content detection, self-write
// marking). All checks return false, so every clipboard
// change will be recorded.
// =========================================================

use super::super::clipboard::ClipboardPlatform;

pub struct FallbackClipboard;

impl ClipboardPlatform for FallbackClipboard {
    fn is_sensitive(&self) -> bool {
        false
    }

    fn is_self_written(&self) -> bool {
        false
    }

    fn ownership_marker(&self) -> Option<clipboard_rs::ClipboardContent> {
        None
    }
}
