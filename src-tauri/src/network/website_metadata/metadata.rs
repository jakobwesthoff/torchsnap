// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Page metadata and favicon extraction from HTML.
//!
//! Pure functions for parsing HTML documents and extracting metadata fields
//! (title, description, favicon URL). No I/O — all functions operate on
//! in-memory strings and return data structures.
//!
//! Fetch orchestration lives in the sibling `fetch` module;
//! this module holds only extraction logic.

use scraper::{Html, Selector};
use url::Url;

use super::html_fields;

// =========================================================
// Public types
// =========================================================

/// Extracted page metadata from an HTML document.
#[derive(Clone, Debug, Default)]
pub struct PageMetadata {
    /// Page title (from og:title or `<title>`)
    pub title: Option<String>,
    /// Page description (from og:description or `<meta name="description">`)
    pub description: Option<String>,
    /// URL of the page's favicon image
    pub favicon_url: Option<String>,
}

impl PageMetadata {
    /// Returns `true` when all three metadata fields are populated,
    /// allowing callers to short-circuit further fetching.
    pub fn is_complete(&self) -> bool {
        self.title.is_some() && self.description.is_some() && self.favicon_url.is_some()
    }
}

/// Downloaded favicon image data, ready for storage.
pub struct FaviconImageData {
    /// The URL the image was fetched from.
    // Sibling of `image_data` and `content_type` below, which
    // `fetch_and_store_favicon` reads; no in-repo caller reads `url` back
    // off this struct.
    #[allow(dead_code)]
    pub url: String,
    /// Raw image bytes.
    pub image_data: Vec<u8>,
    /// MIME type of the image (e.g. "image/png", "image/x-icon").
    pub content_type: String,
}

// =========================================================
// HTML metadata extraction
// =========================================================

/// Extracts metadata from an HTML document string.
///
/// Priority for each field:
/// - **Title**: `og:title` meta tag > `<title>` element
/// - **Description**: `og:description` meta tag > `<meta name="description">`
/// - **Favicon**: All `<link>` elements with favicon-related `rel` attributes
///   are collected and scored. Format quality dominates (SVG > PNG > other >
///   ICO), with declared pixel area as tiebreaker. Falls back to
///   `{origin}/favicon.ico` when no favicon links are present.
pub fn extract_metadata(html: &str, base_url: &Url) -> PageMetadata {
    let document = Html::parse_document(html);

    let title = html_fields::extract_title(&document);
    let description = html_fields::extract_description(&document);
    let favicon_url = extract_favicon(&document, base_url);

    PageMetadata {
        title,
        description,
        favicon_url,
    }
}

// =========================================================
// Inline data: URI decoding
// =========================================================

/// Decodes a `data:` URI into its MIME type and raw bytes.
///
/// Accepts both base64-encoded payloads (`data:image/png;base64,...`) and
/// percent-encoded payloads (`data:image/svg+xml,%3Csvg...`). Unencoded plain
/// text payloads (some inline SVGs) also pass through correctly because
/// percent-decoding is a no-op on characters that are not percent-escaped.
///
/// Only `image/*` MIME types are accepted — non-image data URIs cannot be
/// favicon images. Returns `None` for malformed or non-image URIs.
///
/// The returned MIME type is lower-cased and stripped of any parameters
/// (e.g. `"image/svg+xml;charset=utf-8"` → `"image/svg+xml"`).
pub fn decode_data_uri(uri: &str) -> Option<(String, Vec<u8>)> {
    // Strip the mandatory "data:" scheme prefix.
    let rest = uri.strip_prefix("data:")?;

    // The first comma separates the header (MIME type + flags) from the payload.
    let (header, encoded_data) = rest.split_once(',')?;

    // Detect base64 encoding and isolate the MIME type portion.
    let (mime_part, is_base64) = if let Some(m) = header.strip_suffix(";base64") {
        (m, true)
    } else {
        (header, false)
    };

    // The MIME type is the first semicolon-delimited segment of the header.
    // Normalize to lowercase per RFC 2397.
    let mime_type = mime_part
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();

    // Reject non-image types — they cannot be valid favicon images.
    if !mime_type.starts_with("image/") {
        return None;
    }

    let bytes: Vec<u8> = if is_base64 {
        use base64::Engine;
        // Some generators insert whitespace or line breaks into the base64
        // payload. Remove it before decoding.
        base64::engine::general_purpose::STANDARD
            .decode(encoded_data.trim())
            .ok()?
    } else {
        // Percent-decode the payload. This is a no-op for characters that are
        // not percent-escaped, so plain-text SVG URIs work correctly too.
        percent_encoding::percent_decode_str(encoded_data).collect()
    };

    Some((mime_type, bytes))
}

// =========================================================
// Image validation helpers
// =========================================================

/// Checks if the given bytes look like an SVG image.
///
/// SVG is XML-based and not detected by magic-byte libraries like `infer`.
/// We look for an `<svg` tag in the first 4 KB, which covers both standalone
/// SVGs and those with an XML prolog.
pub fn is_likely_svg(bytes: &[u8]) -> bool {
    let check_len = bytes.len().min(4096);
    let prefix = String::from_utf8_lossy(&bytes[..check_len]).to_lowercase();
    prefix.contains("<svg")
}

// =========================================================
// Favicon scoring types and helpers
// =========================================================

/// Image format of a favicon candidate, used for scoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FaviconFormat {
    Svg,
    Png,
    Ico,
    Other,
}

/// Parsed size information from a favicon's `sizes` attribute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FaviconSize {
    Pixels { width: u32, height: u32 },
    Any,
}

/// The `rel` attribute variant that identified this link as a favicon.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FaviconRel {
    Icon,
    AppleTouchIcon,
}

/// A single favicon candidate extracted from a `<link>` element.
#[derive(Debug, Clone)]
struct FaviconCandidate {
    url: String,
    rel: FaviconRel,
    format: FaviconFormat,
    best_size: Option<FaviconSize>,
}

/// Parses the `rel` attribute value into a `FaviconRel`, if it
/// identifies the link as a favicon.
///
/// Handles the multi-token `rel` format (e.g. `"shortcut icon"`) and is
/// case-insensitive per the HTML spec.
fn parse_favicon_rel(rel: &str) -> Option<FaviconRel> {
    let lower = rel.to_ascii_lowercase();
    let tokens: Vec<&str> = lower.split_whitespace().collect();

    if tokens.contains(&"apple-touch-icon") {
        Some(FaviconRel::AppleTouchIcon)
    } else if tokens.contains(&"icon") {
        Some(FaviconRel::Icon)
    } else {
        None
    }
}

/// Determines the image format from the `type` attribute or, as a fallback,
/// the file extension in the URL.
fn detect_favicon_format(type_attr: Option<&str>, href: &str) -> FaviconFormat {
    // Check explicit type attribute first — it's authoritative when present.
    if let Some(mime) = type_attr {
        let mime_lower = mime.to_ascii_lowercase();
        return match mime_lower.as_str() {
            "image/svg+xml" => FaviconFormat::Svg,
            "image/png" => FaviconFormat::Png,
            "image/x-icon" | "image/vnd.microsoft.icon" => FaviconFormat::Ico,
            _ if mime_lower.starts_with("image/") => FaviconFormat::Other,
            // Non-image type attribute — fall through to extension detection.
            _ => format_from_extension(href),
        };
    }

    format_from_extension(href)
}

/// Infers the favicon format from the URL's file extension.
fn format_from_extension(href: &str) -> FaviconFormat {
    // Strip query string and fragment before checking the extension.
    let path = href.split('?').next().unwrap_or(href);
    let path = path.split('#').next().unwrap_or(path);

    if let Some(dot_pos) = path.rfind('.') {
        match path[dot_pos..].to_ascii_lowercase().as_str() {
            ".svg" => FaviconFormat::Svg,
            ".png" => FaviconFormat::Png,
            ".ico" => FaviconFormat::Ico,
            _ => FaviconFormat::Other,
        }
    } else {
        FaviconFormat::Other
    }
}

/// Parses the `sizes` attribute into the single largest `FaviconSize`.
///
/// The `sizes` attribute can contain multiple space-separated tokens
/// (e.g. `"16x16 32x32 192x192"`) or the keyword `"any"`.
fn parse_favicon_sizes(sizes: Option<&str>) -> Option<FaviconSize> {
    let sizes = sizes?.trim();
    if sizes.is_empty() {
        return None;
    }

    let mut best: Option<FaviconSize> = None;

    for token in sizes.split_whitespace() {
        let lower = token.to_ascii_lowercase();

        if lower == "any" {
            // "any" means the icon is scalable — always the best size.
            return Some(FaviconSize::Any);
        }

        // Parse "WxH" or "WXH" (case-insensitive separator).
        if let Some((w_str, h_str)) = lower.split_once('x')
            && let (Ok(w), Ok(h)) = (w_str.parse::<u32>(), h_str.parse::<u32>())
        {
            let candidate = FaviconSize::Pixels {
                width: w,
                height: h,
            };
            best = Some(match best {
                Some(FaviconSize::Pixels {
                    width: bw,
                    height: bh,
                }) if (bw as u64) * (bh as u64) >= (w as u64) * (h as u64) => FaviconSize::Pixels {
                    width: bw,
                    height: bh,
                },
                _ => candidate,
            });
        }
        // Ignore unparseable tokens silently.
    }

    best
}

/// Collects all favicon `<link>` candidates from the document.
fn collect_favicon_candidates(document: &Html, base_url: &Url) -> Vec<FaviconCandidate> {
    let selector = Selector::parse("link[href]").expect("static selector is valid");

    document
        .select(&selector)
        .filter_map(|el| {
            let attrs = el.value();
            let rel = parse_favicon_rel(attrs.attr("rel")?)?;
            let href = attrs.attr("href").filter(|h| !h.trim().is_empty())?;
            let resolved = base_url.join(href).ok()?;

            let format = detect_favicon_format(attrs.attr("type"), resolved.as_str());
            let best_size = parse_favicon_sizes(attrs.attr("sizes"));

            Some(FaviconCandidate {
                url: resolved.to_string(),
                rel,
                format,
                best_size,
            })
        })
        .collect()
}

/// Produces a `(format_score, size_score)` tuple for lexicographic comparison.
///
/// Format score dominates: SVG always wins regardless of size. Among rasters,
/// larger images are preferred. ICO is penalized because it's typically low-res
/// and uses a legacy container format.
fn score_favicon(candidate: &FaviconCandidate) -> (u32, u64) {
    let format_score = match candidate.format {
        FaviconFormat::Svg => 400,
        FaviconFormat::Png => 300,
        FaviconFormat::Other => 200,
        FaviconFormat::Ico => 100,
    };

    let size_score = match candidate.best_size {
        Some(FaviconSize::Any) => u64::MAX,
        Some(FaviconSize::Pixels { width, height }) => (width as u64) * (height as u64),
        // apple-touch-icon conventionally implies 180×180 when sizes is absent.
        None if candidate.rel == FaviconRel::AppleTouchIcon => 180 * 180,
        None => 0,
    };

    (format_score, size_score)
}

/// Selects the best favicon URL from a list of candidates.
fn select_best_favicon(candidates: Vec<FaviconCandidate>) -> Option<String> {
    candidates
        .into_iter()
        .max_by_key(score_favicon)
        .map(|c| c.url)
}

/// Extracts the best favicon URL from link elements using a scored selection
/// approach, resolving relative URLs against the page's base URL.
///
/// Collects all `<link>` elements with favicon-related `rel` attributes, scores
/// them by format quality and declared size, and returns the best match. Falls
/// back to `{origin}/favicon.ico` when no favicon links are present.
fn extract_favicon(document: &Html, base_url: &Url) -> Option<String> {
    let candidates = collect_favicon_candidates(document, base_url);
    select_best_favicon(candidates)
        .or_else(|| base_url.join("/favicon.ico").ok().map(|u| u.to_string()))
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    // =========================================================
    // Title and description extraction
    // =========================================================

    #[test]
    fn extracts_og_title_over_html_title() {
        let html = r#"
            <html><head>
                <title>HTML Title</title>
                <meta property="og:title" content="OG Title" />
            </head><body></body></html>
        "#;
        let base = Url::parse("https://example.com").expect("valid URL");
        let meta = extract_metadata(html, &base);
        assert_eq!(meta.title.as_deref(), Some("OG Title"));
    }

    #[test]
    fn falls_back_to_html_title() {
        let html = r#"
            <html><head>
                <title>HTML Title</title>
            </head><body></body></html>
        "#;
        let base = Url::parse("https://example.com").expect("valid URL");
        let meta = extract_metadata(html, &base);
        assert_eq!(meta.title.as_deref(), Some("HTML Title"));
    }

    #[test]
    fn extracts_og_description_over_meta_description() {
        let html = r#"
            <html><head>
                <meta name="description" content="Meta Desc" />
                <meta property="og:description" content="OG Desc" />
            </head><body></body></html>
        "#;
        let base = Url::parse("https://example.com").expect("valid URL");
        let meta = extract_metadata(html, &base);
        assert_eq!(meta.description.as_deref(), Some("OG Desc"));
    }

    #[test]
    fn handles_empty_meta_content_gracefully() {
        let html = r#"
            <html><head>
                <meta property="og:title" content="" />
                <title>Fallback</title>
            </head><body></body></html>
        "#;
        let base = Url::parse("https://example.com").expect("valid URL");
        let meta = extract_metadata(html, &base);
        assert_eq!(meta.title.as_deref(), Some("Fallback"));
    }

    #[test]
    fn no_title_or_description_returns_none() {
        let html = "<html><head></head><body></body></html>";
        let base = Url::parse("https://example.com").expect("valid URL");
        let meta = extract_metadata(html, &base);
        assert!(meta.title.is_none());
        assert!(meta.description.is_none());
    }

    // =========================================================
    // SVG detection
    // =========================================================

    #[test]
    fn is_likely_svg_detects_svg_tag() {
        assert!(is_likely_svg(
            b"<svg xmlns='http://www.w3.org/2000/svg'></svg>"
        ));
        assert!(is_likely_svg(
            b"<?xml version='1.0'?><svg viewBox='0 0 100 100'></svg>"
        ));
    }

    #[test]
    fn is_likely_svg_rejects_non_svg() {
        assert!(!is_likely_svg(&[
            0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A
        ]));
        assert!(!is_likely_svg(b""));
        assert!(!is_likely_svg(b"<html><body>hello</body></html>"));
    }

    #[test]
    fn is_likely_svg_respects_4kb_window() {
        let mut early_svg = b"<svg></svg>".to_vec();
        early_svg.resize(8192, b' ');
        assert!(is_likely_svg(&early_svg));

        let mut late_svg = vec![b' '; 4100];
        late_svg.extend_from_slice(b"<svg></svg>");
        assert!(!is_likely_svg(&late_svg));
    }

    // =========================================================
    // parse_favicon_rel
    // =========================================================

    #[test]
    fn parse_rel_icon() {
        assert_eq!(parse_favicon_rel("icon"), Some(FaviconRel::Icon));
    }

    #[test]
    fn parse_rel_shortcut_icon() {
        assert_eq!(parse_favicon_rel("shortcut icon"), Some(FaviconRel::Icon));
    }

    #[test]
    fn parse_rel_apple_touch_icon() {
        assert_eq!(
            parse_favicon_rel("apple-touch-icon"),
            Some(FaviconRel::AppleTouchIcon)
        );
    }

    #[test]
    fn parse_rel_case_insensitive() {
        assert_eq!(parse_favicon_rel("Icon"), Some(FaviconRel::Icon));
        assert_eq!(parse_favicon_rel("ICON"), Some(FaviconRel::Icon));
        assert_eq!(
            parse_favicon_rel("Apple-Touch-Icon"),
            Some(FaviconRel::AppleTouchIcon)
        );
    }

    #[test]
    fn parse_rel_non_favicon_returns_none() {
        assert_eq!(parse_favicon_rel("stylesheet"), None);
        assert_eq!(parse_favicon_rel("preload"), None);
        assert_eq!(parse_favicon_rel(""), None);
    }

    // =========================================================
    // detect_favicon_format
    // =========================================================

    #[test]
    fn format_from_type_attr() {
        assert_eq!(
            detect_favicon_format(Some("image/svg+xml"), "/icon.png"),
            FaviconFormat::Svg
        );
        assert_eq!(
            detect_favicon_format(Some("image/png"), "/icon.svg"),
            FaviconFormat::Png
        );
        assert_eq!(
            detect_favicon_format(Some("image/x-icon"), "/icon.png"),
            FaviconFormat::Ico
        );
        assert_eq!(
            detect_favicon_format(Some("image/vnd.microsoft.icon"), "/icon.png"),
            FaviconFormat::Ico
        );
        assert_eq!(
            detect_favicon_format(Some("image/webp"), "/icon.png"),
            FaviconFormat::Other
        );
    }

    #[test]
    fn format_from_extension_fallback() {
        assert_eq!(
            detect_favicon_format(None, "https://example.com/icon.svg"),
            FaviconFormat::Svg
        );
        assert_eq!(
            detect_favicon_format(None, "https://example.com/icon.png"),
            FaviconFormat::Png
        );
        assert_eq!(
            detect_favicon_format(None, "https://example.com/favicon.ico"),
            FaviconFormat::Ico
        );
        assert_eq!(
            detect_favicon_format(None, "https://example.com/icon.webp"),
            FaviconFormat::Other
        );
        assert_eq!(
            detect_favicon_format(None, "https://example.com/icon"),
            FaviconFormat::Other
        );
    }

    #[test]
    fn format_extension_ignores_query_and_fragment() {
        assert_eq!(
            detect_favicon_format(None, "https://example.com/icon.svg?v=2"),
            FaviconFormat::Svg
        );
        assert_eq!(
            detect_favicon_format(None, "https://example.com/icon.png#hash"),
            FaviconFormat::Png
        );
    }

    // =========================================================
    // parse_favicon_sizes
    // =========================================================

    #[test]
    fn sizes_single_value() {
        assert_eq!(
            parse_favicon_sizes(Some("192x192")),
            Some(FaviconSize::Pixels {
                width: 192,
                height: 192
            })
        );
    }

    #[test]
    fn sizes_multi_value_picks_largest() {
        assert_eq!(
            parse_favicon_sizes(Some("16x16 32x32 192x192")),
            Some(FaviconSize::Pixels {
                width: 192,
                height: 192
            })
        );
    }

    #[test]
    fn sizes_any_keyword() {
        assert_eq!(parse_favicon_sizes(Some("any")), Some(FaviconSize::Any));
        assert_eq!(parse_favicon_sizes(Some("ANY")), Some(FaviconSize::Any));
    }

    #[test]
    fn sizes_empty_or_none() {
        assert_eq!(parse_favicon_sizes(None), None);
        assert_eq!(parse_favicon_sizes(Some("")), None);
        assert_eq!(parse_favicon_sizes(Some("   ")), None);
    }

    #[test]
    fn sizes_garbage_ignored() {
        assert_eq!(parse_favicon_sizes(Some("notasize")), None);
        assert_eq!(parse_favicon_sizes(Some("abc x def")), None);
    }

    // =========================================================
    // Scoring
    // =========================================================

    fn make_candidate(
        format: FaviconFormat,
        rel: FaviconRel,
        best_size: Option<FaviconSize>,
    ) -> FaviconCandidate {
        FaviconCandidate {
            url: "https://example.com/icon".to_string(),
            rel,
            format,
            best_size,
        }
    }

    #[test]
    fn svg_beats_any_raster() {
        let svg = make_candidate(FaviconFormat::Svg, FaviconRel::Icon, None);
        let large_png = make_candidate(
            FaviconFormat::Png,
            FaviconRel::Icon,
            Some(FaviconSize::Pixels {
                width: 512,
                height: 512,
            }),
        );
        assert!(score_favicon(&svg) > score_favicon(&large_png));
    }

    #[test]
    fn larger_png_beats_smaller_png() {
        let large = make_candidate(
            FaviconFormat::Png,
            FaviconRel::Icon,
            Some(FaviconSize::Pixels {
                width: 192,
                height: 192,
            }),
        );
        let small = make_candidate(
            FaviconFormat::Png,
            FaviconRel::Icon,
            Some(FaviconSize::Pixels {
                width: 32,
                height: 32,
            }),
        );
        assert!(score_favicon(&large) > score_favicon(&small));
    }

    #[test]
    fn png_192_beats_apple_touch_icon_implied_180() {
        let png_192 = make_candidate(
            FaviconFormat::Png,
            FaviconRel::Icon,
            Some(FaviconSize::Pixels {
                width: 192,
                height: 192,
            }),
        );
        let apple = make_candidate(FaviconFormat::Png, FaviconRel::AppleTouchIcon, None);
        assert!(score_favicon(&png_192) > score_favicon(&apple));
    }

    #[test]
    fn apple_touch_icon_beats_unsized_generic_icon() {
        let apple = make_candidate(FaviconFormat::Png, FaviconRel::AppleTouchIcon, None);
        let unsized_icon = make_candidate(FaviconFormat::Png, FaviconRel::Icon, None);
        assert!(score_favicon(&apple) > score_favicon(&unsized_icon));
    }

    #[test]
    fn ico_loses_to_png_at_same_size() {
        let png = make_candidate(
            FaviconFormat::Png,
            FaviconRel::Icon,
            Some(FaviconSize::Pixels {
                width: 32,
                height: 32,
            }),
        );
        let ico = make_candidate(
            FaviconFormat::Ico,
            FaviconRel::Icon,
            Some(FaviconSize::Pixels {
                width: 32,
                height: 32,
            }),
        );
        assert!(score_favicon(&png) > score_favicon(&ico));
    }

    // =========================================================
    // End-to-end favicon extraction
    // =========================================================

    #[test]
    fn svg_wins_over_apple_touch_icon_and_png() {
        let html = r#"
            <html><head>
                <link rel="apple-touch-icon" href="/apple-icon.png" />
                <link rel="icon" type="image/png" sizes="192x192" href="/icon-192.png" />
                <link rel="icon" type="image/svg+xml" href="/icon.svg" />
            </head><body></body></html>
        "#;
        let base = Url::parse("https://example.com").expect("valid URL");
        let meta = extract_metadata(html, &base);
        assert_eq!(
            meta.favicon_url.as_deref(),
            Some("https://example.com/icon.svg")
        );
    }

    #[test]
    fn large_png_wins_over_small_png() {
        let html = r#"
            <html><head>
                <link rel="icon" type="image/png" sizes="16x16" href="/icon-16.png" />
                <link rel="icon" type="image/png" sizes="192x192" href="/icon-192.png" />
            </head><body></body></html>
        "#;
        let base = Url::parse("https://example.com").expect("valid URL");
        let meta = extract_metadata(html, &base);
        assert_eq!(
            meta.favicon_url.as_deref(),
            Some("https://example.com/icon-192.png")
        );
    }

    #[test]
    fn apple_touch_icon_without_sizes_loses_to_png_192() {
        let html = r#"
            <html><head>
                <link rel="apple-touch-icon" href="/apple-icon.png" />
                <link rel="icon" type="image/png" sizes="192x192" href="/icon-192.png" />
            </head><body></body></html>
        "#;
        let base = Url::parse("https://example.com").expect("valid URL");
        let meta = extract_metadata(html, &base);
        assert_eq!(
            meta.favicon_url.as_deref(),
            Some("https://example.com/icon-192.png")
        );
    }

    #[test]
    fn apple_touch_icon_beats_unsized_generic_icon_e2e() {
        let html = r#"
            <html><head>
                <link rel="icon" href="/favicon.ico" />
                <link rel="apple-touch-icon" href="/apple-icon.png" />
            </head><body></body></html>
        "#;
        let base = Url::parse("https://example.com").expect("valid URL");
        let meta = extract_metadata(html, &base);
        assert_eq!(
            meta.favicon_url.as_deref(),
            Some("https://example.com/apple-icon.png")
        );
    }

    #[test]
    fn resolves_relative_favicon_url() {
        let html = r#"
            <html><head>
                <link rel="icon" href="assets/favicon.ico" />
            </head><body></body></html>
        "#;
        let base = Url::parse("https://example.com/page/").expect("valid URL");
        let meta = extract_metadata(html, &base);
        assert_eq!(
            meta.favicon_url.as_deref(),
            Some("https://example.com/page/assets/favicon.ico")
        );
    }

    #[test]
    fn falls_back_to_default_favicon_ico() {
        let html = "<html><head></head><body></body></html>";
        let base = Url::parse("https://example.com/some/path").expect("valid URL");
        let meta = extract_metadata(html, &base);
        assert_eq!(
            meta.favicon_url.as_deref(),
            Some("https://example.com/favicon.ico")
        );
    }

    #[test]
    fn shortcut_icon_is_discovered() {
        let html = r#"
            <html><head>
                <link rel="shortcut icon" href="/old-favicon.ico" />
            </head><body></body></html>
        "#;
        let base = Url::parse("https://example.com").expect("valid URL");
        let meta = extract_metadata(html, &base);
        assert_eq!(
            meta.favicon_url.as_deref(),
            Some("https://example.com/old-favicon.ico")
        );
    }

    #[test]
    fn mixed_formats_with_sizes() {
        let html = r#"
            <html><head>
                <link rel="icon" type="image/x-icon" href="/favicon.ico" />
                <link rel="icon" type="image/png" sizes="32x32" href="/icon-32.png" />
                <link rel="icon" type="image/png" sizes="16x16" href="/icon-16.png" />
                <link rel="apple-touch-icon" sizes="180x180" href="/apple-180.png" />
            </head><body></body></html>
        "#;
        let base = Url::parse("https://example.com").expect("valid URL");
        let meta = extract_metadata(html, &base);
        // apple-touch-icon 180×180 has area 32400, which beats PNG 32×32 (1024)
        assert_eq!(
            meta.favicon_url.as_deref(),
            Some("https://example.com/apple-180.png")
        );
    }

    // =========================================================
    // decode_data_uri
    // =========================================================

    #[test]
    fn decode_data_uri_base64_png() {
        use base64::Engine;
        let png_bytes = b"\x89PNG\r\n\x1a\n fake png data";
        let b64 = base64::engine::general_purpose::STANDARD.encode(png_bytes);
        let uri = format!("data:image/png;base64,{b64}");

        let (mime, bytes) = decode_data_uri(&uri).expect("decodes successfully");
        assert_eq!(mime, "image/png");
        assert_eq!(bytes, png_bytes);
    }

    #[test]
    fn decode_data_uri_base64_ico() {
        use base64::Engine;
        let ico_bytes = b"\x00\x00\x01\x00 fake ico";
        let b64 = base64::engine::general_purpose::STANDARD.encode(ico_bytes);
        let uri = format!("data:image/x-icon;base64,{b64}");

        let (mime, bytes) = decode_data_uri(&uri).expect("decodes successfully");
        assert_eq!(mime, "image/x-icon");
        assert_eq!(bytes, ico_bytes);
    }

    #[test]
    fn decode_data_uri_percent_encoded_svg() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"></svg>"#;
        let encoded =
            percent_encoding::utf8_percent_encode(svg, percent_encoding::NON_ALPHANUMERIC)
                .to_string();
        let uri = format!("data:image/svg+xml,{encoded}");

        let (mime, bytes) = decode_data_uri(&uri).expect("decodes successfully");
        assert_eq!(mime, "image/svg+xml");
        assert_eq!(bytes, svg.as_bytes());
    }

    #[test]
    fn decode_data_uri_plain_svg_no_encoding() {
        let svg = "<svg xmlns='http://www.w3.org/2000/svg'></svg>";
        let uri = format!("data:image/svg+xml,{svg}");

        let (mime, bytes) = decode_data_uri(&uri).expect("decodes successfully");
        assert_eq!(mime, "image/svg+xml");
        assert_eq!(bytes, svg.as_bytes());
    }

    #[test]
    fn decode_data_uri_base64_with_whitespace() {
        use base64::Engine;
        let data = b"some image data that is long enough to matter";
        let b64 = base64::engine::general_purpose::STANDARD.encode(data);
        let b64_with_newline = format!("  {b64}  ");
        let uri = format!("data:image/webp;base64,{b64_with_newline}");

        let (mime, bytes) = decode_data_uri(&uri).expect("decodes despite whitespace");
        assert_eq!(mime, "image/webp");
        assert_eq!(bytes, data);
    }

    #[test]
    fn decode_data_uri_strips_mime_parameters() {
        use base64::Engine;
        let data = b"<svg></svg>";
        let b64 = base64::engine::general_purpose::STANDARD.encode(data);
        let uri = format!("data:image/svg+xml;charset=utf-8;base64,{b64}");

        let (mime, bytes) = decode_data_uri(&uri).expect("decodes successfully");
        assert_eq!(mime, "image/svg+xml");
        assert_eq!(bytes, data);
    }

    #[test]
    fn decode_data_uri_normalizes_mime_to_lowercase() {
        use base64::Engine;
        let data = b"fake png";
        let b64 = base64::engine::general_purpose::STANDARD.encode(data);
        let uri = format!("data:Image/PNG;base64,{b64}");

        let (mime, _) = decode_data_uri(&uri).expect("decodes successfully");
        assert_eq!(mime, "image/png");
    }

    #[test]
    fn decode_data_uri_non_image_type_returns_none() {
        let uri = "data:text/plain,hello world";
        assert!(decode_data_uri(uri).is_none());
    }

    #[test]
    fn decode_data_uri_application_type_returns_none() {
        use base64::Engine;
        let data = b"some data";
        let b64 = base64::engine::general_purpose::STANDARD.encode(data);
        let uri = format!("data:application/octet-stream;base64,{b64}");
        assert!(decode_data_uri(&uri).is_none());
    }

    #[test]
    fn decode_data_uri_no_comma_returns_none() {
        assert!(decode_data_uri("data:image/png;base64").is_none());
    }

    #[test]
    fn decode_data_uri_not_a_data_uri_returns_none() {
        assert!(decode_data_uri("https://example.com/favicon.png").is_none());
        assert!(decode_data_uri("").is_none());
    }

    #[test]
    fn decode_data_uri_invalid_base64_returns_none() {
        let uri = "data:image/png;base64,!!not-valid-base64!!";
        assert!(decode_data_uri(uri).is_none());
    }

    #[test]
    fn decode_data_uri_empty_payload_is_valid() {
        let uri = "data:image/png;base64,";
        let (mime, bytes) = decode_data_uri(uri).expect("empty payload is valid");
        assert_eq!(mime, "image/png");
        assert!(bytes.is_empty());
    }

    #[test]
    fn extract_metadata_data_uri_favicon_passes_through() {
        let svg = "<svg xmlns='http://www.w3.org/2000/svg'></svg>";
        let encoded =
            percent_encoding::utf8_percent_encode(svg, percent_encoding::NON_ALPHANUMERIC)
                .to_string();
        let data_uri = format!("data:image/svg+xml,{encoded}");
        let html = format!(
            r#"<html><head><link rel="icon" type="image/svg+xml" href="{data_uri}"/></head></html>"#
        );

        let base = Url::parse("https://example.com").expect("valid URL");
        let meta = extract_metadata(&html, &base);

        // The raw data: URI must be preserved as favicon_url.
        assert_eq!(meta.favicon_url.as_deref(), Some(data_uri.as_str()));
    }

    #[test]
    fn page_metadata_is_complete_when_all_fields_present() {
        let meta = PageMetadata {
            title: Some("Title".to_string()),
            description: Some("Desc".to_string()),
            favicon_url: Some("https://example.com/icon.png".to_string()),
        };
        assert!(meta.is_complete());
    }

    #[test]
    fn page_metadata_is_not_complete_with_missing_fields() {
        let meta = PageMetadata {
            title: Some("Title".to_string()),
            description: None,
            favicon_url: Some("https://example.com/icon.png".to_string()),
        };
        assert!(!meta.is_complete());
    }
}
