// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Icon Processing
//
// Platform-independent post-processing for extracted app icons.
// Takes a decoded `DynamicImage` (any resolution the platform
// extractor provides), resizes to a consistent target size,
// and encodes to WebP.
//
// This is the single chokepoint for icon format decisions:
// every platform extractor feeds a DynamicImage in, and the
// cache always receives the same format and dimensions out.
// =========================================================

use anyhow::Context;
use image::DynamicImage;
use image::imageops::FilterType;
use std::io::Cursor;

/// Target icon size in pixels. 256×256 covers Retina displays
/// (128×128 CSS points at 2× scale) while keeping file sizes
/// small (~5–20 KB as WebP).
const TARGET_SIZE: u32 = 256;

/// Resize the image to [`TARGET_SIZE`]×[`TARGET_SIZE`] and encode
/// as WebP.
///
/// The input image may be any resolution — platform extractors
/// often return the highest available representation (e.g.
/// 1024×1024 on macOS Retina). This function normalises
/// everything to a consistent size and format.
pub fn process_icon(img: DynamicImage) -> anyhow::Result<Vec<u8>> {
    // Resize preserving aspect ratio. App icons are square, but
    // if they aren't, `resize` will fit within the target box.
    let resized = img.resize(TARGET_SIZE, TARGET_SIZE, FilterType::Lanczos3);

    let mut webp_bytes = Vec::new();
    resized
        .write_to(&mut Cursor::new(&mut webp_bytes), image::ImageFormat::WebP)
        .context("encode icon as WebP")?;

    Ok(webp_bytes)
}
