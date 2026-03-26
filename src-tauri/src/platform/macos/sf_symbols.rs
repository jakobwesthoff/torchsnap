// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// SF Symbol Rendering (macOS)
//
// Renders an SF Symbol by name into a `DynamicImage` suitable
// for icon caching. SF Symbols are vector-based system icons
// provided by macOS — they cannot be loaded as web fonts in a
// Tauri web view, so we rasterize them here on the native side.
//
// The symbol is requested at 512×512 to ensure a high-quality
// source image. The downstream `icon_processing::process_icon`
// resizes to the final 256×256 target.
// =========================================================

use anyhow::Context;
use image::DynamicImage;
use objc2_app_kit::NSImage;
use objc2_foundation::{NSSize, NSString};

use super::cgimage_conversion::cgimage_to_dynamic_image;

/// Size at which to rasterize the SF Symbol before downstream
/// resizing. 512×512 gives plenty of detail for the final
/// 256×256 cached icon.
const RENDER_SIZE: f64 = 512.0;

/// Render an SF Symbol by name into a `DynamicImage`.
///
/// Returns `Ok(None)` if the symbol name is not recognized by
/// the system. Returns `Err` on rasterization failures.
pub fn render_sf_symbol(symbol_name: &str) -> anyhow::Result<Option<DynamicImage>> {
    let ns_name = NSString::from_str(symbol_name);

    let Some(ns_image) =
        NSImage::imageWithSystemSymbolName_accessibilityDescription(&ns_name, None)
    else {
        return Ok(None);
    };

    // SF Symbols are vector-based. Setting the size before
    // extracting the CGImage controls the rasterization
    // resolution.
    ns_image.setSize(NSSize {
        width: RENDER_SIZE,
        height: RENDER_SIZE,
    });

    let Some(cg_image) = (unsafe {
        ns_image.CGImageForProposedRect_context_hints(
            std::ptr::null_mut(),
            None,
            None,
        )
    }) else {
        anyhow::bail!("failed to get CGImage for SF Symbol \"{symbol_name}\"");
    };

    cgimage_to_dynamic_image(&cg_image)
        .context(format!("convert SF Symbol \"{symbol_name}\" to DynamicImage"))
}
