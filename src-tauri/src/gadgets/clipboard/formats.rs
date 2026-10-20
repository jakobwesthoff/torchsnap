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
use clipboard_rs::{Clipboard, ContentFormat};

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
/// a human-readable display string from what was captured.
pub fn extract_all_formats(clipboard: &impl Clipboard) -> CaptureResult {
    let mut formats = Vec::new();
    let mut image_size = None;

    for content_format in KNOWN_FORMATS {
        if !clipboard.has(content_format.clone()) {
            continue;
        }

        if let Some(captured) = read_format(clipboard, content_format, &mut image_size) {
            formats.push(captured);
        }
    }

    let display_text = display_text(&formats, image_size);

    CaptureResult {
        formats,
        display_text,
    }
}

/// Read a single format from the clipboard, returning
/// serialized bytes ready for storage. A captured image also
/// leaves its size in `image_size`, so the display text needs
/// no second read of a possibly multi-megabyte image.
fn read_format(
    clipboard: &impl Clipboard,
    content_format: &ContentFormat,
    image_size: &mut Option<(u32, u32)>,
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
            *image_size = Some(image.get_size());
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
// Display text is derived from the formats this capture stored,
// never from a further pasteboard read, so it only shows
// content the entry contains. The pasteboard can change at any
// moment; a second read could pick up the next copy, and the
// FTS index built from display text would then find the entry
// by text it does not hold.
// =========================================================

/// Derive a human-readable display string from captured formats.
///
/// Priority: files, then text, then image dimensions. Files take
/// precedence over text because when files are copied, macOS
/// also provides the paths as plain text, and the structured
/// file display is more useful. HTML and RTF don't produce
/// display text (they're markup, not readable content).
fn display_text(formats: &[CapturedFormat], image_size: Option<(u32, u32)>) -> String {
    let captured = |name: &str| formats.iter().find(|f| f.format == name);

    // Files: structured display with filenames and count.
    if let Some(files) = captured("files")
        && let Ok(paths) = serde_json::from_slice::<Vec<String>>(&files.data)
    {
        return truncate_display_text(&file_paths_to_display_text(&paths));
    }

    // Plain text: the most common and broadly useful display.
    // Captured text is never empty, `read_format` skips it.
    if let Some(text) = captured("text") {
        return truncate_display_text(&String::from_utf8_lossy(&text.data));
    }

    // Image: show dimensions as a summary.
    if let Some((w, h)) = image_size {
        return format!("Image ({w}×{h})");
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
pub fn captured_to_clipboard_contents(
    formats: &[CapturedFormat],
) -> Vec<clipboard_rs::ClipboardContent> {
    formats
        .iter()
        .filter_map(captured_to_clipboard_content)
        .collect()
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

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::VecDeque;

    use clipboard_rs::{ClipboardContent, RustImageData};
    use image::DynamicImage;

    use super::*;

    /// What the fake pasteboard holds in place of an image.
    #[derive(Clone, Copy)]
    enum FakeImage {
        Pixels(u32, u32),
        Empty,
    }

    /// A pasteboard whose text and files can change between reads, so a
    /// test can model a copy that lands while a capture is running.
    /// Each queue yields its values in order and then keeps repeating
    /// the last one.
    #[derive(Default)]
    struct FakeClipboard {
        texts: RefCell<VecDeque<String>>,
        files: RefCell<VecDeque<Vec<String>>>,
        image: Option<FakeImage>,
        image_reads: Cell<usize>,
    }

    impl FakeClipboard {
        fn with_texts(mut self, texts: &[&str]) -> Self {
            self.texts = RefCell::new(texts.iter().map(|t| t.to_string()).collect());
            self
        }

        fn with_files(mut self, reads: &[&[&str]]) -> Self {
            self.files = RefCell::new(
                reads
                    .iter()
                    .map(|paths| paths.iter().map(|p| p.to_string()).collect())
                    .collect(),
            );
            self
        }

        fn with_image(mut self, image: FakeImage) -> Self {
            self.image = Some(image);
            self
        }
    }

    fn next_value<T: Clone>(queue: &RefCell<VecDeque<T>>) -> Option<T> {
        let mut queue = queue.borrow_mut();
        if queue.len() > 1 {
            queue.pop_front()
        } else {
            queue.front().cloned()
        }
    }

    fn unsupported<T>() -> clipboard_rs::Result<T> {
        Err("not supported by the fake clipboard".into())
    }

    impl Clipboard for FakeClipboard {
        fn available_formats(&self) -> clipboard_rs::Result<Vec<String>> {
            unsupported()
        }

        fn has(&self, format: ContentFormat) -> bool {
            match format {
                ContentFormat::Text => !self.texts.borrow().is_empty(),
                ContentFormat::Files => !self.files.borrow().is_empty(),
                ContentFormat::Image => self.image.is_some(),
                _ => false,
            }
        }

        fn clear(&self) -> clipboard_rs::Result<()> {
            unsupported()
        }

        fn get_buffer(&self, _format: &str) -> clipboard_rs::Result<Vec<u8>> {
            unsupported()
        }

        fn get_text(&self) -> clipboard_rs::Result<String> {
            next_value(&self.texts).ok_or_else(|| "no text".into())
        }

        fn get_rich_text(&self) -> clipboard_rs::Result<String> {
            unsupported()
        }

        fn get_html(&self) -> clipboard_rs::Result<String> {
            unsupported()
        }

        fn get_image(&self) -> clipboard_rs::Result<RustImageData> {
            self.image_reads.set(self.image_reads.get() + 1);
            match self.image {
                Some(FakeImage::Pixels(width, height)) => Ok(RustImageData::from_dynamic_image(
                    DynamicImage::new_rgba8(width, height),
                )),
                Some(FakeImage::Empty) => Ok(RustImageData::empty()),
                None => Err("no image".into()),
            }
        }

        fn get_files(&self) -> clipboard_rs::Result<Vec<String>> {
            next_value(&self.files).ok_or_else(|| "no files".into())
        }

        fn get(&self, _formats: &[ContentFormat]) -> clipboard_rs::Result<Vec<ClipboardContent>> {
            unsupported()
        }

        fn set_buffer(&self, _format: &str, _buffer: Vec<u8>) -> clipboard_rs::Result<()> {
            unsupported()
        }

        fn set_text(&self, _text: String) -> clipboard_rs::Result<()> {
            unsupported()
        }

        fn set_rich_text(&self, _text: String) -> clipboard_rs::Result<()> {
            unsupported()
        }

        fn set_html(&self, _html: String) -> clipboard_rs::Result<()> {
            unsupported()
        }

        fn set_image(&self, _image: RustImageData) -> clipboard_rs::Result<()> {
            unsupported()
        }

        fn set_files(&self, _files: Vec<String>) -> clipboard_rs::Result<()> {
            unsupported()
        }

        fn set(&self, _contents: Vec<ClipboardContent>) -> clipboard_rs::Result<()> {
            unsupported()
        }
    }

    fn format_names(result: &CaptureResult) -> Vec<&str> {
        result.formats.iter().map(|f| f.format.as_str()).collect()
    }

    // -----------------------------------------------------
    // Display text and stored content describe one copy
    // -----------------------------------------------------

    #[test]
    fn display_text_describes_the_captured_text_when_the_clipboard_changes_after_capture() {
        let clipboard = FakeClipboard::default().with_texts(&["first copy", "second copy"]);

        let result = extract_all_formats(&clipboard);

        assert_eq!(result.formats[0].data, b"first copy");
        assert_eq!(result.display_text, "first copy");
    }

    #[test]
    fn display_text_describes_the_captured_files_when_the_clipboard_changes_after_capture() {
        let clipboard =
            FakeClipboard::default().with_files(&[&["/first/report.pdf"], &["/second/photo.jpg"]]);

        let result = extract_all_formats(&clipboard);

        assert_eq!(result.display_text, "report.pdf");
    }

    #[test]
    fn an_image_is_read_from_the_clipboard_once_per_capture() {
        let clipboard = FakeClipboard::default().with_image(FakeImage::Pixels(3, 2));

        extract_all_formats(&clipboard);

        assert_eq!(clipboard.image_reads.get(), 1);
    }

    // -----------------------------------------------------
    // Display priority
    // -----------------------------------------------------

    #[test]
    fn files_take_precedence_over_text() {
        let clipboard = FakeClipboard::default()
            .with_files(&[&["/a/report.pdf"]])
            .with_texts(&["/a/report.pdf"]);

        let result = extract_all_formats(&clipboard);

        assert_eq!(format_names(&result), ["text", "files"]);
        assert_eq!(result.display_text, "report.pdf");
    }

    #[test]
    fn several_files_show_their_count_and_names() {
        let clipboard = FakeClipboard::default().with_files(&[&["/a/a.txt", "/b/b.txt"]]);

        let result = extract_all_formats(&clipboard);

        assert_eq!(result.display_text, "2 files: a.txt, b.txt");
    }

    #[test]
    fn an_image_shows_its_size() {
        let clipboard = FakeClipboard::default().with_image(FakeImage::Pixels(3, 2));

        let result = extract_all_formats(&clipboard);

        assert_eq!(format_names(&result), ["image"]);
        assert_eq!(result.display_text, "Image (3×2)");
    }

    #[test]
    fn text_takes_precedence_over_an_image() {
        let clipboard = FakeClipboard::default()
            .with_texts(&["caption"])
            .with_image(FakeImage::Pixels(3, 2));

        let result = extract_all_formats(&clipboard);

        assert_eq!(result.display_text, "caption");
    }

    #[test]
    fn an_empty_clipboard_captures_nothing() {
        let result = extract_all_formats(&FakeClipboard::default());

        assert!(result.formats.is_empty());
        assert_eq!(result.display_text, "");
    }

    #[test]
    fn long_text_is_cut_to_the_display_maximum() {
        let long = "x".repeat(DISPLAY_TEXT_MAX_CHARS + 10);
        let clipboard = FakeClipboard::default().with_texts(&[&long]);

        let result = extract_all_formats(&clipboard);

        assert_eq!(result.display_text.chars().count(), DISPLAY_TEXT_MAX_CHARS);
        assert_eq!(result.formats[0].data.len(), DISPLAY_TEXT_MAX_CHARS + 10);
    }

    #[test]
    fn empty_text_is_skipped_and_the_image_is_shown() {
        let clipboard = FakeClipboard::default()
            .with_texts(&[""])
            .with_image(FakeImage::Pixels(3, 2));

        let result = extract_all_formats(&clipboard);

        assert_eq!(format_names(&result), ["image"]);
        assert_eq!(result.display_text, "Image (3×2)");
    }

    #[test]
    fn an_empty_image_is_not_captured() {
        let clipboard = FakeClipboard::default().with_image(FakeImage::Empty);

        let result = extract_all_formats(&clipboard);

        assert!(result.formats.is_empty());
        assert_eq!(result.display_text, "");
    }
}
