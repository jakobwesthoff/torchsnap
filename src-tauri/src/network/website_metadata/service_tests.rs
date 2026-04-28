// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Integration tests for `WebsiteMetadataService`.
//!
//! Tests stand up a real `httpmock::MockServer`, point the service's
//! URL builder at it, and exercise the public API end-to-end. Mocks
//! are routed by per-domain path prefixes so multiple domains can
//! coexist on the same mock server.

use std::sync::Arc;

use httpmock::MockServer;
use httpmock::prelude::*;
use tempfile::TempDir;

use super::{MetadataResult, WebsiteMetadataService};
use crate::settings_notifier::SettingsNotifier;

// =========================================================
// Test harness
// =========================================================

/// All state needed to run a service test: the mock HTTP server,
/// a temporary directory that owns the cache + favicon files,
/// the settings notifier (kept alive so the watch thread stays
/// healthy), and the service itself.
pub(super) struct TestEnv {
    pub server: MockServer,
    pub service: Arc<WebsiteMetadataService>,
    // Kept alive — both own resources the service depends on. Never
    // read directly; the `Drop` order matters (service must drop first).
    _tmp: TempDir,
    _notifier: SettingsNotifier,
}

/// Build a service rooted at a fresh temp dir, with its URL builder
/// pointing at the given mock server. The service treats the input
/// domain as an opaque key — tests register routes at
/// `/<domain>/...` on the mock server and the URL builder emits
/// matching URLs.
pub(super) fn new_test_env() -> TestEnv {
    let server = MockServer::start();
    let tmp = TempDir::new().expect("create test temp dir");
    let notifier = SettingsNotifier::new();

    let server_base = server.base_url();
    let service = WebsiteMetadataService::new(tmp.path().to_path_buf(), &notifier, 30)
        .expect("construct service");
    let service = service.with_url_builder(move |domain, path| {
        format!("{server_base}/{domain}{path}")
    });

    TestEnv {
        server,
        service: Arc::new(service),
        _tmp: tmp,
        _notifier: notifier,
    }
}

/// Build a 16×16 solid-colour PNG at runtime. The `image` crate's
/// WebP encoder rejects 1×1 inputs (post-resize), so use a slightly
/// larger image when the favicon path goes through `process_icon`.
pub(super) fn make_png_bytes() -> Vec<u8> {
    use image::{ImageBuffer, ImageFormat, Rgba};
    let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
        ImageBuffer::from_pixel(16, 16, Rgba([0, 128, 255, 255]));
    let mut bytes: Vec<u8> = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut std::io::Cursor::new(&mut bytes), ImageFormat::Png)
        .expect("encode test PNG");
    bytes
}

pub(super) fn html_with_metadata(title: &str, desc: &str, favicon_path: &str) -> String {
    format!(
        r#"<!doctype html><html><head>
            <title>{title}</title>
            <meta name="description" content="{desc}" />
            <link rel="icon" type="image/png" href="{favicon_path}" />
        </head><body></body></html>"#
    )
}

/// Run `service.get(domain)` on the tokio blocking pool.
///
/// `Http::send()` calls `Handle::block_on` internally, which panics
/// when called from a tokio worker thread. Production callers are
/// always on `spawn_blocking` tasks; tests need the same dispatch.
pub(super) async fn blocking_get(
    service: &Arc<WebsiteMetadataService>,
    domain: &str,
) -> MetadataResult {
    let svc = Arc::clone(service);
    let domain = domain.to_string();
    tokio::task::spawn_blocking(move || svc.get(&domain))
        .await
        .expect("blocking task panicked")
}

// =========================================================
// Smoke tests against the current `get` / `try_cached` API
//
// These confirm the harness wiring works. Coalescing tests
// land alongside the in-flight refactor (Phase 1.2); the API
// migration tests land with the `lookup` refactor (Phase 1.3).
// =========================================================

#[tokio::test(flavor = "multi_thread")]
async fn get_returns_hit_for_complete_metadata() {
    let env = new_test_env();
    let domain = "example.test";

    let _page_mock = env.server.mock(|when, then| {
        when.method(GET).path(format!("/{domain}/"));
        then.status(200)
            .header("content-type", "text/html; charset=utf-8")
            .body(html_with_metadata(
                "Example Title",
                "An example page",
                "/icon.png",
            ));
    });
    let _icon_mock = env.server.mock(|when, then| {
        when.method(GET).path(format!("/{domain}/icon.png"));
        then.status(200).header("content-type", "image/png").body(make_png_bytes());
    });

    let result = blocking_get(&env.service, domain).await;

    let meta = match result {
        MetadataResult::Found(m) => m,
        other => panic!("expected Found, got {other:?}"),
    };
    assert_eq!(meta.title.as_deref(), Some("Example Title"));
    assert_eq!(meta.description.as_deref(), Some("An example page"));
}

#[tokio::test(flavor = "multi_thread")]
async fn get_returns_unreachable_for_failed_fetch() {
    let env = new_test_env();
    let domain = "broken.test";

    // No page mock registered — httpmock returns 404 for unknown paths.
    // Favicon fallback paths also miss, so the result is Unreachable
    // (server replies but content-type isn't text/html).
    let _page_mock = env.server.mock(|when, then| {
        when.method(GET).path(format!("/{domain}/"));
        then.status(500);
    });

    match blocking_get(&env.service, domain).await {
        // 500 with no content-type → fetch_page_metadata succeeds at the
        // HTTP layer (returns NotHtml), so we hit the fallback-favicon
        // path. With no favicons registered either, ReachableNoData.
        MetadataResult::ReachableNoData | MetadataResult::Unreachable => {}
        other => panic!("expected ReachableNoData/Unreachable, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn get_caches_result_so_second_call_skips_network() {
    let env = new_test_env();
    let domain = "cached.test";

    let page_mock = env.server.mock(|when, then| {
        when.method(GET).path(format!("/{domain}/"));
        then.status(200)
            .header("content-type", "text/html")
            .body(html_with_metadata("T", "D", "/i.png"));
    });
    let _icon_mock = env.server.mock(|when, then| {
        when.method(GET).path(format!("/{domain}/i.png"));
        then.status(200).header("content-type", "image/png").body(make_png_bytes());
    });

    let _ = blocking_get(&env.service, domain).await;
    let _ = blocking_get(&env.service, domain).await;

    // Page should have been fetched exactly once — the second call
    // is served from the SQLite cache.
    page_mock.assert_calls(1);
}

#[tokio::test(flavor = "multi_thread")]
async fn get_caches_reachable_no_data_response() {
    let env = new_test_env();
    let domain = "noisy.test";

    // 200 with text/plain → fetch returns NotHtml → service writes a
    // `ReachableNoData` row to SQLite, so the second call short-circuits
    // without another network request.
    let page_mock = env.server.mock(|when, then| {
        when.method(GET).path(format!("/{domain}/"));
        then.status(200).header("content-type", "text/plain").body("nope");
    });

    let _ = blocking_get(&env.service, domain).await;
    let _ = blocking_get(&env.service, domain).await;

    page_mock.assert_calls(1);
}

// MetadataResult has no `#[derive(Debug)]` so panic-message formatting
// uses this manual impl. Kept here (test-only) to avoid leaking Debug
// into the production type.
impl std::fmt::Debug for MetadataResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MetadataResult::Found(_) => write!(f, "Found(..)"),
            MetadataResult::ReachableNoData => write!(f, "ReachableNoData"),
            MetadataResult::Unreachable => write!(f, "Unreachable"),
        }
    }
}
