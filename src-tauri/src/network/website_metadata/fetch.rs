// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! HTTP fetching layer for website metadata extraction.
//!
//! Bridges the `network::Http` client with the pure extraction functions
//! in `metadata.rs`. Uses a two-phase streaming approach to minimize
//! bandwidth: phase 1 reads up to 256 KB (or until end-of-head markers),
//! phase 2 reads another 256 KB if metadata was incomplete.
//!
//! All functions are synchronous — `Http` handles the sync-to-async
//! bridging internally via `tauri::async_runtime::handle().block_on()`.

use std::time::Duration;

use thiserror::Error;
use url::Url;

use super::metadata::{self, FaviconImageData, PageMetadata};
use crate::network::Http;

// =========================================================
// Constants
// =========================================================

/// Phase 1 streaming cap: we attempt early metadata extraction after
/// reading this many bytes (or seeing `</head>` / `<body`).
const EARLY_CHECK_SIZE: usize = 256 * 1024;

/// Maximum total download: phase 1 + phase 2 combined. After reaching
/// this limit we use whatever metadata we have — no further reading.
const MAX_DOWNLOAD_SIZE: usize = 512 * 1024;

/// Request timeout for page and favicon fetches.
const FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum size for a favicon image download.
const MAX_FAVICON_SIZE: u64 = 256 * 1024;

// =========================================================
// Error types
// =========================================================

/// Errors specific to metadata fetching.
///
/// These are module-internal typed errors for distinguishing failure
/// modes. The service layer maps them to `MetadataResult` variants.
#[derive(Error, Debug)]
pub enum FetchError {
    #[error("HTTP request failed: {0}")]
    Http(#[from] anyhow::Error),

    #[error("response is not HTML (content-type: {content_type})")]
    NotHtml { content_type: String },

    #[error("response is not a valid image (content-type: {content_type})")]
    NotImage { content_type: String },
}

// =========================================================
// Page metadata fetching
// =========================================================

/// Fetch the root page of a domain and extract metadata.
///
/// Uses a two-phase streaming approach:
/// 1. Read up to 256 KB or `</head>`/`<body` marker, try extraction.
///    If all fields present → return early without downloading more.
/// 2. Continue reading up to 512 KB total, re-extract with full content.
///
/// Returns the best metadata we could extract, even if partial. The
/// caller decides what to do with incomplete results.
pub fn fetch_page_metadata(http: &Http, domain: &str) -> Result<PageMetadata, FetchError> {
    let url_string = format!("https://{domain}/");
    let base_url = Url::parse(&url_string).map_err(|e| FetchError::Http(e.into()))?;

    let mut response = http
        .get(&url_string)
        .timeout(FETCH_TIMEOUT)
        .max_size(MAX_DOWNLOAD_SIZE as u64)
        .send()
        .map_err(FetchError::Http)?;

    // Check Content-Type before reading the body. Servers that return
    // non-HTML responses (redirects to APIs, binary files, etc.) are
    // not useful for metadata extraction.
    let content_type = response.header("content-type").unwrap_or("").to_string();

    if !content_type.contains("text/html") {
        return Err(FetchError::NotHtml { content_type });
    }

    // =========================================================
    // Phase 1 — Stream initial portion and attempt early extraction
    // =========================================================
    //
    // Metadata tags (title, description, favicon) live in <head>,
    // which is typically well within the first few KB. We stream
    // chunks until we see an end-of-head marker or reach the early
    // check size, then try to extract.

    loop {
        match response.read_chunk() {
            Ok(Some(_chunk)) => {
                let accumulated = response.accumulated();

                // Check for end-of-head markers or size cap.
                let reached_limit = accumulated.len() >= EARLY_CHECK_SIZE;
                let found_marker = if !reached_limit {
                    let lower = String::from_utf8_lossy(accumulated).to_lowercase();
                    lower.contains("</head>") || lower.contains("<body")
                } else {
                    false
                };

                if reached_limit || found_marker {
                    let html = String::from_utf8_lossy(accumulated);
                    let early_metadata = metadata::extract_metadata(&html, &base_url);

                    if early_metadata.is_complete() {
                        response.abort();
                        return Ok(early_metadata);
                    }
                    break;
                }
            }
            Ok(None) => {
                // Stream exhausted before reaching the early check size.
                // Extract from whatever we have.
                let html = String::from_utf8_lossy(response.accumulated());
                return Ok(metadata::extract_metadata(&html, &base_url));
            }
            Err(e) => return Err(FetchError::Http(e)),
        }
    }

    // =========================================================
    // Phase 2 — Continue streaming up to MAX_DOWNLOAD_SIZE
    // =========================================================
    //
    // Metadata was incomplete from the initial portion. Read
    // the rest (up to the max limit) and try again with the
    // full document.

    let full_body = response.bytes().map_err(FetchError::Http)?;
    let html = String::from_utf8_lossy(&full_body);
    Ok(metadata::extract_metadata(&html, &base_url))
}

// =========================================================
// Favicon image fetching
// =========================================================

/// Download a favicon image and validate it.
///
/// Handles both HTTP URLs and `data:` URIs. For HTTP URLs, validates
/// that the response is an actual image using a three-step strategy:
/// 1. Trust the server's `Content-Type: image/*` header.
/// 2. Fall back to magic-byte detection via the `infer` crate.
/// 3. Check for SVG (`<svg` tag in first 4 KB).
pub fn fetch_favicon_image(http: &Http, favicon_url: &str) -> Result<FaviconImageData, FetchError> {
    // Handle data: URIs — decode inline, no network request needed.
    if let Some((mime, bytes)) = metadata::decode_data_uri(favicon_url) {
        return Ok(FaviconImageData {
            url: favicon_url.to_string(),
            image_data: bytes,
            content_type: mime,
        });
    }

    let mut response = http
        .get(favicon_url)
        .timeout(FETCH_TIMEOUT)
        .max_size(MAX_FAVICON_SIZE)
        .send()
        .map_err(FetchError::Http)?;

    let server_content_type = response.header("content-type").unwrap_or("").to_string();

    let bytes = response.bytes().map_err(FetchError::Http)?;

    // Strategy 1: Trust the server if it explicitly declares an image type.
    if server_content_type.starts_with("image/") {
        return Ok(FaviconImageData {
            url: favicon_url.to_string(),
            image_data: bytes,
            content_type: server_content_type,
        });
    }

    // Strategy 2: Magic-byte detection for raster formats.
    if let Some(kind) = infer::get(&bytes)
        && kind.matcher_type() == infer::MatcherType::Image
    {
        return Ok(FaviconImageData {
            url: favicon_url.to_string(),
            image_data: bytes,
            content_type: kind.mime_type().to_string(),
        });
    }

    // Strategy 3: SVG is text-based and not detected by `infer`.
    // Check for an `<svg` tag in the first few KB as a fallback.
    if metadata::is_likely_svg(&bytes) {
        return Ok(FaviconImageData {
            url: favicon_url.to_string(),
            image_data: bytes,
            content_type: "image/svg+xml".to_string(),
        });
    }

    Err(FetchError::NotImage {
        content_type: server_content_type,
    })
}
