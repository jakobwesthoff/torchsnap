// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// macOS Clipboard Platform
//
// Checks the general NSPasteboard for metadata that
// clipboard-rs doesn't expose:
//
// - Concealed type (`org.nspasteboard.ConcealedType`): set by
//   password managers to indicate sensitive content.
// - Self-write marker (`com.torchsnap.clipboard-self-write`):
//   set by our own paste-back so the watcher can skip it.
//
// Both checks inspect `types` on the general pasteboard. The
// self-write marker is included as a `ClipboardContent::Other`
// item in the `ctx.set()` call so it's written atomically with
// the content — the mere presence of the type is the signal.
// =========================================================

use objc2_app_kit::NSPasteboard;
use objc2_foundation::NSString;

use super::super::clipboard::ClipboardPlatform;

/// Custom pasteboard type used to mark our own writes.
const SELF_WRITE_TYPE: &str = "app.torchsnap.clipboard-self-write";

/// Standard macOS pasteboard type set by password managers and
/// other apps that write sensitive content.
const CONCEALED_TYPE: &str = "org.nspasteboard.ConcealedType";

pub struct MacosClipboard;

impl ClipboardPlatform for MacosClipboard {
    fn is_sensitive(&self) -> bool {
        has_pasteboard_type(CONCEALED_TYPE)
    }

    fn is_self_written(&self) -> bool {
        has_pasteboard_type(SELF_WRITE_TYPE)
    }

    fn ownership_marker(&self) -> Option<clipboard_rs::ClipboardContent> {
        Some(clipboard_rs::ClipboardContent::Other(
            SELF_WRITE_TYPE.to_string(),
            Vec::new(),
        ))
    }
}

/// Check whether the general pasteboard currently has the given type.
fn has_pasteboard_type(type_name: &str) -> bool {
    let pb = NSPasteboard::generalPasteboard();
    let Some(types) = pb.types() else {
        return false;
    };

    let target = NSString::from_str(type_name);
    types.iter().any(|t| t.isEqualToString(&target))
}
