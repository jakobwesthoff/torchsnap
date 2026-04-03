// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Shared HTML field extraction helpers.
//!
//! Centralizes the CSS selector logic for extracting common metadata fields
//! (title, description) from parsed HTML documents. The website metadata
//! service delegates to these functions for page metadata extraction.
//!
//! Adapted from the squirly project's `html_fields.rs`.

use scraper::{Html, Selector};

// =========================================================
// Public extraction functions
// =========================================================

/// Extracts the page title from an HTML document.
///
/// Fallback chain: `og:title` meta tag > `<title>` element.
pub fn extract_title(document: &Html) -> Option<String> {
    meta_content(document, r#"meta[property="og:title"]"#).or_else(|| title_tag(document))
}

/// Extracts the page description from an HTML document.
///
/// Fallback chain: `og:description` meta tag > `<meta name="description">`.
pub fn extract_description(document: &Html) -> Option<String> {
    meta_content(document, r#"meta[property="og:description"]"#)
        .or_else(|| meta_content(document, r#"meta[name="description"]"#))
}

// =========================================================
// Internal helpers
// =========================================================

/// Extracts the `content` attribute from the first element matching the given
/// CSS selector. Returns `None` if nothing matches or the content is empty.
fn meta_content(document: &Html, selector: &str) -> Option<String> {
    let sel = Selector::parse(selector).expect("hardcoded selector is valid");
    document
        .select(&sel)
        .next()
        .and_then(|el| el.value().attr("content"))
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Extracts the text content of the `<title>` element.
fn title_tag(document: &Html) -> Option<String> {
    let sel = Selector::parse("title").expect("hardcoded selector is valid");
    document
        .select(&sel)
        .next()
        .map(|el| el.text().collect::<String>())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    // -------------------------------------------------------
    // Title extraction
    // -------------------------------------------------------

    #[test]
    fn title_prefers_og_over_title_tag() {
        let html = r#"
            <html><head>
                <title>HTML Title</title>
                <meta property="og:title" content="OG Title" />
            </head><body></body></html>
        "#;
        let doc = Html::parse_document(html);
        assert_eq!(extract_title(&doc).as_deref(), Some("OG Title"));
    }

    #[test]
    fn title_falls_back_to_title_tag() {
        let html = r#"
            <html><head>
                <title>HTML Title</title>
            </head><body></body></html>
        "#;
        let doc = Html::parse_document(html);
        assert_eq!(extract_title(&doc).as_deref(), Some("HTML Title"));
    }

    #[test]
    fn title_skips_empty_og_and_uses_title_tag() {
        let html = r#"
            <html><head>
                <meta property="og:title" content="" />
                <title>Fallback</title>
            </head><body></body></html>
        "#;
        let doc = Html::parse_document(html);
        assert_eq!(extract_title(&doc).as_deref(), Some("Fallback"));
    }

    #[test]
    fn title_skips_whitespace_only_og() {
        let html = r#"
            <html><head>
                <meta property="og:title" content="   " />
                <title>Fallback</title>
            </head><body></body></html>
        "#;
        let doc = Html::parse_document(html);
        assert_eq!(extract_title(&doc).as_deref(), Some("Fallback"));
    }

    #[test]
    fn title_returns_none_when_absent() {
        let html = "<html><head></head><body></body></html>";
        let doc = Html::parse_document(html);
        assert!(extract_title(&doc).is_none());
    }

    #[test]
    fn title_trims_whitespace() {
        let html = r#"
            <html><head>
                <meta property="og:title" content="  Padded Title  " />
            </head><body></body></html>
        "#;
        let doc = Html::parse_document(html);
        assert_eq!(extract_title(&doc).as_deref(), Some("Padded Title"));
    }

    // -------------------------------------------------------
    // Description extraction
    // -------------------------------------------------------

    #[test]
    fn description_prefers_og_over_meta_name() {
        let html = r#"
            <html><head>
                <meta name="description" content="Meta Desc" />
                <meta property="og:description" content="OG Desc" />
            </head><body></body></html>
        "#;
        let doc = Html::parse_document(html);
        assert_eq!(extract_description(&doc).as_deref(), Some("OG Desc"));
    }

    #[test]
    fn description_falls_back_to_meta_name() {
        let html = r#"
            <html><head>
                <meta name="description" content="Meta Desc" />
            </head><body></body></html>
        "#;
        let doc = Html::parse_document(html);
        assert_eq!(extract_description(&doc).as_deref(), Some("Meta Desc"));
    }

    #[test]
    fn description_skips_empty_og_and_uses_meta_name() {
        let html = r#"
            <html><head>
                <meta property="og:description" content="" />
                <meta name="description" content="Fallback Desc" />
            </head><body></body></html>
        "#;
        let doc = Html::parse_document(html);
        assert_eq!(extract_description(&doc).as_deref(), Some("Fallback Desc"));
    }

    #[test]
    fn description_returns_none_when_absent() {
        let html = "<html><head></head><body></body></html>";
        let doc = Html::parse_document(html);
        assert!(extract_description(&doc).is_none());
    }

    #[test]
    fn description_trims_whitespace() {
        let html = r#"
            <html><head>
                <meta property="og:description" content="  Padded Desc  " />
            </head><body></body></html>
        "#;
        let doc = Html::parse_document(html);
        assert_eq!(extract_description(&doc).as_deref(), Some("Padded Desc"));
    }

    // -------------------------------------------------------
    // Both fields together
    // -------------------------------------------------------

    #[test]
    fn extracts_both_fields_from_complete_document() {
        let html = r#"
            <html><head>
                <meta property="og:title" content="My Page" />
                <meta property="og:description" content="A description" />
            </head><body><p>Content</p></body></html>
        "#;
        let doc = Html::parse_document(html);
        assert_eq!(extract_title(&doc).as_deref(), Some("My Page"));
        assert_eq!(extract_description(&doc).as_deref(), Some("A description"));
    }

    #[test]
    fn returns_none_for_empty_document() {
        let doc = Html::parse_document("");
        assert!(extract_title(&doc).is_none());
        assert!(extract_description(&doc).is_none());
    }
}
