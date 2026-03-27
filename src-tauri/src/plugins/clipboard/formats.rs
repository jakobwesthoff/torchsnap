// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Clipboard Format Handling
//
// Extracts and converts clipboard content across all known
// formats. Each format is read via its typed clipboard-rs
// getter and serialized to raw bytes for uniform storage.
// On conversion back, the format name maps to the corresponding
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

/// All clipboard formats we know how to capture. Order does
/// not matter — every present format is read unconditionally.
const KNOWN_FORMATS: &[ContentFormat] = &[
    ContentFormat::Text,
    ContentFormat::Html,
    ContentFormat::Rtf,
    ContentFormat::Files,
    ContentFormat::Image,
];

/// Read all available formats from the clipboard.
///
/// Captures every known format that is present, then derives
/// a human-readable display string separately with its own
/// priority logic.
pub fn extract_all_formats(clipboard: &ClipboardContext) -> CaptureResult {
    let mut formats = Vec::new();

    for content_format in KNOWN_FORMATS {
        if !clipboard.has(content_format.clone()) {
            continue;
        }

        if let Some(captured) = read_format(clipboard, content_format) {
            formats.push(captured);
        }
    }

    let display_text = derive_display_text(clipboard);

    CaptureResult {
        formats,
        display_text,
    }
}

/// Read a single format from the clipboard, returning
/// serialized bytes ready for storage.
fn read_format(
    clipboard: &ClipboardContext,
    content_format: &ContentFormat,
) -> Option<CapturedFormat> {
    match content_format {
        ContentFormat::Text => {
            let text = clipboard.get_text().ok()?;
            if text.is_empty() {
                return None;
            }
            Some(CapturedFormat {
                format: "text".into(),
                data: text.into_bytes(),
            })
        }

        ContentFormat::Html => {
            let html = clipboard.get_html().ok()?;
            if html.is_empty() {
                return None;
            }
            Some(CapturedFormat {
                format: "html".into(),
                data: html.into_bytes(),
            })
        }

        ContentFormat::Rtf => {
            let rtf = clipboard.get_rich_text().ok()?;
            if rtf.is_empty() {
                return None;
            }
            Some(CapturedFormat {
                format: "rtf".into(),
                data: rtf.into_bytes(),
            })
        }

        ContentFormat::Files => {
            let files = clipboard.get_files().ok()?;
            if files.is_empty() {
                return None;
            }
            let json = serde_json::to_string(&files).ok()?;
            Some(CapturedFormat {
                format: "files".into(),
                data: json.into_bytes(),
            })
        }

        ContentFormat::Image => {
            let image = clipboard.get_image().ok()?;
            if image.is_empty() {
                return None;
            }
            let png_buf = image.to_png().ok()?;
            let data = png_buf.get_bytes().to_vec();
            Some(CapturedFormat {
                format: "image".into(),
                data,
            })
        }

        // Intentionally not capturing unknown formats — see
        // module-level comment for rationale.
        ContentFormat::Other(_) => None,
    }
}

// =========================================================
// Display Text Derivation
//
// Separate from capture — uses its own priority to pick the
// most meaningful human-readable representation. The typed
// clipboard-rs getters are called again here (cheap, data is
// still on the pasteboard) so capture and display concerns
// stay fully decoupled.
// =========================================================

/// Derive a human-readable display string from the clipboard.
///
/// Priority: files → text → image dimensions. Files take
/// precedence over text because when files are copied, macOS
/// also provides the paths as plain text — but the structured
/// file display is more useful. HTML and RTF don't produce
/// display text (they're markup, not readable content).
fn derive_display_text(clipboard: &ClipboardContext) -> String {
    // Files: structured display with filenames and count.
    if let Ok(files) = clipboard.get_files() {
        if !files.is_empty() {
            return truncate_display_text(&file_paths_to_display_text(&files));
        }
    }

    // Plain text: the most common and broadly useful display.
    if let Ok(text) = clipboard.get_text() {
        if !text.is_empty() {
            return truncate_display_text(&text);
        }
    }

    // Image: show dimensions as a summary.
    if let Ok(image) = clipboard.get_image() {
        if !image.is_empty() {
            let (w, h) = image.get_size();
            return format!("Image ({w}×{h})");
        }
    }

    String::new()
}

/// Cap display text at [`DISPLAY_TEXT_MAX_CHARS`].
fn truncate_display_text(s: &str) -> String {
    if s.chars().count() > DISPLAY_TEXT_MAX_CHARS {
        s.chars().take(DISPLAY_TEXT_MAX_CHARS).collect()
    } else {
        s.to_string()
    }
}

// =========================================================
// File Display
// =========================================================

/// Build a display string for a list of file paths.
///
/// Uses filenames (not full paths) to keep the display compact.
/// Single files show just the name, multiple files show the
/// count followed by the names.
fn file_paths_to_display_text(paths: &[String]) -> String {
    use std::path::Path;

    let names: Vec<&str> = paths
        .iter()
        .map(|p| {
            Path::new(p)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(p)
        })
        .collect();

    if names.len() == 1 {
        names[0].to_string()
    } else {
        format!("{} files: {}", names.len(), names.join(", "))
    }
}

// =========================================================
// Clipboard Content Conversion
// =========================================================

/// Convert captured formats back to clipboard-writable contents.
///
/// Maps each format name back to the corresponding
/// `ClipboardContent` variant. Unknown format names are
/// silently skipped.
pub fn captured_to_clipboard_contents(formats: &[CapturedFormat]) -> Vec<clipboard_rs::ClipboardContent> {
    formats.iter().filter_map(captured_to_clipboard_content).collect()
}

fn captured_to_clipboard_content(cf: &CapturedFormat) -> Option<clipboard_rs::ClipboardContent> {
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
