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

use super::{LookupError, LookupMode, LookupResult, WebsiteMetadataService};
use crate::settings::notifier::SettingsNotifier;

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

/// Run `service.lookup(domain, Blocking)` on the tokio blocking pool.
///
/// `Http::send()` calls `Handle::block_on` internally, which panics
/// when called from a tokio worker thread. Production callers are
/// always on `spawn_blocking` tasks; tests need the same dispatch.
pub(super) async fn blocking_lookup(
    service: &Arc<WebsiteMetadataService>,
    domain: &str,
) -> LookupResult {
    let svc = Arc::clone(service);
    let domain = domain.to_string();
    tokio::task::spawn_blocking(move || svc.lookup(&domain, LookupMode::Blocking))
        .await
        .expect("blocking task panicked")
        .expect("valid domain")
}

// =========================================================
// Happy-path lookup tests
// =========================================================

#[tokio::test(flavor = "multi_thread")]
async fn lookup_blocking_returns_hit_for_complete_metadata() {
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

    let result = blocking_lookup(&env.service, domain).await;

    let meta = match result {
        LookupResult::Hit(m) => m,
        other => panic!("expected Found, got {other:?}"),
    };
    assert_eq!(meta.title.as_deref(), Some("Example Title"));
    assert_eq!(meta.description.as_deref(), Some("An example page"));
}

#[tokio::test(flavor = "multi_thread")]
async fn lookup_blocking_returns_unreachable_or_no_data_for_failed_fetch() {
    let env = new_test_env();
    let domain = "broken.test";

    // No page mock registered — httpmock returns 404 for unknown paths.
    // Favicon fallback paths also miss, so the result is Unreachable
    // (server replies but content-type isn't text/html).
    let _page_mock = env.server.mock(|when, then| {
        when.method(GET).path(format!("/{domain}/"));
        then.status(500);
    });

    match blocking_lookup(&env.service, domain).await {
        // 500 with no content-type → fetch_page_metadata succeeds at the
        // HTTP layer (returns NotHtml), so we hit the fallback-favicon
        // path. With no favicons registered either, ReachableNoData.
        LookupResult::ReachableNoData | LookupResult::Unreachable => {}
        other => panic!("expected ReachableNoData/Unreachable, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn lookup_caches_result_so_second_call_skips_network() {
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

    let _ = blocking_lookup(&env.service, domain).await;
    let _ = blocking_lookup(&env.service, domain).await;

    // Page should have been fetched exactly once — the second call
    // is served from the SQLite cache.
    page_mock.assert_calls(1);
}

#[tokio::test(flavor = "multi_thread")]
async fn lookup_caches_reachable_no_data_response() {
    let env = new_test_env();
    let domain = "noisy.test";

    // 200 with text/plain → fetch returns NotHtml → service writes a
    // `ReachableNoData` row to SQLite, so the second call short-circuits
    // without another network request.
    let page_mock = env.server.mock(|when, then| {
        when.method(GET).path(format!("/{domain}/"));
        then.status(200).header("content-type", "text/plain").body("nope");
    });

    let _ = blocking_lookup(&env.service, domain).await;
    let _ = blocking_lookup(&env.service, domain).await;

    page_mock.assert_calls(1);
}

// =========================================================
// Coalescing tests
//
// These exercise `fetch_coalesced`: concurrent callers for the
// same domain must share a single network request; concurrent
// callers for different domains must not interfere; a panicking
// leader must unblock subscribers via `LeaderGuard`.
// =========================================================

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_blocking_lookups_for_same_domain_issue_one_request() {
    let env = new_test_env();
    let domain = "shared.test";

    let page_mock = env.server.mock(|when, then| {
        when.method(GET).path(format!("/{domain}/"));
        // Small delay so concurrent callers reliably register as
        // subscribers before the leader publishes.
        then.status(200)
            .header("content-type", "text/html")
            .delay(std::time::Duration::from_millis(150))
            .body(html_with_metadata("Title", "Desc", "/i.png"));
    });
    let _icon_mock = env.server.mock(|when, then| {
        when.method(GET).path(format!("/{domain}/i.png"));
        then.status(200)
            .header("content-type", "image/png")
            .body(make_png_bytes());
    });

    // 8 concurrent blocking-mode fetches for the same domain.
    let mut handles = Vec::new();
    for _ in 0..8 {
        let svc = Arc::clone(&env.service);
        let d = domain.to_string();
        handles.push(tokio::task::spawn_blocking(move || {
            svc.lookup(&d, LookupMode::Blocking)
                .expect("valid domain")
        }));
    }
    for h in handles {
        let result = h.await.expect("task panicked");
        match result {
            LookupResult::Hit(m) => {
                assert_eq!(m.title.as_deref(), Some("Title"));
            }
            other => panic!("expected Hit, got {other:?}"),
        }
    }

    // Single network request despite eight concurrent callers.
    page_mock.assert_calls(1);
}

#[tokio::test(flavor = "multi_thread")]
async fn different_domains_do_not_coalesce() {
    let env = new_test_env();

    let mock_a = env.server.mock(|when, then| {
        when.method(GET).path("/a.test/");
        then.status(200)
            .header("content-type", "text/html")
            .body(html_with_metadata("A", "DA", "/i.png"));
    });
    let mock_b = env.server.mock(|when, then| {
        when.method(GET).path("/b.test/");
        then.status(200)
            .header("content-type", "text/html")
            .body(html_with_metadata("B", "DB", "/i.png"));
    });
    // Both domains share an icon route — irrelevant to coalescing.
    let _icon_a = env.server.mock(|when, then| {
        when.method(GET).path("/a.test/i.png");
        then.status(200)
            .header("content-type", "image/png")
            .body(make_png_bytes());
    });
    let _icon_b = env.server.mock(|when, then| {
        when.method(GET).path("/b.test/i.png");
        then.status(200)
            .header("content-type", "image/png")
            .body(make_png_bytes());
    });

    let svc_a = Arc::clone(&env.service);
    let svc_b = Arc::clone(&env.service);
    let task_a = tokio::task::spawn_blocking(move || {
        svc_a.lookup("a.test", LookupMode::Blocking).expect("valid")
    });
    let task_b = tokio::task::spawn_blocking(move || {
        svc_b.lookup("b.test", LookupMode::Blocking).expect("valid")
    });

    let result_a = task_a.await.expect("task a panicked");
    let result_b = task_b.await.expect("task b panicked");

    assert!(matches!(result_a, LookupResult::Hit(_)));
    assert!(matches!(result_b, LookupResult::Hit(_)));

    mock_a.assert_calls(1);
    mock_b.assert_calls(1);
}

#[tokio::test(flavor = "multi_thread")]
async fn cached_lookup_during_in_flight_blocking_coalesces() {
    let env = new_test_env();
    let domain = "shared2.test";

    let page_mock = env.server.mock(|when, then| {
        when.method(GET).path(format!("/{domain}/"));
        then.status(200)
            .header("content-type", "text/html")
            .delay(std::time::Duration::from_millis(150))
            .body(html_with_metadata("Title", "Desc", "/i.png"));
    });
    let _icon_mock = env.server.mock(|when, then| {
        when.method(GET).path(format!("/{domain}/i.png"));
        then.status(200)
            .header("content-type", "image/png")
            .body(make_png_bytes());
    });

    let svc1 = Arc::clone(&env.service);
    let svc2 = Arc::clone(&env.service);
    let leader = tokio::task::spawn_blocking(move || {
        svc1.lookup(domain, LookupMode::Blocking).expect("valid")
    });

    // Wait briefly so the leader registers in `in_flight` before the
    // background fetch is triggered.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    // Cached-mode lookup spawns a background fetch — coalescing inside
    // `fetch_coalesced` joins the in-flight leader as a subscriber and
    // exits without a second network call. The caller still gets
    // `Pending` immediately (no SQLite row exists yet).
    let cached_result = svc2.lookup(domain, LookupMode::Cached).expect("valid");
    assert!(matches!(cached_result, LookupResult::Pending));

    let _ = leader.await.expect("leader panicked");

    // Sleep just long enough for any duplicate-fetch task to have
    // started, in case coalescing is broken.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    page_mock.assert_calls(1);
}

#[tokio::test(flavor = "multi_thread")]
async fn leader_panic_unblocks_subscribers_with_unreachable() {
    let env = new_test_env();
    let domain = "boom.test";

    // Sleep 100 ms before panicking — gives subscribers time to
    // register on the in_flight slot the leader just inserted.
    env.service
        .set_test_panic_after(domain, std::time::Duration::from_millis(100));

    let svc1 = Arc::clone(&env.service);
    let leader = tokio::task::spawn_blocking(move || {
        svc1.lookup(domain, LookupMode::Blocking).expect("valid")
    });

    // Wait for the leader to register before launching subscribers.
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;

    let svc2 = Arc::clone(&env.service);
    let svc3 = Arc::clone(&env.service);
    let sub_a = tokio::task::spawn_blocking(move || {
        svc2.lookup(domain, LookupMode::Blocking).expect("valid")
    });
    let sub_b = tokio::task::spawn_blocking(move || {
        svc3.lookup(domain, LookupMode::Blocking).expect("valid")
    });

    // Leader's spawn_blocking task surfaces the panic as a JoinError.
    let leader_outcome = leader.await;
    assert!(
        leader_outcome.is_err(),
        "leader task should have panicked, got {leader_outcome:?}"
    );

    // Subscribers must NOT panic — they receive the LeaderGuard fallback.
    let sub_a_result = sub_a.await.expect("subscriber a should not panic");
    let sub_b_result = sub_b.await.expect("subscriber b should not panic");

    assert!(matches!(sub_a_result, LookupResult::Unreachable));
    assert!(matches!(sub_b_result, LookupResult::Unreachable));
}

#[tokio::test(flavor = "multi_thread")]
async fn coalescing_clears_in_flight_after_completion() {
    // After a fetch finishes, the in_flight entry must be removed so
    // subsequent fetches for the same domain — once cache expires —
    // can issue a fresh request rather than coalescing forever.
    let env = new_test_env();
    let domain = "cleared.test";

    let page_mock = env.server.mock(|when, then| {
        when.method(GET).path(format!("/{domain}/"));
        then.status(200)
            .header("content-type", "text/plain")
            .body("plain");
    });

    // First call writes a ReachableNoData row to SQLite.
    let _ = blocking_lookup(&env.service, domain).await;

    // The second call hits SQLite and never enters fetch_coalesced.
    // What we actually verify here is that in_flight is empty after
    // the leader completes (no leftover entry).
    {
        let map = env
            .service
            .in_flight
            .lock()
            .expect("in_flight not poisoned");
        assert!(
            map.is_empty(),
            "in_flight should be empty after leader completion"
        );
    }
    page_mock.assert_calls(1);
}

// =========================================================
// Cached-mode tests
// =========================================================

#[tokio::test(flavor = "multi_thread")]
async fn cached_lookup_returns_pending_on_cold_miss_then_hit() {
    let env = new_test_env();
    let domain = "warm.test";

    let _page_mock = env.server.mock(|when, then| {
        when.method(GET).path(format!("/{domain}/"));
        then.status(200)
            .header("content-type", "text/html")
            .body(html_with_metadata("Title", "Desc", "/i.png"));
    });
    let _icon_mock = env.server.mock(|when, then| {
        when.method(GET).path(format!("/{domain}/i.png"));
        then.status(200)
            .header("content-type", "image/png")
            .body(make_png_bytes());
    });

    // Cold miss → Pending; the background fetch is now in flight.
    let first = env.service.lookup(domain, LookupMode::Cached).expect("valid");
    assert!(matches!(first, LookupResult::Pending));

    // Wait for the background fetch to populate the SQLite cache.
    // 500 ms is generous for a localhost mock; tighten if flaky.
    let svc = Arc::clone(&env.service);
    let domain_owned = domain.to_string();
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        loop {
            let r = svc.lookup(&domain_owned, LookupMode::Cached).expect("valid");
            if matches!(r, LookupResult::Hit(_)) {
                return r;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("Cached lookup never resolved to Hit");
}

#[tokio::test(flavor = "multi_thread")]
async fn cached_lookup_returns_hit_immediately_when_cached() {
    // Pre-populate the cache via a Blocking lookup, then a Cached
    // lookup must short-circuit to Hit without spawning anything.
    let env = new_test_env();
    let domain = "preheated.test";

    let page_mock = env.server.mock(|when, then| {
        when.method(GET).path(format!("/{domain}/"));
        then.status(200)
            .header("content-type", "text/html")
            .body(html_with_metadata("T", "D", "/i.png"));
    });
    let _icon_mock = env.server.mock(|when, then| {
        when.method(GET).path(format!("/{domain}/i.png"));
        then.status(200)
            .header("content-type", "image/png")
            .body(make_png_bytes());
    });

    let _ = blocking_lookup(&env.service, domain).await;
    let cached = env.service.lookup(domain, LookupMode::Cached).expect("valid");
    assert!(matches!(cached, LookupResult::Hit(_)));

    // Only one network round-trip happened.
    page_mock.assert_calls(1);
}

// =========================================================
// Domain validation tests
//
// These exercise `validate_domain` via the public API. The
// underlying helper is private; testing through `lookup` keeps
// the validation contract observable from the public surface.
// =========================================================

fn assert_invalid(env: &TestEnv, input: &str) {
    let err = env.service.lookup(input, LookupMode::Blocking).unwrap_err();
    match err {
        LookupError::InvalidDomain(_) => {}
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn lookup_rejects_empty_domain() {
    let env = new_test_env();
    assert_invalid(&env, "");
}

#[tokio::test(flavor = "multi_thread")]
async fn lookup_rejects_oversized_domain() {
    let env = new_test_env();
    let too_long: String = "a".repeat(254);
    assert_invalid(&env, &too_long);
}

#[tokio::test(flavor = "multi_thread")]
async fn lookup_rejects_domain_with_scheme_separator() {
    let env = new_test_env();
    assert_invalid(&env, "https://example.com");
}

#[tokio::test(flavor = "multi_thread")]
async fn lookup_rejects_domain_with_path() {
    let env = new_test_env();
    assert_invalid(&env, "example.com/path");
}

#[tokio::test(flavor = "multi_thread")]
async fn lookup_rejects_domain_with_port() {
    let env = new_test_env();
    assert_invalid(&env, "example.com:8080");
}

#[tokio::test(flavor = "multi_thread")]
async fn lookup_rejects_domain_with_whitespace() {
    let env = new_test_env();
    assert_invalid(&env, "example .com");
    assert_invalid(&env, "example.com\n");
    assert_invalid(&env, "\texample.com");
}

#[tokio::test(flavor = "multi_thread")]
async fn lookup_rejects_domain_with_null_byte() {
    let env = new_test_env();
    assert_invalid(&env, "example\0.com");
}

#[tokio::test(flavor = "multi_thread")]
async fn lookup_rejects_leading_or_trailing_dot() {
    let env = new_test_env();
    assert_invalid(&env, ".example.com");
    assert_invalid(&env, "example.com.");
}

#[tokio::test(flavor = "multi_thread")]
async fn lookup_accepts_idn_punycode() {
    // Punycode domains are pure ASCII; the host should accept them
    // as it does any other syntactically valid bare domain. The
    // network mock doesn't have a route for this name — we only
    // verify that validation passes, not that the fetch succeeds.
    let env = new_test_env();
    let domain = "xn--bcher-kva.example";

    // Wrap in spawn_blocking because the Blocking lookup ultimately
    // calls `Http::send` → `Handle::block_on`, which must not run on
    // a tokio worker thread.
    let svc = Arc::clone(&env.service);
    let d = domain.to_string();
    let r = tokio::task::spawn_blocking(move || svc.lookup(&d, LookupMode::Blocking))
        .await
        .expect("task panicked");
    assert!(r.is_ok(), "punycode domain rejected: {r:?}");
}

// `LookupResult` has no `#[derive(Debug)]` so panic-message formatting
// uses this manual impl. Kept here (test-only) to avoid leaking Debug
// into the production type.
impl std::fmt::Debug for LookupResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LookupResult::Hit(_) => write!(f, "Hit(..)"),
            LookupResult::ReachableNoData => write!(f, "ReachableNoData"),
            LookupResult::Unreachable => write!(f, "Unreachable"),
            LookupResult::Pending => write!(f, "Pending"),
        }
    }
}
