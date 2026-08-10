// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Clipboard Watcher
//
// Background handler that responds to clipboard changes,
// captures all available formats, stores the entry, and
// refreshes the active query so the UI updates.
// =========================================================

use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Result;
use clipboard_rs::{ClipboardContext, ClipboardHandler};

use crate::platform::clipboard::ClipboardPlatform;

use super::WatcherLifecycle;
use super::formats;
use super::storage::SharedState;

// =========================================================
// WatcherHandler
// =========================================================

/// Clipboard change handler passed to clipboard-rs.
///
/// On each change: checks platform flags (self-written,
/// sensitive), extracts all formats via `formats::extract_all_formats`,
/// stores the entry, and notifies subscribers.
pub struct WatcherHandler {
    pub platform: Arc<dyn ClipboardPlatform>,
    pub state: Arc<SharedState>,
    pub shutdown: Arc<Mutex<WatcherLifecycle>>,
    pub clipboard: ClipboardContext,
}

impl ClipboardHandler for WatcherHandler {
    fn on_clipboard_change(&mut self) {
        // Check if the watcher has been stopped (disabled or
        // app shutting down).
        {
            let lc = self.shutdown.lock().expect("lifecycle not poisoned");
            if !lc.running || lc.app_shutting_down {
                return;
            }
        }

        // Skip our own writes and sensitive content (e.g.,
        // password manager entries).
        if self.platform.is_self_written() || self.platform.is_sensitive() {
            return;
        }

        if let Err(e) = self.process_clipboard_change() {
            eprintln!("clipboard: process change failed: {e:#}");
        }
    }
}

impl WatcherHandler {
    fn process_clipboard_change(&mut self) -> Result<()> {
        let result = formats::extract_all_formats(&self.clipboard);

        // Nothing captured — skip.
        if result.formats.is_empty() {
            return Ok(());
        }

        let id = ulid::Ulid::generate().to_string().to_lowercase();
        self.state
            .store_entry(&id, &result.display_text, &result.formats)?;
        self.state.refresh_active_query();

        Ok(())
    }
}
