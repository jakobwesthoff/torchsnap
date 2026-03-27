// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Clipboard Platform Trait
//
// Platform-specific clipboard introspection for the clipboard
// manager plugin. The main concern is distinguishing "our own"
// writes from user-originated changes, and respecting macOS
// sensitive content markers (e.g., password managers that set
// `org.nspasteboard.ConcealedType`).
//
// The watcher thread calls these on every clipboard change to
// decide whether to record the entry.
// =========================================================

/// Platform-specific clipboard introspection.
///
/// Implementations access the system pasteboard directly (e.g.,
/// `NSPasteboard` on macOS) to check metadata that clipboard-rs
/// does not expose through its cross-platform API.
pub trait ClipboardPlatform: Send + Sync {
    /// Whether the current clipboard contents are marked as
    /// sensitive (e.g., a password manager's concealed field).
    /// The watcher should skip recording sensitive entries.
    fn is_sensitive(&self) -> bool;

    /// Whether the current clipboard contents were written by
    /// Torchsnap itself (via `mark_self_written`). The watcher
    /// should skip recording self-written entries to avoid
    /// duplicating paste-back operations.
    fn is_self_written(&self) -> bool;

    /// Return a `ClipboardContent::Other` item that marks the
    /// clipboard as written by Torchsnap. Include this in the
    /// content list passed to `ctx.set()` so the marker is
    /// written atomically with the content — avoids the race
    /// where the watcher detects the change before a separate
    /// `mark_self_written()` call runs.
    fn self_write_marker(&self) -> Option<clipboard_rs::ClipboardContent>;
}
