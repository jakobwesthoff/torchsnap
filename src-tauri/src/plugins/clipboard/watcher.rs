// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Clipboard Watcher
//
// Background handler that responds to clipboard changes,
// captures all available formats, stores the entry, and
// notifies subscriber channels.
// =========================================================

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use clipboard_rs::{ClipboardContext, ClipboardHandler};

use crate::platform::clipboard::ClipboardPlatform;

use super::formats;
use super::schema::RETENTION_INTERVAL;
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
    pub shutdown: Arc<AtomicBool>,
    pub clipboard: ClipboardContext,
}

impl ClipboardHandler for WatcherHandler {
    fn on_clipboard_change(&mut self) {
        if self.shutdown.load(Ordering::Relaxed) {
            return;
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

        let id = ulid::Ulid::new().to_string().to_lowercase();
        self.state
            .store_entry(&id, &result.display_text, &result.formats)?;
        self.state.notify_subscribers();

        // Periodic retention cleanup.
        let count = self.state.capture_count.fetch_add(1, Ordering::Relaxed);
        if count > 0 && count % RETENTION_INTERVAL == 0 {
            if let Err(e) = self.state.delete_expired_entries() {
                eprintln!("clipboard: periodic retention failed: {e:#}");
            }
        }

        Ok(())
    }
}
