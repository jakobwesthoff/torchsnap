// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Icon Extraction (macOS)
//
// Uses NSWorkspace to get the system-composited app icon,
// which handles all icon sources: .icns files, Asset Catalogs
// (.car), and system-provided defaults.
//
// The CGImage→DynamicImage conversion (bitmap context, alpha
// un-premultiplication) is handled by the shared
// `cgimage_conversion` module.
// =========================================================

use std::path::Path;

use crate::platform::icon_extraction::IconExtractor;

use super::cgimage_conversion::cgimage_to_dynamic_image;

pub struct MacosIconExtractor;

impl IconExtractor for MacosIconExtractor {
    fn extract(&self, app_path: &Path) -> anyhow::Result<Option<image::DynamicImage>> {
        use objc2_app_kit::NSWorkspace;
        use objc2_foundation::NSString;

        let ns_path = NSString::from_str(&app_path.to_string_lossy());

        let workspace = NSWorkspace::sharedWorkspace();
        let ns_image = workspace.iconForFile(&ns_path);

        // Obtain a CGImage from the NSImage. The CGImage may use
        // 16-bit float components, any alpha layout, and any color
        // space — the shared conversion handles normalisation.
        let Some(cg_image) = (unsafe {
            ns_image.CGImageForProposedRect_context_hints(
                std::ptr::null_mut(),
                None,
                None,
            )
        }) else {
            return Ok(None);
        };

        cgimage_to_dynamic_image(&cg_image)
    }
}
