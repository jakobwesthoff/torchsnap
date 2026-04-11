// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Window Chrome (macOS)
//
// Hides the three `standardWindowButton`s (close, miniaturize,
// zoom) on an auxiliary window so the frontend can render its
// own title bar. The rest of the native window stays exactly
// as Tauri configured it — shadow, rounded corners, and
// edge-drag resize remain intact.
//
// This uses the public `NSWindow.standardWindowButton(_:)` API,
// not a private selector: it is documented AppKit and has been
// used by third-party macOS apps for well over a decade. Tauri
// v2 simply does not expose it in its builder API, so we reach
// for it directly here. See ADR 0034 for the full rationale and
// the alternatives considered.
// =========================================================

use anyhow::Context;
use objc2_app_kit::{NSWindow, NSWindowButton};

use crate::platform::WindowChrome;

pub struct MacosWindowChrome;

impl WindowChrome for MacosWindowChrome {
    fn hide_controls(window: &tauri::WebviewWindow) -> anyhow::Result<()> {
        let ns_window = window
            .ns_window()
            .map_err(|e| anyhow::anyhow!("acquire NSWindow handle: {e:?}"))
            .context("hide window controls")?;

        // SAFETY: Tauri returns a valid pointer to the backing
        // NSWindow for the lifetime of the WebviewWindow. We only
        // borrow it to perform AppKit calls that are themselves
        // documented-public API.
        let ns_window: &NSWindow = unsafe { &*(ns_window as *const NSWindow) };

        for button in [
            NSWindowButton::CloseButton,
            NSWindowButton::MiniaturizeButton,
            NSWindowButton::ZoomButton,
        ] {
            if let Some(btn) = ns_window.standardWindowButton(button) {
                btn.setHidden(true);
            }
        }

        Ok(())
    }
}
