// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Clipboard Format Handling
//
// Format-agnostic capture and restore logic. The clipboard
// carries content in multiple formats simultaneously (text,
// HTML, RTF, files, image, …). This module reads all formats
// that are present during capture and reconstructs them
// faithfully during paste-back.
//
// Storage model:
//   - Text-like formats → stored as a string in SQL
//   - Binary formats (images) → stored in FileStorage, SQL
//     holds a reference key
//
// No format receives special treatment beyond the text/binary
// split. Adding a new format means extending `KNOWN_FORMATS`,
// `read_format`, and `restore_format`.
// =========================================================

use clipboard_rs::common::RustImage;
use clipboard_rs::{Clipboard, ClipboardContext, ContentFormat};

use crate::storage::{FileStorage, StorageKey};

// =========================================================
// Captured Content
// =========================================================

/// Content read from the clipboard for a single format.
pub enum CapturedContent {
    /// Text-like content stored directly in SQL (plain text,
    /// HTML, RTF, JSON-encoded file paths).
    Text(String),
    /// Binary content stored in FileStorage. `ext` is the file
    /// extension used for storage (e.g., "png").
    Binary { data: Vec<u8>, ext: String },
}

/// A single captured format ready for storage.
pub struct CapturedFormat {
    /// Format identifier stored in the database (e.g., "text",
    /// "html", "image").
    pub name: String,
    pub content: CapturedContent,
}

/// Result of capturing all clipboard formats.
pub struct CaptureResult {
    /// All formats that were successfully read.
    pub formats: Vec<CapturedFormat>,
    /// Human-readable preview for the list UI, derived from
    /// the most representative format available.
    pub preview: String,
}

// =========================================================
// Known Formats
//
// Ordered by preview preference: the first text-producing
// format provides the preview string.
// =========================================================

const KNOWN_FORMATS: &[ContentFormat] = &[
    ContentFormat::Text,
    ContentFormat::Html,
    ContentFormat::Rtf,
    ContentFormat::Files,
    ContentFormat::Image,
];

/// Map a `ContentFormat` variant to the string stored in the
/// database.
fn format_name(format: &ContentFormat) -> &'static str {
    match format {
        ContentFormat::Text => "text",
        ContentFormat::Html => "html",
        ContentFormat::Rtf => "rtf",
        ContentFormat::Files => "files",
        ContentFormat::Image => "image",
        ContentFormat::Other(_) => "other",
    }
}

// =========================================================
// Capture
// =========================================================

/// Read all available formats from the clipboard.
///
/// Iterates `KNOWN_FORMATS`, reads each one that is present,
/// and builds a preview from the first text-producing format.
pub fn capture_all(clipboard: &ClipboardContext) -> CaptureResult {
    let mut formats = Vec::new();
    let mut preview = String::new();

    for format in KNOWN_FORMATS {
        if !clipboard.has(format.clone()) {
            continue;
        }

        if let Some((captured, format_preview)) = read_format(clipboard, format) {
            // Use the first non-empty preview we encounter.
            if preview.is_empty() {
                if let Some(p) = format_preview {
                    preview = p;
                }
            }

            formats.push(CapturedFormat {
                name: format_name(format).to_string(),
                content: captured,
            });
        }
    }

    CaptureResult { formats, preview }
}

/// Read a single format from the clipboard.
///
/// Returns the captured content and an optional preview string.
/// The preview is only produced for formats that contribute
/// meaningful human-readable text (plain text, file paths,
/// image dimensions).
fn read_format(
    clipboard: &ClipboardContext,
    format: &ContentFormat,
) -> Option<(CapturedContent, Option<String>)> {
    match format {
        ContentFormat::Text => {
            let text = clipboard.get_text().ok()?;
            if text.is_empty() {
                return None;
            }
            let preview: String = text.chars().take(500).collect();
            Some((CapturedContent::Text(text), Some(preview)))
        }

        ContentFormat::Html => {
            let html = clipboard.get_html().ok()?;
            if html.is_empty() {
                return None;
            }
            // HTML is not suitable as a preview — we rely on the
            // text format for that.
            Some((CapturedContent::Text(html), None))
        }

        ContentFormat::Rtf => {
            let rtf = clipboard.get_rich_text().ok()?;
            if rtf.is_empty() {
                return None;
            }
            Some((CapturedContent::Text(rtf), None))
        }

        ContentFormat::Files => {
            let files = clipboard.get_files().ok()?;
            if files.is_empty() {
                return None;
            }
            let preview: String = files.join(", ").chars().take(500).collect();
            let json = serde_json::to_string(&files).ok()?;
            Some((CapturedContent::Text(json), Some(preview)))
        }

        ContentFormat::Image => {
            let image = clipboard.get_image().ok()?;
            if image.is_empty() {
                return None;
            }
            let png_buf = image.to_png().ok()?;
            let data = png_buf.get_bytes().to_vec();
            let (w, h) = image.get_size();
            let preview = format!("Image ({w}×{h})");
            Some((
                CapturedContent::Binary {
                    data,
                    ext: "png".to_string(),
                },
                Some(preview),
            ))
        }

        ContentFormat::Other(_) => {
            // TODO: Could use get_buffer() for arbitrary formats.
            None
        }
    }
}

// =========================================================
// Restore
// =========================================================

/// Stored content row from the database — the inputs needed
/// to reconstruct a `ClipboardContent` for paste-back.
pub struct StoredContent {
    pub format: String,
    pub text_value: Option<String>,
    /// Currently unused during restore (the image branch
    /// derives the storage key from the entry ID), but kept
    /// for schema completeness and future format support.
    #[allow(dead_code)]
    pub file_key: Option<String>,
}

/// Reconstruct clipboard contents from stored data and write
/// them to the given clipboard context.
///
/// Loads binary content from `files` using the stored file key.
/// Returns the list of `ClipboardContent` items ready for
/// `ctx.set()`.
pub fn restore_contents(
    stored: &[StoredContent],
    entry_id: &str,
    files: &FileStorage,
) -> Vec<clipboard_rs::ClipboardContent> {
    stored
        .iter()
        .filter_map(|sc| restore_format(sc, entry_id, files))
        .collect()
}

/// Reconstruct a single `ClipboardContent` from a stored row.
fn restore_format(
    stored: &StoredContent,
    entry_id: &str,
    files: &FileStorage,
) -> Option<clipboard_rs::ClipboardContent> {
    match stored.format.as_str() {
        "text" => {
            let text = stored.text_value.as_ref()?;
            Some(clipboard_rs::ClipboardContent::Text(text.clone()))
        }
        "html" => {
            let html = stored.text_value.as_ref()?;
            Some(clipboard_rs::ClipboardContent::Html(html.clone()))
        }
        "rtf" => {
            let rtf = stored.text_value.as_ref()?;
            Some(clipboard_rs::ClipboardContent::Rtf(rtf.clone()))
        }
        "files" => {
            let json = stored.text_value.as_ref()?;
            let paths: Vec<String> = serde_json::from_str(json).ok()?;
            Some(clipboard_rs::ClipboardContent::Files(paths))
        }
        "image" => {
            let key = StorageKey::new(entry_id);
            let data = files.load(&key, "png").ok()??;
            let img = clipboard_rs::RustImageData::from_bytes(&data).ok()?;
            Some(clipboard_rs::ClipboardContent::Image(img))
        }
        _ => None,
    }
}
