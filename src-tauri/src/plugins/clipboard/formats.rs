// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Clipboard Format Handling
//
// Captures and restores clipboard content across all known
// formats. Each format is read via its typed clipboard-rs
// getter and serialized to raw bytes for uniform storage.
// On restore, the format name maps back to the corresponding
// `ClipboardContent` variant.
//
// Supported formats:
//   text   — plain text (UTF-8 bytes)
//   html   — HTML markup (UTF-8 bytes)
//   rtf    — Rich Text Format (UTF-8 bytes)
//   files  — file paths, JSON-encoded (UTF-8 bytes)
//   image  — PNG-encoded pixel data (binary)
//
// The storage layer treats all content as opaque bytes — it
// only needs the format name for the restore mapping and the
// file extension decision (PNG for images, .bin otherwise).
// =========================================================

use clipboard_rs::common::RustImage;
use clipboard_rs::{Clipboard, ClipboardContext, ContentFormat};

use super::schema::DISPLAY_TEXT_MAX_CHARS;

// =========================================================
// Captured Content
// =========================================================

/// A single format captured from the clipboard, serialized to
/// raw bytes for uniform storage.
pub struct CapturedFormat {
    /// Our format identifier: "text", "html", "rtf", "files",
    /// or "image".
    pub format: String,
    /// Serialized content. Text-like formats are UTF-8 bytes,
    /// images are PNG-encoded.
    pub data: Vec<u8>,
}

/// Result of capturing all clipboard formats.
pub struct CaptureResult {
    /// All formats that were successfully read.
    pub formats: Vec<CapturedFormat>,
    /// Human-readable display text derived from the most
    /// representative format. Capped at [`DISPLAY_TEXT_MAX_CHARS`].
    pub display_text: String,
}

// =========================================================
// Capture
// =========================================================

/// Formats to capture, in display-text priority order: the
/// first format that produces a display string wins.
const CAPTURE_ORDER: &[ContentFormat] = &[
    ContentFormat::Text,
    ContentFormat::Html,
    ContentFormat::Rtf,
    ContentFormat::Files,
    ContentFormat::Image,
];

/// Read all available formats from the clipboard.
///
/// Iterates known formats, reads each via its typed getter,
/// serializes to bytes, and derives a display string from the
/// first text-producing format.
pub fn capture_all(clipboard: &ClipboardContext) -> CaptureResult {
    let mut formats = Vec::new();
    let mut display_text = String::new();

    for content_format in CAPTURE_ORDER {
        if !clipboard.has(content_format.clone()) {
            continue;
        }

        if let Some((captured, format_display)) = read_format(clipboard, content_format) {
            if display_text.is_empty() {
                if let Some(d) = format_display {
                    display_text = d;
                }
            }

            formats.push(captured);
        }
    }

    // Cap at the storage limit.
    if display_text.chars().count() > DISPLAY_TEXT_MAX_CHARS {
        display_text = display_text.chars().take(DISPLAY_TEXT_MAX_CHARS).collect();
    }

    CaptureResult {
        formats,
        display_text,
    }
}

/// Read a single format from the clipboard, returning
/// serialized bytes and an optional display text contribution.
fn read_format(
    clipboard: &ClipboardContext,
    content_format: &ContentFormat,
) -> Option<(CapturedFormat, Option<String>)> {
    match content_format {
        ContentFormat::Text => {
            let text = clipboard.get_text().ok()?;
            if text.is_empty() {
                return None;
            }
            let display = text.clone();
            Some((
                CapturedFormat {
                    format: "text".into(),
                    data: text.into_bytes(),
                },
                Some(display),
            ))
        }

        ContentFormat::Html => {
            let html = clipboard.get_html().ok()?;
            if html.is_empty() {
                return None;
            }
            Some((
                CapturedFormat {
                    format: "html".into(),
                    data: html.into_bytes(),
                },
                None,
            ))
        }

        ContentFormat::Rtf => {
            let rtf = clipboard.get_rich_text().ok()?;
            if rtf.is_empty() {
                return None;
            }
            Some((
                CapturedFormat {
                    format: "rtf".into(),
                    data: rtf.into_bytes(),
                },
                None,
            ))
        }

        ContentFormat::Files => {
            let files = clipboard.get_files().ok()?;
            if files.is_empty() {
                return None;
            }
            let display = files.join(", ");
            let json = serde_json::to_string(&files).ok()?;
            Some((
                CapturedFormat {
                    format: "files".into(),
                    data: json.into_bytes(),
                },
                Some(display),
            ))
        }

        ContentFormat::Image => {
            let image = clipboard.get_image().ok()?;
            if image.is_empty() {
                return None;
            }
            let png_buf = image.to_png().ok()?;
            let data = png_buf.get_bytes().to_vec();
            let (w, h) = image.get_size();
            let display = format!("Image ({w}×{h})");
            Some((
                CapturedFormat {
                    format: "image".into(),
                    data,
                },
                Some(display),
            ))
        }

        // Intentionally not capturing unknown formats — see
        // module-level comment for rationale.
        ContentFormat::Other(_) => None,
    }
}

// =========================================================
// Restore
// =========================================================

/// Reconstruct clipboard contents from stored format data.
///
/// Maps each format name back to the corresponding
/// `ClipboardContent` variant. Unknown format names are
/// silently skipped.
pub fn restore_contents(formats: &[CapturedFormat]) -> Vec<clipboard_rs::ClipboardContent> {
    formats.iter().filter_map(restore_format).collect()
}

fn restore_format(cf: &CapturedFormat) -> Option<clipboard_rs::ClipboardContent> {
    match cf.format.as_str() {
        "text" => {
            let text = String::from_utf8(cf.data.clone()).ok()?;
            Some(clipboard_rs::ClipboardContent::Text(text))
        }
        "html" => {
            let html = String::from_utf8(cf.data.clone()).ok()?;
            Some(clipboard_rs::ClipboardContent::Html(html))
        }
        "rtf" => {
            let rtf = String::from_utf8(cf.data.clone()).ok()?;
            Some(clipboard_rs::ClipboardContent::Rtf(rtf))
        }
        "files" => {
            let json = String::from_utf8(cf.data.clone()).ok()?;
            let paths: Vec<String> = serde_json::from_str(&json).ok()?;
            Some(clipboard_rs::ClipboardContent::Files(paths))
        }
        "image" => {
            let img = clipboard_rs::RustImageData::from_bytes(&cf.data).ok()?;
            Some(clipboard_rs::ClipboardContent::Image(img))
        }
        _ => None,
    }
}
