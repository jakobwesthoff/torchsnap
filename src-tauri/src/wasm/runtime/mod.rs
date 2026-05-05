// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// WASM Plugin Runtime
//
// Manages the wasmtime engine (shared across all plugins)
// and provides per-plugin instances that wrap a Store and
// typed component bindings.
//
// Architecture:
//   WasmRuntime (one per app, owns Engine + Component cache)
//    ├── compile(plugin_id, wasm_bytes)
//    └── instantiate(plugin_id) → WasmGadgetInstance
//
//   WasmGadgetInstance (one per plugin, owns Store + Plugin)
//    ├── enable() / disable()
//    ├── entries() → Vec<CatalogEntry>
//    └── execute(entry_id, action_id) → PostAction
//
// The compile/instantiate split lets the expensive step run
// once per plugin while instantiation stays cheap enough to
// repeat on demand. See ADR 0033.
// =========================================================

// =========================================================
// Module layout
//
// `runtime` is split by concern:
//
//   - `engine`   — `WasmRuntime`: component compile + cache,
//                  linker construction, instantiation.
//   - `state`    — `GadgetState` struct (the wasmtime store
//                  data) plus the foundational
//                  `Default::default()` / `default_for_test`
//                  constructors and the `WasiView` impl.
//   - `instance` — `WasmGadgetInstance` lifecycle wrapper
//                  with foundational setters
//                  (`set_path_context`, `set_plugin_source`)
//                  and guest-call dispatch (`enable`,
//                  `disable`, `entries`, `search`,
//                  `execute`, `handle_message`, `run_task`,
//                  `on_setting_changed`).
//   - `host/`    — one file per WIT host import. Each owns
//                  its state sub-struct, Host trait impl,
//                  helpers, and `impl WasmGadgetInstance`
//                  blocks for the capability-specific
//                  setters/clearers.
//
// Tests live in this file's `tests` module so all the
// integration tests share the fixture-loading helpers
// (`test_runtime()`, fixture WASM byte constants).
// =========================================================

pub mod engine;
pub mod host;
pub mod instance;
pub mod state;

pub use engine::WasmRuntime;
pub use host::sql::{SqlConfig, SqlHandleEntry};
pub use instance::WasmGadgetInstance;
pub use state::GadgetState;

#[cfg(test)]
use std::sync::{Arc, Mutex};

#[cfg(test)]
use super::bindings;
#[cfg(test)]
use super::logging::channel::LogSender;
#[cfg(test)]
use super::logging::spans::SpanRegistry;
#[cfg(test)]
use host::http::HttpState;

#[cfg(test)]
mod tests {
    use super::*;

    /// Bytes of the committed `minimal-plugin` fixture. The
    /// fixture is a standalone `wasm32-wasip2` crate under
    /// `src-tauri/tests/fixtures/minimal-plugin/` whose guest
    /// implements every WIT export as a no-op. See that
    /// directory's README for rebuild instructions.
    const MINIMAL_PLUGIN_WASM: &[u8] =
        include_bytes!("../../../tests/fixtures/minimal-plugin/minimal_plugin.wasm");

    /// Construct a bare `WasmRuntime` for tests. Uses a
    /// discarding `LogSender` and a fresh `SpanRegistry` so
    /// tests do not depend on a running logging task.
    fn test_runtime() -> Arc<WasmRuntime> {
        // No metadata service in the default test runtime — tests
        // exercising the website-metadata host import construct one
        // and pass it explicitly via a dedicated helper.
        WasmRuntime::new(
            LogSender::test_sender(),
            Arc::new(SpanRegistry::new()),
            None,
        )
        .expect("WasmRuntime::new should succeed with default config")
    }

    #[test]
    fn compile_then_instantiate_succeeds() {
        let runtime = test_runtime();
        runtime
            .compile("minimal", MINIMAL_PLUGIN_WASM)
            .expect("compile valid fixture");
        let instance = runtime.instantiate("minimal").expect("instantiate");
        instance.enable().expect("guest enable no-op");
    }

    #[test]
    fn compile_caches_component() {
        // No direct recompilation counter to assert on; we
        // verify the cache retains the entry after two
        // successful instantiates instead.
        let runtime = test_runtime();
        runtime
            .compile("minimal", MINIMAL_PLUGIN_WASM)
            .expect("compile");
        let _first = runtime.instantiate("minimal").expect("first instantiate");
        let _second = runtime.instantiate("minimal").expect("second instantiate");

        assert!(
            runtime
                .components
                .lock()
                .expect("cache lock")
                .contains_key("minimal")
        );
    }

    #[test]
    fn instantiate_without_compile_errors() {
        let runtime = test_runtime();
        let err = runtime
            .instantiate("never-compiled")
            .err()
            .expect("instantiate returns Err for unknown id");
        let msg = format!("{err:#}");
        assert!(
            msg.contains("never-compiled") && msg.contains("compile"),
            "error should name the missing plugin and hint at `compile` (got: {msg})"
        );
    }

    #[test]
    fn compile_invalid_bytes_errors() {
        // Both checks matter: the return is an `Err` AND the
        // cache stays empty, so a subsequent `instantiate`
        // cannot silently succeed against a partial entry.
        let runtime = test_runtime();
        assert!(
            runtime
                .compile("garbage", b"not a wasm component at all")
                .is_err()
        );
        assert!(
            !runtime
                .components
                .lock()
                .expect("cache lock")
                .contains_key("garbage")
        );
        assert!(runtime.instantiate("garbage").is_err());
    }

    #[test]
    fn compile_replaces_existing_entry() {
        // Pins the "re-compile replaces" semantic so a future
        // refactor that accidentally rejects duplicate compiles
        // would trip this assertion.
        let runtime = test_runtime();
        runtime
            .compile("minimal", MINIMAL_PLUGIN_WASM)
            .expect("first compile");
        runtime
            .compile("minimal", MINIMAL_PLUGIN_WASM)
            .expect("second compile replaces");
        runtime.instantiate("minimal").expect("instantiate");
    }

    #[test]
    fn instances_are_independent() {
        // Two instances from the same cached Component must
        // have disjoint GadgetStates: operating on one must
        // not disturb the other.
        let runtime = test_runtime();
        runtime
            .compile("minimal", MINIMAL_PLUGIN_WASM)
            .expect("compile");

        let first = runtime.instantiate("minimal").expect("first");
        let second = runtime.instantiate("minimal").expect("second");

        first.clear_sql_storage();
        second
            .enable()
            .expect("second instance untouched by operations on the first");
    }

    /// Bytes of the committed `website-metadata-plugin` fixture.
    /// See the fixture crate's README for rebuild instructions.
    const WEBSITE_METADATA_PLUGIN_WASM: &[u8] = include_bytes!(
        "../../../tests/fixtures/website-metadata-plugin/website_metadata_plugin.wasm"
    );

    /// Bytes of the committed `opener-http-plugin` fixture.
    /// Exercises the `opener` and `http` host interfaces via
    /// `messaging::handle-message` dispatch.
    const OPENER_HTTP_PLUGIN_WASM: &[u8] =
        include_bytes!("../../../tests/fixtures/opener-http-plugin/opener_http_plugin.wasm");

    /// Bytes of the committed `assets-plugin` fixture.
    /// Exercises the `assets` host interface via
    /// `messaging::handle-message` dispatch.
    const ASSETS_PLUGIN_WASM: &[u8] =
        include_bytes!("../../../tests/fixtures/assets-plugin/assets_plugin.wasm");

    /// Bytes of the committed `command-plugin` fixture.
    /// Exercises the `command` host interface via
    /// `messaging::handle-message` dispatch.
    #[cfg(unix)]
    const COMMAND_PLUGIN_WASM: &[u8] =
        include_bytes!("../../../tests/fixtures/command-plugin/command_plugin.wasm");

    // =========================================================
    // Unit tests for opener/http pure functions
    //
    // These functions have no WASM or tokio dependency so the
    // tests are fast synchronous assertions.
    // =========================================================

    fn strs(ss: &[&str]) -> Vec<String> {
        ss.iter().map(|s| s.to_string()).collect()
    }

    // ---- check_opener_scheme --------------------------------

    use super::host::opener::{OpenerSchemeCheckError, check_opener_scheme};

    #[test]
    fn opener_permitted_scheme_passes() {
        assert!(check_opener_scheme(&strs(&["https"]), "https://example.com").is_ok());
    }

    #[test]
    fn opener_forbidden_scheme_blocked() {
        let err = check_opener_scheme(&strs(&["https"]), "ftp://example.com").unwrap_err();
        match err {
            OpenerSchemeCheckError::SchemeNotPermitted(scheme) => assert_eq!(scheme, "ftp"),
            other => panic!("expected SchemeNotPermitted, got {other:?}"),
        }
    }

    #[test]
    fn opener_empty_allowlist_denies_everything() {
        assert!(check_opener_scheme(&[], "https://example.com").is_err());
    }

    #[test]
    fn opener_unparseable_url_returns_error() {
        assert!(check_opener_scheme(&strs(&["https"]), "not-a-url").is_err());
    }

    #[test]
    fn opener_scheme_check_is_case_insensitive() {
        // The `url` crate normalizes schemes to lowercase, so an
        // uppercase scheme in the URL still matches the allowlist.
        assert!(check_opener_scheme(&strs(&["https"]), "HTTPS://example.com").is_ok());
    }

    #[test]
    fn opener_multiple_schemes_second_matches() {
        assert!(check_opener_scheme(&strs(&["https", "mailto"]), "mailto:user@x.com").is_ok());
    }

    // ---- check_http_origin ----------------------------------

    use super::host::http::{WasmHttpError, check_http_origin, wit_method_to_reqwest};

    #[test]
    fn http_exact_origin_match_passes() {
        assert!(
            check_http_origin(&strs(&["https://example.com"]), "https://example.com/path").is_ok()
        );
    }

    #[test]
    fn http_non_matching_origin_denied() {
        let err =
            check_http_origin(&strs(&["https://example.com"]), "https://other.com/x").unwrap_err();
        assert!(matches!(err, WasmHttpError::PermissionDenied(_)));
    }

    #[test]
    fn http_wildcard_allows_any_origin() {
        assert!(check_http_origin(&strs(&["*"]), "https://any-host.example/path").is_ok());
    }

    #[test]
    fn http_empty_origins_denies_everything() {
        assert!(check_http_origin(&[], "https://example.com").is_err());
    }

    #[test]
    fn http_non_default_port_included_in_origin() {
        assert!(
            check_http_origin(
                &strs(&["https://example.com:8443"]),
                "https://example.com:8443/resource"
            )
            .is_ok()
        );
    }

    #[test]
    fn http_default_port_stripped_from_origin() {
        // https:443 normalizes to the same origin as https (no port).
        assert!(
            check_http_origin(
                &strs(&["https://example.com"]),
                "https://example.com:443/resource"
            )
            .is_ok()
        );
    }

    #[test]
    fn http_wrong_port_is_different_origin() {
        let err = check_http_origin(
            &strs(&["https://example.com"]),
            "https://example.com:8080/x",
        )
        .unwrap_err();
        assert!(matches!(err, WasmHttpError::PermissionDenied(_)));
    }

    #[test]
    fn http_unparseable_url_returns_invalid_url_error() {
        let err = check_http_origin(&strs(&["https://example.com"]), "not-a-url").unwrap_err();
        assert!(matches!(err, WasmHttpError::InvalidUrl(_)));
    }

    #[test]
    fn http_wildcard_short_circuits_before_url_parse() {
        // The wildcard check happens before `url::Url::parse`, so an
        // unparseable URL is still allowed when the list contains `"*"`.
        assert!(check_http_origin(&strs(&["*"]), "not-a-url").is_ok());
    }

    // ---- wit_method_to_reqwest ------------------------------

    #[test]
    fn wit_method_named_variants_map_correctly() {
        use bindings::torchsnap::gadget::http::HttpMethod;
        let cases = [
            (HttpMethod::Get, reqwest::Method::GET),
            (HttpMethod::Post, reqwest::Method::POST),
            (HttpMethod::Put, reqwest::Method::PUT),
            (HttpMethod::Patch, reqwest::Method::PATCH),
            (HttpMethod::Delete, reqwest::Method::DELETE),
            (HttpMethod::Head, reqwest::Method::HEAD),
        ];
        for (wit, expected) in cases {
            assert_eq!(wit_method_to_reqwest(wit).unwrap(), expected,);
        }
    }

    #[test]
    fn wit_method_other_valid_string() {
        use bindings::torchsnap::gadget::http::HttpMethod;
        let method = wit_method_to_reqwest(HttpMethod::Other("PROPFIND".into())).unwrap();
        assert_eq!(method.as_str(), "PROPFIND");
    }

    #[test]
    fn wit_method_other_invalid_string_returns_other_error() {
        use bindings::torchsnap::gadget::http::HttpMethod;
        let err = wit_method_to_reqwest(HttpMethod::Other("has space".into())).unwrap_err();
        assert!(matches!(err, WasmHttpError::Other(_)));
    }

    // ---- WasmHttpError → HttpError conversion ---------------

    #[test]
    fn wasm_http_error_conversion_all_variants() {
        use bindings::torchsnap::gadget::http::HttpError;
        let cases: Vec<(WasmHttpError, HttpError)> = vec![
            (
                WasmHttpError::PermissionDenied("origin".into()),
                HttpError::PermissionDenied("origin".into()),
            ),
            (
                WasmHttpError::ConnectionRefused("connect".into()),
                HttpError::ConnectionRefused("connect".into()),
            ),
            (WasmHttpError::Timeout, HttpError::Timeout),
            (
                WasmHttpError::DnsFailed("dns".into()),
                HttpError::DnsFailed("dns".into()),
            ),
            (
                WasmHttpError::TlsFailed("tls".into()),
                HttpError::TlsFailed("tls".into()),
            ),
            (
                WasmHttpError::InvalidUrl("url".into()),
                HttpError::InvalidUrl("url".into()),
            ),
            (
                WasmHttpError::Other("other".into()),
                HttpError::Other("other".into()),
            ),
        ];
        for (input, expected) in cases {
            let converted: HttpError = input.into();
            assert!(
                std::mem::discriminant(&converted) == std::mem::discriminant(&expected),
                "discriminant mismatch"
            );
        }
    }

    // ---- httpmock integration tests for http::fetch ---------
    //
    // These tests exercise the full `Http` builder pipeline in
    // `http::Host::fetch` — specifically that headers, body,
    // status, and response headers are forwarded correctly.
    // They don't need a live WASM instance; they call the
    // private pieces directly.

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_forwards_request_headers() {
        use httpmock::MockServer;
        use httpmock::prelude::*;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(GET)
                .path("/test")
                .header("x-custom", "value123");
            then.status(200).body("ok");
        });

        let client = crate::network::Http::new();
        let request = bindings::torchsnap::gadget::http::HttpRequest {
            url: server.url("/test"),
            method: bindings::torchsnap::gadget::http::HttpMethod::Get,
            headers: vec![("x-custom".into(), "value123".into())],
            body: None,
            timeout_ms: None,
            max_body_size: None,
            insecure_tls: false,
        };

        let mut state = GadgetState {
            http: HttpState {
                origins: vec!["*".into()],
                client: Some(Arc::new(client)),
                insecure_client: Arc::new(std::sync::OnceLock::new()),
            },
            ..GadgetState::default_for_test()
        };

        use bindings::torchsnap::gadget::http::Host;
        let resp = state.fetch(request).expect("fetch should succeed");
        assert_eq!(resp.status, 200);
        mock.assert();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_forwards_request_body() {
        use httpmock::MockServer;
        use httpmock::prelude::*;

        let server = MockServer::start();
        let mock = server.mock(|when, then| {
            when.method(POST).path("/data").body("hello");
            then.status(201);
        });

        let client = crate::network::Http::new();
        let request = bindings::torchsnap::gadget::http::HttpRequest {
            url: server.url("/data"),
            method: bindings::torchsnap::gadget::http::HttpMethod::Post,
            headers: vec![],
            body: Some(b"hello".to_vec()),
            timeout_ms: None,
            max_body_size: None,
            insecure_tls: false,
        };

        let mut state = GadgetState {
            http: HttpState {
                origins: vec!["*".into()],
                client: Some(Arc::new(client)),
                insecure_client: Arc::new(std::sync::OnceLock::new()),
            },
            ..GadgetState::default_for_test()
        };

        use bindings::torchsnap::gadget::http::Host;
        let resp = state.fetch(request).expect("fetch should succeed");
        assert_eq!(resp.status, 201);
        mock.assert();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_returns_response_status_and_headers() {
        use httpmock::MockServer;
        use httpmock::prelude::*;

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/head-test");
            then.status(404).header("x-resp-header", "present");
        });

        let client = crate::network::Http::new();
        let request = bindings::torchsnap::gadget::http::HttpRequest {
            url: server.url("/head-test"),
            method: bindings::torchsnap::gadget::http::HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout_ms: None,
            max_body_size: None,
            insecure_tls: false,
        };

        let mut state = GadgetState {
            http: HttpState {
                origins: vec!["*".into()],
                client: Some(Arc::new(client)),
                insecure_client: Arc::new(std::sync::OnceLock::new()),
            },
            ..GadgetState::default_for_test()
        };

        use bindings::torchsnap::gadget::http::Host;
        let resp = state.fetch(request).expect("fetch should succeed");
        assert_eq!(resp.status, 404);
        let has_header = resp
            .headers
            .iter()
            .any(|(k, v)| k.eq_ignore_ascii_case("x-resp-header") && v == "present");
        assert!(has_header, "expected x-resp-header in response headers");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn fetch_returns_response_body() {
        use httpmock::MockServer;
        use httpmock::prelude::*;

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/body");
            then.status(200).body(b"hello body".as_slice());
        });

        let client = crate::network::Http::new();
        let request = bindings::torchsnap::gadget::http::HttpRequest {
            url: server.url("/body"),
            method: bindings::torchsnap::gadget::http::HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout_ms: None,
            max_body_size: None,
            insecure_tls: false,
        };

        let mut state = GadgetState {
            http: HttpState {
                origins: vec!["*".into()],
                client: Some(Arc::new(client)),
                insecure_client: Arc::new(std::sync::OnceLock::new()),
            },
            ..GadgetState::default_for_test()
        };

        use bindings::torchsnap::gadget::http::Host;
        let resp = state.fetch(request).expect("fetch should succeed");
        assert_eq!(resp.body, b"hello body");
    }

    // =========================================================
    // WASM integration tests for opener/http via fixture plugin
    //
    // These tests run the actual `opener-http-plugin` fixture
    // WASM and exercise `opener::open-url` and `http::fetch`
    // end-to-end through `handle_message` dispatch, with a mock
    // opener closure and a `httpmock` HTTP server respectively.
    // =========================================================

    fn compile_opener_http_fixture() -> (Arc<WasmRuntime>, WasmGadgetInstance) {
        let runtime = test_runtime();
        runtime
            .compile("opener-http-plugin", OPENER_HTTP_PLUGIN_WASM)
            .expect("compile opener-http fixture");
        let instance = runtime
            .instantiate("opener-http-plugin")
            .expect("instantiate opener-http fixture");
        (runtime, instance)
    }

    #[test]
    fn opener_permitted_scheme_calls_writer() {
        let (_runtime, instance) = compile_opener_http_fixture();

        let called_url: std::sync::Arc<Mutex<Option<String>>> =
            std::sync::Arc::new(Mutex::new(None));
        let called_url_clone = called_url.clone();

        instance.set_opener_schemes(vec!["https".into()]);
        instance.set_opener_writer(Box::new(move |url: &str| {
            *called_url_clone.lock().expect("not poisoned") = Some(url.to_string());
            Ok(())
        }));

        instance.enable().expect("enable");

        let result = instance
            .handle_message("opener.open-url", "https://example.com")
            .expect("handle_message call succeeded")
            .expect("guest returned Ok");

        assert_eq!(result, "ok");
        assert_eq!(
            called_url.lock().expect("not poisoned").as_deref(),
            Some("https://example.com")
        );
    }

    #[test]
    fn opener_forbidden_scheme_returns_error() {
        let (_runtime, instance) = compile_opener_http_fixture();

        instance.set_opener_schemes(vec!["https".into()]);
        instance.set_opener_writer(Box::new(|_: &str| Ok(())));

        instance.enable().expect("enable");

        let result = instance
            .handle_message("opener.open-url", "ftp://example.com")
            .expect("handle_message call succeeded");

        let err = result.expect_err("guest should return Err for ftp://");
        assert!(
            err.contains("scheme not permitted: ftp"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn opener_writer_not_set_returns_error() {
        let (_runtime, instance) = compile_opener_http_fixture();

        instance.set_opener_schemes(vec!["https".into()]);
        // Intentionally omit set_opener_writer.

        instance.enable().expect("enable");

        let result = instance
            .handle_message("opener.open-url", "https://example.com")
            .expect("handle_message call succeeded");

        let err = result.expect_err("guest should return Err when writer missing");
        assert!(
            err.contains("opener not initialized"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn http_permitted_origin_proceeds() {
        use httpmock::MockServer;
        use httpmock::prelude::*;

        let server = MockServer::start();
        server.mock(|when, then| {
            when.method(GET).path("/ping");
            then.status(200).body("pong");
        });

        let (_runtime, instance) = compile_opener_http_fixture();
        instance.set_http_origins(vec!["*".into()]);
        instance.set_http_client(Arc::new(crate::network::Http::new()));

        instance.enable().expect("enable");

        let result = instance
            .handle_message("http.get", &server.url("/ping"))
            .expect("handle_message call succeeded")
            .expect("guest returned Ok");

        assert_eq!(result, "status:200");
    }

    // =========================================================
    // WASM integration tests for the website-metadata host import
    //
    // The committed `website-metadata-plugin` fixture exposes
    // `lookup-blocking` and `lookup-cached` messaging methods.
    // Tests below stand up a real `WebsiteMetadataService`
    // pointed at an httpmock server, install it on the plugin
    // instance via `set_website_metadata`, and verify the WIT
    // boundary round-trip end-to-end.
    // =========================================================

    fn build_test_metadata_service(
        server: &httpmock::MockServer,
    ) -> (
        Arc<crate::network::website_metadata::WebsiteMetadataService>,
        tempfile::TempDir,
        crate::settings::notifier::SettingsNotifier,
    ) {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let notifier = crate::settings::notifier::SettingsNotifier::new();
        let server_base = server.base_url();
        let svc = crate::network::website_metadata::WebsiteMetadataService::new(
            tmp.path().to_path_buf(),
            &notifier,
            30,
        )
        .expect("construct service");
        let svc = Arc::new(
            svc.with_url_builder(move |domain, path| format!("{server_base}/{domain}{path}")),
        );
        (svc, tmp, notifier)
    }

    fn compile_website_metadata_fixture() -> (Arc<WasmRuntime>, WasmGadgetInstance) {
        let runtime = test_runtime();
        runtime
            .compile("website-metadata-plugin", WEBSITE_METADATA_PLUGIN_WASM)
            .expect("compile website-metadata fixture");
        let instance = runtime
            .instantiate("website-metadata-plugin")
            .expect("instantiate website-metadata fixture");
        (runtime, instance)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn website_metadata_blocking_lookup_returns_hit() {
        use httpmock::MockServer;
        use httpmock::prelude::*;

        let server = MockServer::start();
        let domain = "ws.test";
        let _page = server.mock(|when, then| {
            when.method(GET).path(format!("/{domain}/"));
            then.status(200)
                .header("content-type", "text/html")
                .body(format!(
                    r#"<!doctype html><html><head>
                        <title>WS Title</title>
                        <meta name="description" content="WS Desc" />
                        <link rel="icon" type="image/png" href="icon.png" />
                    </head></html>"#
                ));
        });
        let _icon = server.mock(|when, then| {
            when.method(GET).path(format!("/{domain}/icon.png"));
            then.status(200).header("content-type", "image/png").body({
                use image::{ImageBuffer, ImageFormat, Rgba};
                let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
                    ImageBuffer::from_pixel(16, 16, Rgba([10, 20, 30, 255]));
                let mut buf: Vec<u8> = Vec::new();
                image::DynamicImage::ImageRgba8(img)
                    .write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)
                    .expect("encode test PNG");
                buf
            });
        });

        let (svc, _tmp, _notifier) = build_test_metadata_service(&server);
        let (_runtime, instance) = compile_website_metadata_fixture();
        instance.set_website_metadata(true, Some(svc));
        instance.enable().expect("enable");

        let result = instance
            .handle_message("lookup-blocking", domain)
            .expect("handle_message call succeeded")
            .expect("guest returned Ok");

        assert_eq!(result, "hit:WS Title:WS Desc:asset-icon");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn website_metadata_cached_lookup_returns_pending_on_cold_miss() {
        use httpmock::MockServer;
        use httpmock::prelude::*;

        let server = MockServer::start();
        let domain = "wait.test";
        let _page = server.mock(|when, then| {
            when.method(GET).path(format!("/{domain}/"));
            then.status(200)
                .header("content-type", "text/html")
                .body("<html><head><title>x</title></head></html>");
        });

        let (svc, _tmp, _notifier) = build_test_metadata_service(&server);
        let (_runtime, instance) = compile_website_metadata_fixture();
        instance.set_website_metadata(true, Some(svc));
        instance.enable().expect("enable");

        let result = instance
            .handle_message("lookup-cached", domain)
            .expect("handle_message call succeeded")
            .expect("guest returned Ok");

        assert_eq!(result, "pending");
    }

    #[test]
    fn website_metadata_lookup_without_permission_returns_permission_denied() {
        let (_runtime, instance) = compile_website_metadata_fixture();
        // Intentionally do NOT call set_website_metadata: state defaults
        // to disabled with no service installed.
        instance.enable().expect("enable");

        let result = instance
            .handle_message("lookup-blocking", "any.test")
            .expect("handle_message call succeeded");

        let err = result.expect_err("guest should see PermissionDenied");
        assert!(err.contains("PermissionDenied"), "unexpected error: {err}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn website_metadata_invalid_domain_returns_invalid_domain_error() {
        let server = httpmock::MockServer::start();
        let (svc, _tmp, _notifier) = build_test_metadata_service(&server);
        let (_runtime, instance) = compile_website_metadata_fixture();
        instance.set_website_metadata(true, Some(svc));
        instance.enable().expect("enable");

        let result = instance
            .handle_message("lookup-blocking", "https://example.com")
            .expect("handle_message call succeeded");

        let err = result.expect_err("guest should see InvalidDomain");
        assert!(err.contains("InvalidDomain"), "unexpected error: {err}");
    }

    #[test]
    fn http_blocked_origin_returns_permission_denied() {
        let (_runtime, instance) = compile_opener_http_fixture();
        // Empty origins = deny all.
        instance.set_http_origins(vec![]);
        instance.set_http_client(Arc::new(crate::network::Http::new()));

        instance.enable().expect("enable");

        let result = instance
            .handle_message("http.get", "https://example.com/test")
            .expect("handle_message call succeeded");

        let err = result.expect_err("guest should return Err for blocked origin");
        assert!(err.contains("PermissionDenied"), "unexpected error: {err}");
    }

    // =========================================================
    // Assets host import — unit tests
    //
    // These drive the `assets::Host` impl directly against a
    // `GadgetState` built with `default_for_test` and a
    // `DirectorySource` stashed on `plugin_source`. They cover
    // each variant of `AssetsError` plus the happy paths for
    // both `read` and `exists`. The fixture-driven integration
    // tests (step 1f) exercise the same paths through a real
    // WASM guest call.
    // =========================================================

    fn make_plugin_source_dir() -> (
        tempfile::TempDir,
        Arc<dyn super::super::source::GadgetSource + Send + Sync>,
    ) {
        use super::super::source::DirectorySource;

        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();

        std::fs::write(
            root.join("manifest.toml"),
            r#"
[plugin]
id = "assets-unit-plugin"
name = "Assets Unit Plugin"
description = "Fixture for runtime unit tests"
version = "0.0.0"
wasm = "plugin.wasm"
icon = "heroicons:beaker"
"#,
        )
        .expect("write manifest");
        std::fs::write(root.join("plugin.wasm"), b"wasm").expect("write wasm");
        std::fs::write(root.join("greeting.txt"), b"hello from the fixture\n")
            .expect("write greeting");
        std::fs::create_dir_all(root.join("data")).expect("mkdir data");
        std::fs::write(root.join("data/payload.bin"), [0u8, 1, 2, 3, 255]).expect("write payload");

        let src: Arc<dyn super::super::source::GadgetSource + Send + Sync> =
            Arc::new(DirectorySource::open(root).expect("open"));
        (dir, src)
    }

    fn state_with_source(
        src: Arc<dyn super::super::source::GadgetSource + Send + Sync>,
    ) -> GadgetState {
        GadgetState {
            plugin_source: Some(src),
            ..GadgetState::default_for_test()
        }
    }

    #[test]
    fn assets_read_returns_bytes_for_existing_file() {
        use bindings::torchsnap::gadget::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let bytes = state
            .read("greeting.txt".to_string())
            .expect("read succeeds");
        assert_eq!(bytes, b"hello from the fixture\n");
    }

    #[test]
    fn assets_read_preserves_binary_content() {
        use bindings::torchsnap::gadget::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let bytes = state
            .read("data/payload.bin".to_string())
            .expect("read succeeds");
        assert_eq!(bytes, [0u8, 1, 2, 3, 255]);
    }

    #[test]
    fn assets_read_returns_not_found_for_missing_file() {
        use bindings::torchsnap::gadget::assets::AssetsError;
        use bindings::torchsnap::gadget::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let err = state
            .read("not-here.json".to_string())
            .expect_err("missing file returns Err");
        assert!(
            matches!(err, AssetsError::NotFound),
            "expected NotFound, got {err:?}"
        );
    }

    #[test]
    fn assets_read_rejects_traversal_path() {
        use bindings::torchsnap::gadget::assets::AssetsError;
        use bindings::torchsnap::gadget::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let err = state
            .read("../../../etc/passwd".to_string())
            .expect_err("traversal rejected");
        match err {
            AssetsError::InvalidPath(msg) => assert!(
                msg.contains("escapes"),
                "InvalidPath message should mention escape (got: {msg})"
            ),
            other => panic!("expected InvalidPath, got {other:?}"),
        }
    }

    #[test]
    fn assets_read_rejects_absolute_path() {
        use bindings::torchsnap::gadget::assets::AssetsError;
        use bindings::torchsnap::gadget::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let err = state
            .read("/etc/passwd".to_string())
            .expect_err("absolute rejected");
        match err {
            AssetsError::InvalidPath(msg) => assert!(
                msg.contains("relative"),
                "InvalidPath message should mention relative (got: {msg})"
            ),
            other => panic!("expected InvalidPath, got {other:?}"),
        }
    }

    #[test]
    fn assets_read_rejects_empty_path() {
        use bindings::torchsnap::gadget::assets::AssetsError;
        use bindings::torchsnap::gadget::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let err = state.read("".to_string()).expect_err("empty rejected");
        assert!(matches!(err, AssetsError::InvalidPath(_)));
    }

    #[test]
    fn assets_read_rejects_nul_byte_path() {
        use bindings::torchsnap::gadget::assets::AssetsError;
        use bindings::torchsnap::gadget::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let err = state
            .read("data\0hidden".to_string())
            .expect_err("NUL rejected");
        assert!(matches!(err, AssetsError::InvalidPath(_)));
    }

    #[test]
    fn assets_read_without_plugin_source_returns_io_error() {
        use bindings::torchsnap::gadget::assets::AssetsError;
        use bindings::torchsnap::gadget::assets::Host;

        // Default state has `plugin_source: None` — mirrors
        // calling an asset import outside an enable lifetime.
        let mut state = GadgetState::default_for_test();
        let err = state
            .read("greeting.txt".to_string())
            .expect_err("uninitialized returns Err");
        match err {
            AssetsError::IoError(msg) => assert!(
                msg.contains("not initialized"),
                "expected `not initialized` message, got: {msg}"
            ),
            other => panic!("expected IoError, got {other:?}"),
        }
    }

    #[test]
    fn assets_exists_returns_true_when_present() {
        use bindings::torchsnap::gadget::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        assert!(
            state
                .exists("greeting.txt".to_string())
                .expect("probe succeeds")
        );
    }

    #[test]
    fn assets_exists_returns_false_when_absent() {
        use bindings::torchsnap::gadget::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        assert!(
            !state
                .exists("not-here.json".to_string())
                .expect("probe succeeds")
        );
    }

    #[test]
    fn assets_exists_rejects_traversal_path() {
        use bindings::torchsnap::gadget::assets::AssetsError;
        use bindings::torchsnap::gadget::assets::Host;

        let (_dir, src) = make_plugin_source_dir();
        let mut state = state_with_source(src);

        let err = state
            .exists("../../../etc/passwd".to_string())
            .expect_err("traversal rejected");
        assert!(matches!(err, AssetsError::InvalidPath(_)));
    }

    #[test]
    fn assets_exists_without_plugin_source_returns_io_error() {
        use bindings::torchsnap::gadget::assets::AssetsError;
        use bindings::torchsnap::gadget::assets::Host;

        let mut state = GadgetState::default_for_test();
        let err = state
            .exists("greeting.txt".to_string())
            .expect_err("uninitialized returns Err");
        assert!(matches!(err, AssetsError::IoError(_)));
    }

    #[test]
    fn assets_into_io_error_includes_full_chain() {
        // `anyhow::Error::chain` — make sure the `{e:#}`
        // formatting captures the `.context()` prefix so
        // host logs stay diagnostic.
        let err = anyhow::anyhow!("root cause").context("while doing X");
        let mapped = host::assets::into_assets_io_error(err);
        match mapped {
            bindings::torchsnap::gadget::assets::AssetsError::IoError(msg) => {
                assert!(msg.contains("while doing X"), "missing context: {msg}");
                assert!(msg.contains("root cause"), "missing root: {msg}");
            }
            other => panic!("expected IoError, got {other:?}"),
        }
    }

    // =========================================================
    // WASM integration tests for assets via fixture plugin
    //
    // The assets fixture directory contains `greeting.txt` at
    // the root and `data/payload.bin` nested. These tests
    // compile the fixture, wire it to a `DirectorySource`
    // pointing at the fixture directory, and exercise
    // `assets::read` / `assets::exists` end-to-end through
    // `handle_message` dispatch.
    // =========================================================

    fn compile_assets_fixture() -> (Arc<WasmRuntime>, WasmGadgetInstance) {
        let runtime = test_runtime();
        runtime
            .compile("assets-plugin", ASSETS_PLUGIN_WASM)
            .expect("compile assets fixture");
        let instance = runtime
            .instantiate("assets-plugin")
            .expect("instantiate assets fixture");
        (runtime, instance)
    }

    /// Build a `GadgetSource` pointing at the committed
    /// assets-plugin fixture directory. This mirrors what
    /// the bridge does in production: `DirectorySource::open`
    /// on the plugin root, wrapped in an `Arc`, then stashed
    /// on the instance via `set_plugin_source`.
    fn assets_fixture_source() -> Arc<dyn super::super::source::GadgetSource + Send + Sync> {
        use super::super::source::DirectorySource;
        let fixture_root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/assets-plugin");
        Arc::new(DirectorySource::open(fixture_root).expect("open assets fixture"))
    }

    #[test]
    fn wasm_assets_read_returns_bundled_file_contents() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.read", "greeting.txt")
            .expect("dispatch succeeded")
            .expect("guest returned Ok");

        assert!(
            result.contains("Hello from the assets fixture"),
            "unexpected body: {result}"
        );
    }

    #[test]
    fn wasm_assets_read_binary_preserves_byte_count() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        // `data/payload.bin` was written with a known
        // length; verify the full byte count crosses the
        // WIT boundary without truncation.
        let result = instance
            .handle_message("assets.read-len", "data/payload.bin")
            .expect("dispatch succeeded")
            .expect("guest returned Ok");

        let expected_len = std::fs::metadata(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/assets-plugin/data/payload.bin"),
        )
        .expect("stat fixture")
        .len();

        assert_eq!(result, format!("len:{expected_len}"));
    }

    #[test]
    fn wasm_assets_exists_true_for_bundled_file() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.exists", "greeting.txt")
            .expect("dispatch succeeded")
            .expect("guest returned Ok");
        assert_eq!(result, "true");
    }

    #[test]
    fn wasm_assets_exists_false_for_missing_file() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.exists", "not-here.txt")
            .expect("dispatch succeeded")
            .expect("guest returned Ok");
        assert_eq!(result, "false");
    }

    #[test]
    fn wasm_assets_exists_false_for_nested_missing_file() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.exists", "data/not-here.txt")
            .expect("dispatch succeeded")
            .expect("guest returned Ok");
        assert_eq!(result, "false");
    }

    #[test]
    fn wasm_assets_read_rejects_traversal() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.read", "../../../etc/passwd")
            .expect("dispatch succeeded");
        let err = result.expect_err("traversal must error");
        assert!(
            err.contains("InvalidPath"),
            "expected InvalidPath variant in guest debug output, got: {err}"
        );
    }

    #[test]
    fn wasm_assets_read_rejects_absolute_path() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.read", "/etc/passwd")
            .expect("dispatch succeeded");
        let err = result.expect_err("absolute must error");
        assert!(
            err.contains("InvalidPath"),
            "expected InvalidPath, got: {err}"
        );
    }

    #[test]
    fn wasm_assets_read_returns_not_found_for_missing_file() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.read", "not-present.json")
            .expect("dispatch succeeded");
        let err = result.expect_err("missing must error");
        assert!(
            err.contains("NotFound"),
            "expected NotFound variant in guest debug output, got: {err}"
        );
    }

    #[test]
    fn wasm_assets_read_succeeds_on_nested_path() {
        let (_runtime, instance) = compile_assets_fixture();
        instance.set_plugin_source(assets_fixture_source());
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.read", "data/payload.bin")
            .expect("dispatch succeeded")
            .expect("guest returned Ok");
        assert!(
            result.contains("nested binary data for tests"),
            "unexpected body: {result}"
        );
    }

    #[test]
    fn wasm_assets_calls_without_plugin_source_return_io_error() {
        // Skip `set_plugin_source` — same shape the bridge
        // would produce if it forgot to stash the source
        // on enable. The host returns `IoError` and the
        // guest bubbles the debug form up to our caller.
        let (_runtime, instance) = compile_assets_fixture();
        instance.enable().expect("enable");

        let result = instance
            .handle_message("assets.read", "greeting.txt")
            .expect("dispatch succeeded");
        let err = result.expect_err("uninit must error");
        assert!(err.contains("IoError"), "expected IoError, got: {err}");
    }

    // =========================================================
    // Command host import — integration tests
    //
    // These drive the full `command::run` impl through the
    // `command-plugin` fixture, which dispatches the test
    // scenarios from its `messaging::handle-message` impl.
    // Unix-only: the fixture's manifest grants `/bin/echo` and
    // `/bin/sh -c <script>` rules, both of which require Unix
    // utilities. A Windows port would need its own fixture
    // with rules over `cmd.exe` / PowerShell.
    // =========================================================

    #[cfg(unix)]
    fn compile_command_fixture() -> (Arc<WasmRuntime>, WasmGadgetInstance, tempfile::TempDir) {
        use super::super::argv_matcher::{CompiledArgvConstraint, CompiledCommandRule};
        use super::super::permission_vars::PathContext;

        let runtime = test_runtime();
        runtime
            .compile("command-plugin", COMMAND_PLUGIN_WASM)
            .expect("compile command fixture");
        let instance = runtime
            .instantiate("command-plugin")
            .expect("instantiate command fixture");

        // Stash a path context so the default cwd resolution
        // (`<plugin-data>/exec-cwd/`) has somewhere real to
        // create. The tempdir lives on so the test's child
        // process actually has a valid cwd at spawn time.
        let scratch = tempfile::TempDir::new().expect("scratch tempdir");
        instance.set_path_context(PathContext {
            plugin_data: scratch.path().to_path_buf(),
            plugin_archive: scratch.path().to_path_buf(),
            home: scratch.path().to_path_buf(),
            xdg_config: scratch.path().to_path_buf(),
            xdg_data: scratch.path().to_path_buf(),
        });

        // Compile rules matching the fixture's manifest:
        //   /bin/echo with [{any-string}]
        //   /bin/sh   with [{literal:"-c"}, {any-string}]
        // The fixture's `handle-message` dispatches into
        // these via the four test methods.
        instance.set_command_rules(vec![
            CompiledCommandRule {
                binary: "/bin/echo".to_string(),
                argv: vec![CompiledArgvConstraint::AnyString],
            },
            CompiledCommandRule {
                binary: "/bin/sh".to_string(),
                argv: vec![
                    CompiledArgvConstraint::Literal("-c".to_string()),
                    CompiledArgvConstraint::AnyString,
                ],
            },
        ]);

        (runtime, instance, scratch)
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn command_allowed_call_returns_stdout() {
        let (_runtime, instance, _scratch) = compile_command_fixture();
        instance.enable().expect("enable");

        let result = instance
            .handle_message("echo", "hello")
            .expect("handle_message call succeeded")
            .expect("guest returned Ok");

        // /bin/echo emits the argument plus a trailing newline.
        assert_eq!(result.trim(), "hello");
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn command_denied_binary_returns_permission_denied() {
        let (_runtime, instance, _scratch) = compile_command_fixture();
        instance.enable().expect("enable");

        let result = instance
            .handle_message("deny:/bin/cat", "")
            .expect("handle_message call succeeded");

        let err = result.expect_err("guest should return Err for unlisted binary");
        assert!(
            err.contains("PermissionDenied"),
            "expected permission-denied, got: {err}"
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn command_timeout_returns_timeout_variant() {
        let (_runtime, instance, _scratch) = compile_command_fixture();
        instance.enable().expect("enable");

        // Fixture sends `/bin/sh -c "sleep 5"` with a 150ms
        // timeout. The host should kill the child via
        // SIGTERM/SIGKILL on the process group and surface
        // `Timeout`. Anything longer than ~1s would mean the
        // grace period failed.
        let started = std::time::Instant::now();
        let result = instance
            .handle_message("sleep:5", "")
            .expect("handle_message call succeeded");
        let elapsed = started.elapsed();

        let err = result.expect_err("guest should return Err on timeout");
        assert!(
            err.contains("Timeout"),
            "expected Timeout variant, got: {err}"
        );
        assert!(
            elapsed < std::time::Duration::from_secs(2),
            "kill grace took too long: {elapsed:?}"
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "multi_thread")]
    async fn command_output_overflow_returns_too_large() {
        let (_runtime, instance, _scratch) = compile_command_fixture();
        instance.enable().expect("enable");

        // Fixture sends `/bin/sh -c "head -c 1024 /dev/zero"`
        // with a 64-byte output cap. The host should kill the
        // child and surface `OutputTooLarge` carrying the
        // bytes captured up to the cap.
        let result = instance
            .handle_message("flood:1024", "")
            .expect("handle_message call succeeded");

        let err = result.expect_err("guest should return Err on output overflow");
        assert!(
            err.contains("OutputTooLarge"),
            "expected OutputTooLarge variant, got: {err}"
        );
    }
}
