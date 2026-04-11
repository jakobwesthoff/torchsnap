// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Window Chrome (fallback)
//
// A no-op implementation used on platforms that do not yet
// have a custom title-bar implementation. Adding Windows or
// Linux support means replacing this file (or adding a sibling)
// with a real implementation — until then, the auxiliary window
// simply keeps whatever chrome Tauri gave it.
// =========================================================

use crate::platform::WindowChrome;

pub struct FallbackWindowChrome;

impl WindowChrome for FallbackWindowChrome {
    fn hide_controls(_window: &tauri::WebviewWindow) -> anyhow::Result<()> {
        Ok(())
    }
}
