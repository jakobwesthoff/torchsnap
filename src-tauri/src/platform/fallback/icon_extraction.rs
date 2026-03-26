// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Icon Extraction (Fallback)
//
// Stub implementation. Returns None until platform-specific
// icon extraction is implemented for Linux and Windows.
// =========================================================

use std::path::Path;

use crate::platform::icon_extraction::IconExtractor;

pub struct FallbackIconExtractor;

impl IconExtractor for FallbackIconExtractor {
    fn extract(&self, _app_path: &Path) -> anyhow::Result<Option<image::DynamicImage>> {
        // TODO: Linux — extract from icon theme based on .desktop Icon= field
        // TODO: Windows — extract from PE resources or shortcut targets
        Ok(None)
    }
}
