// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Website-metadata host import
//
// Thin shim over the host's shared `WebsiteMetadataCap`.
// The cap wraps the shared service that handles caching,
// single-flight coalescing, negative-caching, and favicon
// storage. The shim's job is permission gating (via the
// Option on `WasmGadgetCaps.website_metadata`) and type
// translation across the WIT boundary.
// =========================================================

use crate::caps::WebsiteMetadataCapError;
use crate::network::website_metadata::{LookupMode, LookupResult};
use crate::wasm::bindings;

use super::super::GadgetState;

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

        let cap = caps
            .website_metadata
            .as_ref()
            .ok_or_else(|| WitError::PermissionDenied(
                "gadget manifest does not declare permissions.website-metadata = true".into(),
            ))?;

        cap.lookup(&domain, LookupMode::from(mode))
            .map(|r| bindings::torchsnap::gadget::website_metadata::LookupResult::from(r))
            .map_err(|e| WitError::from(e))
    }
}

// =========================================================
// Type conversions
// =========================================================

impl From<bindings::torchsnap::gadget::website_metadata::LookupMode> for LookupMode {
    fn from(mode: bindings::torchsnap::gadget::website_metadata::LookupMode) -> Self {
        use bindings::torchsnap::gadget::website_metadata::LookupMode as WitMode;
        match mode {
            WitMode::Cached => LookupMode::Cached,
            WitMode::Blocking => LookupMode::Blocking,
        }
    }
}

impl From<LookupResult> for bindings::torchsnap::gadget::website_metadata::LookupResult {
    fn from(result: LookupResult) -> Self {
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
}

impl From<WebsiteMetadataCapError>
    for bindings::torchsnap::gadget::website_metadata::WebsiteMetadataError
{
    fn from(err: WebsiteMetadataCapError) -> Self {
        match err {
            WebsiteMetadataCapError::InvalidDomain(d) => Self::InvalidDomain(d),
        }
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::Arc;

    use httpmock::MockServer;
    use httpmock::prelude::*;
    use tempfile::TempDir;

    use crate::caps::WebsiteMetadataCap;
    use crate::network::website_metadata::WebsiteMetadataService;
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
        caps.website_metadata = if enabled {
            Some(Arc::new(WebsiteMetadataCap::new(svc)))
        } else {
            None
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
