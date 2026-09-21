// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// CGImage → DynamicImage Conversion
//
// Shared helper for converting a Core Graphics `CGImage` into
// an `image::DynamicImage` with 8-bit RGBA pixels.
//
// macOS internally represents images with 16-bit float samples,
// which neither `TIFFRepresentation` nor `CGImageForProposedRect`
// will convert for us. To get clean 8-bit RGBA, we draw the
// CGImage into a `CGBitmapContext` configured for 8-bit
// premultiplied RGBA. Core Graphics handles float→int conversion
// and color space mapping during the draw call. We then read
// the raw pixels and un-premultiply alpha on the Rust side.
//
// Used by both the app icon extractor (NSWorkspace → CGImage)
// and the SF Symbol renderer (NSImage → CGImage).
// =========================================================

use anyhow::Context;
use image::DynamicImage;
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextGetData, CGColorSpace, CGContext, CGImage,
};

/// Pixel format flag for `CGBitmapContextCreate`:
/// premultiplied alpha in the last component (RGBA layout).
const K_CG_IMAGE_ALPHA_PREMULTIPLIED_LAST: u32 = 1;

/// Render a `CGImage` into an 8-bit RGBA `DynamicImage` via a
/// bitmap context.
///
/// Handles format conversion (including 16-bit float sources),
/// color space mapping, and alpha un-premultiplication.
///
/// Returns `Ok(None)` if the image has zero dimensions.
pub fn cgimage_to_dynamic_image(cg_image: &CGImage) -> anyhow::Result<Option<image::DynamicImage>> {
    let width = CGImage::width(Some(cg_image));
    let height = CGImage::height(Some(cg_image));

    if width == 0 || height == 0 {
        return Ok(None);
    }

    let bytes_per_row = width * 4;

    // Create an 8-bit RGBA bitmap context. Core Graphics will
    // convert the source image's pixel format (including 16-bit
    // float) to 8-bit integer during the draw call.
    let color_space = CGColorSpace::new_device_rgb().context("create device RGB color space")?;

    // SAFETY: Passing null for `data` makes CG allocate its own
    // buffer. The color space, dimensions, and bitmap info are
    // valid for an 8-bit RGBA context.
    let ctx = unsafe {
        CGBitmapContextCreate(
            std::ptr::null_mut(),
            width,
            height,
            8,
            bytes_per_row,
            Some(&color_space),
            K_CG_IMAGE_ALPHA_PREMULTIPLIED_LAST,
        )
    }
    .context("create bitmap context")?;

    // Draw the source CGImage into the bitmap context, filling
    // the entire area. CG handles format conversion, color space
    // mapping, and compositing.
    let dest_rect = CGRect {
        origin: CGPoint { x: 0.0, y: 0.0 },
        size: CGSize {
            width: width as f64,
            height: height as f64,
        },
    };
    CGContext::draw_image(Some(&ctx), dest_rect, Some(cg_image));

    // Read the raw 8-bit RGBA pixel data from the bitmap context.
    let data_ptr = CGBitmapContextGetData(Some(&ctx));
    anyhow::ensure!(!data_ptr.is_null(), "bitmap context data pointer is null");

    let total_bytes = height * bytes_per_row;
    // SAFETY: The bitmap context owns the buffer, which remains
    // valid while `ctx` is alive. We copy it out immediately.
    let premultiplied = unsafe { std::slice::from_raw_parts(data_ptr as *const u8, total_bytes) };

    // Un-premultiply alpha. The bitmap context produces
    // premultiplied RGBA, but the `image` crate and WebP
    // encoder expect straight (non-premultiplied) alpha.
    let mut rgba = Vec::with_capacity(total_bytes);
    for &[r, g, b, a] in premultiplied.as_chunks::<4>().0 {
        let (r, g, b) = if a > 0 && a < 255 {
            let af = a as f32 / 255.0;
            (
                (r as f32 / af).min(255.0) as u8,
                (g as f32 / af).min(255.0) as u8,
                (b as f32 / af).min(255.0) as u8,
            )
        } else {
            (r, g, b)
        };

        rgba.push(r);
        rgba.push(g);
        rgba.push(b);
        rgba.push(a);
    }

    let img = image::RgbaImage::from_raw(width as u32, height as u32, rgba)
        .context("construct RgbaImage from bitmap context pixel data")?;

    Ok(Some(image::DynamicImage::ImageRgba8(img)))
}

/// Get the system-composited icon for a file or bundle at the
/// given path via `NSWorkspace`.
///
/// Works for `.app` bundles, `.appex` extensions, and any other
/// file type macOS knows how to display an icon for. Returns
/// the full-resolution image; downstream processing handles
/// resizing and format conversion.
pub fn nsworkspace_icon_for_file(path: &str) -> anyhow::Result<Option<DynamicImage>> {
    use objc2_app_kit::NSWorkspace;
    use objc2_foundation::NSString;

    let ns_path = NSString::from_str(path);
    let workspace = NSWorkspace::sharedWorkspace();
    let ns_image = workspace.iconForFile(&ns_path);

    // SAFETY: CGImageForProposedRect requires a mutable pointer
    // for the proposed rect (null = use natural size) and optional
    // context/hints. The NSImage is valid and retained for the
    // duration of this call. The returned CGImage borrows from
    // the NSImage and is used immediately.
    let Some(cg_image) = (unsafe {
        ns_image.CGImageForProposedRect_context_hints(std::ptr::null_mut(), None, None)
    }) else {
        return Ok(None);
    };

    cgimage_to_dynamic_image(&cg_image)
}
