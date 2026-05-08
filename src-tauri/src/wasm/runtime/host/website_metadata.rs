// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Website-metadata host import
//
// Thin shim over the host's shared `WebsiteMetadataService`.
// The service handles caching, single-flight coalescing,
// negative-caching, and favicon storage; the shim's job is
// permission gating, type translation across the WIT
// boundary, and wrapping `Http::send`'s `block_on` in
// `block_in_place` so it can be called from a tokio worker
// thread.
// =========================================================

use std::sync::Arc;

use crate::network::website_metadata::{
    LookupError, LookupMode, LookupResult, WebsiteMetadataService,
};
use crate::wasm::bindings;

use super::super::GadgetState;

/// Per-gadget website-metadata state.
#[derive(Default)]
pub(crate) struct WebsiteMetadataState {
    pub(crate) enabled: bool,
    pub(crate) service: Option<Arc<WebsiteMetadataService>>,
}

impl bindings::torchsnap::gadget::website_metadata::Host for GadgetState {
    fn lookup(
        &mut self,
        domain: String,
        mode: bindings::torchsnap::gadget::website_metadata::LookupMode,
    ) -> Result<
        bindings::torchsnap::gadget::website_metadata::LookupResult,
        bindings::torchsnap::gadget::website_metadata::WebsiteMetadataError,
    > {
        use bindings::torchsnap::gadget::website_metadata::WebsiteMetadataError as WitError;

        let caps = self.caps().map_err(|e| WitError::PermissionDenied(e))?;

        if !caps.website_metadata.enabled {
            return Err(WitError::PermissionDenied(
                "gadget manifest does not declare permissions.website-metadata = true".into(),
            ));
        }

        let service = caps
            .website_metadata
            .service
            .as_ref()
            .expect("service handle present when enabled");

        let native_mode = wit_mode_to_native(mode);

        let result = tokio::task::block_in_place(|| service.lookup(&domain, native_mode));

        match result {
            Ok(r) => Ok(native_to_wit(r)),
            Err(LookupError::InvalidDomain(d)) => Err(WitError::InvalidDomain(d)),
        }
    }
}

// =========================================================
// Type conversions
// =========================================================

fn wit_mode_to_native(
    mode: bindings::torchsnap::gadget::website_metadata::LookupMode,
) -> LookupMode {
    use bindings::torchsnap::gadget::website_metadata::LookupMode as WitMode;
    match mode {
        WitMode::Cached => LookupMode::Cached,
        WitMode::Blocking => LookupMode::Blocking,
    }
}

fn native_to_wit(
    result: LookupResult,
) -> bindings::torchsnap::gadget::website_metadata::LookupResult {
    use bindings::torchsnap::gadget::website_metadata::{CacheEntry, LookupResult as WitResult};
    match result {
        LookupResult::Hit(meta) => WitResult::Hit(CacheEntry {
            title: meta.title,
            description: meta.description,
            favicon: meta.favicon.into(),
        }),
        LookupResult::ReachableNoData => WitResult::ReachableNoData,
        LookupResult::Unreachable => WitResult::Unreachable,
        LookupResult::Pending => WitResult::Pending,
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    use httpmock::MockServer;
    use httpmock::prelude::*;
    use tempfile::TempDir;

    use crate::settings::notifier::SettingsNotifier;
    use crate::wasm::bindings::torchsnap::gadget::website_metadata as wit;
    use crate::wasm::runtime::caps::WasmGadgetCaps;

    struct ShimEnv {
        server: MockServer,
        state: GadgetState,
        _tmp: TempDir,
        _notifier: SettingsNotifier,
    }

    fn make_env(enabled: bool) -> ShimEnv {
        let server = MockServer::start();
        let tmp = TempDir::new().expect("temp dir");
        let notifier = SettingsNotifier::new();

        let server_base = server.base_url();
        let svc = WebsiteMetadataService::new(tmp.path().to_path_buf(), &notifier, 30)
            .expect("construct service");
        let svc = Arc::new(
            svc.with_url_builder(move |domain, path| format!("{server_base}/{domain}{path}")),
        );

        let mut caps = WasmGadgetCaps::default_for_test();
        caps.website_metadata = WebsiteMetadataState {
            enabled,
            service: if enabled { Some(svc) } else { None },
        };

        let state = GadgetState {
            caps: Some(caps),
            ..GadgetState::default_for_test()
        };

        ShimEnv {
            server,
            state,
            _tmp: tmp,
            _notifier: notifier,
        }
    }

    fn html_with_icon(title: &str, desc: &str, icon: &str) -> String {
        format!(
            r#"<!doctype html><html><head>
                <title>{title}</title>
                <meta name="description" content="{desc}" />
                <link rel="icon" type="image/png" href="{icon}" />
            </head></html>"#
        )
    }

    fn png_bytes() -> Vec<u8> {
        use image::{ImageBuffer, ImageFormat, Rgba};
        let img: ImageBuffer<Rgba<u8>, Vec<u8>> =
            ImageBuffer::from_pixel(16, 16, Rgba([10, 20, 30, 255]));
        let mut buf: Vec<u8> = Vec::new();
        image::DynamicImage::ImageRgba8(img)
            .write_to(&mut std::io::Cursor::new(&mut buf), ImageFormat::Png)
            .expect("encode test PNG");
        buf
    }

    fn lookup_sync(
        state: &mut GadgetState,
        domain: &str,
        mode: wit::LookupMode,
    ) -> Result<wit::LookupResult, wit::WebsiteMetadataError> {
        tokio::task::block_in_place(|| {
            <GadgetState as wit::Host>::lookup(state, domain.to_string(), mode)
        })
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shim_returns_permission_denied_when_disabled() {
        let mut env = make_env(false);

        let mock = env.server.mock(|when, then| {
            when.method(GET).path("/x.test/");
            then.status(200)
                .header("content-type", "text/html")
                .body("ok");
        });

        let err = lookup_sync(&mut env.state, "x.test", wit::LookupMode::Blocking).unwrap_err();
        match err {
            wit::WebsiteMetadataError::PermissionDenied(_) => {}
            other => panic!("expected PermissionDenied, got {other:?}"),
        }

        mock.assert_calls(0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shim_rejects_invalid_domain() {
        let mut env = make_env(true);
        let err = lookup_sync(
            &mut env.state,
            "https://example.com",
            wit::LookupMode::Blocking,
        )
        .unwrap_err();
        match err {
            wit::WebsiteMetadataError::InvalidDomain(_) => {}
            other => panic!("expected InvalidDomain, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shim_returns_hit_in_blocking_mode() {
        let mut env = make_env(true);
        let domain = "happy.test";

        let _page = env.server.mock(|when, then| {
            when.method(GET).path(format!("/{domain}/"));
            then.status(200)
                .header("content-type", "text/html")
                .body(html_with_icon("My Title", "Some desc", "icon.png"));
        });
        let _icon = env.server.mock(|when, then| {
            when.method(GET).path(format!("/{domain}/icon.png"));
            then.status(200)
                .header("content-type", "image/png")
                .body(png_bytes());
        });

        let result = lookup_sync(&mut env.state, domain, wit::LookupMode::Blocking).expect("ok");
        match result {
            wit::LookupResult::Hit(entry) => {
                assert_eq!(entry.title.as_deref(), Some("My Title"));
                assert_eq!(entry.description.as_deref(), Some("Some desc"));
                match &entry.favicon {
                    wit::EntryIcon::AssetIcon(url) => {
                        assert!(
                            url.starts_with("torchsnap-favicon://localhost/"),
                            "unexpected favicon URL: {url}"
                        );
                    }
                    other => panic!("expected AssetIcon, got {other:?}"),
                }
            }
            other => panic!("expected Hit, got {other:?}"),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shim_returns_pending_in_cached_mode_on_cold_miss() {
        let mut env = make_env(true);
        let domain = "wait.test";

        let _page = env.server.mock(|when, then| {
            when.method(GET).path(format!("/{domain}/"));
            then.status(200)
                .header("content-type", "text/html")
                .body(html_with_icon("T", "D", "/i.png"));
        });
        let _icon = env.server.mock(|when, then| {
            when.method(GET).path(format!("/{domain}/i.png"));
            then.status(200)
                .header("content-type", "image/png")
                .body(png_bytes());
        });

        let first = lookup_sync(&mut env.state, domain, wit::LookupMode::Cached).unwrap();
        assert!(matches!(first, wit::LookupResult::Pending));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shim_returns_reachable_no_data_for_text_response() {
        let mut env = make_env(true);
        let domain = "plain.test";

        let _page = env.server.mock(|when, then| {
            when.method(GET).path(format!("/{domain}/"));
            then.status(200)
                .header("content-type", "text/plain")
                .body("nope");
        });

        let result = lookup_sync(&mut env.state, domain, wit::LookupMode::Blocking).unwrap();
        assert!(matches!(result, wit::LookupResult::ReachableNoData));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn shim_maps_native_hero_icon_to_wit_variant() {
        let mut env = make_env(true);
        let domain = "iconless.test";

        let _page = env.server.mock(|when, then| {
            when.method(GET).path(format!("/{domain}/"));
            then.status(200)
                .header("content-type", "text/html")
                .body("<html><head><title>OnlyTitle</title></head></html>");
        });

        let result = lookup_sync(&mut env.state, domain, wit::LookupMode::Blocking).unwrap();
        match result {
            wit::LookupResult::Hit(entry) => {
                assert_eq!(entry.title.as_deref(), Some("OnlyTitle"));
                match entry.favicon {
                    wit::EntryIcon::HeroIcon(name) => assert_eq!(name, "globe-alt"),
                    other => panic!("expected HeroIcon('globe-alt'), got {other:?}"),
                }
            }
            other => panic!("expected Hit, got {other:?}"),
        }
    }
}
