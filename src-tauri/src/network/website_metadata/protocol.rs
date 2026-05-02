// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Host favicon URI scheme
//
// Registers a `torchsnap-favicon://` custom URI scheme that
// serves cached favicons from the host-owned `FaviconStore`.
// Mirrors `wasm/protocol.rs` (the plugin asset scheme) — the
// host owns two parallel asset roots, one for plugin-bundled
// files and one for cached favicons, so plugin- and host-
// issued icons can both ride a typed URL through the
// renderer without ever needing absolute filesystem paths.
//
// URI format:
//   torchsnap-favicon://localhost/<key>.<ext>
//
// `<key>` is the blake3-hex `StorageKey` produced by
// `FaviconStore::store`, and `<ext>` is the file extension
// stored alongside it (`webp`, `svg`, `png`, `ico`).
//
// On macOS/Linux the scheme is `torchsnap-favicon://`.
// On Windows Tauri maps it to `http://torchsnap-favicon.localhost/`.
// We parse from the `http::Request` path, so both shapes work.
// =========================================================

use std::sync::{Arc, OnceLock};

use tauri::http;

use super::favicon_store::FaviconStore;

/// URI scheme prefix used by host-issued favicons. Single
/// source of truth — the website-metadata service formats
/// output with this prefix and the WASM bridge resolver
/// recognizes it on the response path.
pub const HOST_FAVICON_SCHEME: &str = "torchsnap-favicon://";

// =========================================================
// Favicon Store Registry
// =========================================================

/// Lazily-populated handle to the `FaviconStore`.
///
/// Custom URI schemes must be registered on the Tauri
/// `Builder` before `setup()` runs, but
/// `WebsiteMetadataService` (which owns the `FaviconStore`)
/// is constructed inside `setup()`. The `OnceLock`
/// indirection lets the protocol handler hold onto a slot
/// at builder time and read the store handle lazily on the
/// first request, after `setup()` has populated it.
pub type FaviconRegistry = Arc<OnceLock<Arc<FaviconStore>>>;

/// Create an empty registry. Callers register the protocol
/// handler with this handle, then `set` the store inside
/// `setup()` once `WebsiteMetadataService` is constructed.
pub fn new_registry() -> FaviconRegistry {
    Arc::new(OnceLock::new())
}

// =========================================================
// URL formatting
// =========================================================

/// Format a favicon URL for the host-owned scheme. Used by
/// `WebsiteMetadataService` to populate
/// `EntryIcon::AssetIcon(...)` with a renderable reference
/// instead of an absolute filesystem path.
pub fn host_favicon_url(key: &str, ext: &str) -> String {
    format!("{HOST_FAVICON_SCHEME}localhost/{key}.{ext}")
}

// =========================================================
// Protocol Registration
// =========================================================

/// Register the `torchsnap-favicon` URI scheme on the Tauri
/// builder. Must be called before `.build()` / `.setup()`
/// because custom schemes are bound to the webview at
/// creation time.
pub fn register_favicon_protocol<R: tauri::Runtime>(
    builder: tauri::Builder<R>,
    registry: FaviconRegistry,
) -> tauri::Builder<R> {
    builder.register_asynchronous_uri_scheme_protocol(
        "torchsnap-favicon",
        move |_ctx, request, responder| {
            let registry = Arc::clone(&registry);

            // Spawn a thread because `FaviconStore::resolve`
            // and `std::fs::read` block on the filesystem.
            std::thread::spawn(move || {
                let response = handle_request(&registry, &request);
                responder.respond(response);
            });
        },
    )
}

// =========================================================
// Request Handler
// =========================================================

fn handle_request(
    registry: &FaviconRegistry,
    request: &http::Request<Vec<u8>>,
) -> http::Response<Vec<u8>> {
    // Echo the request's `Origin` for CORS — same pattern as
    // `wasm/protocol.rs` so the scheme works in both dev
    // (http://localhost:1420) and prod (tauri://localhost,
    // https://tauri.localhost).
    let origin = request
        .headers()
        .get("Origin")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("*");

    match serve_favicon(registry, request) {
        Ok((body, content_type)) => http::Response::builder()
            .status(200)
            .header("Content-Type", content_type)
            .header("Access-Control-Allow-Origin", origin)
            .body(body)
            .expect("valid response"),
        Err(ErrorResponse { status, message }) => http::Response::builder()
            .status(status)
            .header("Content-Type", "text/plain; charset=utf-8")
            .header("Access-Control-Allow-Origin", origin)
            .body(message.into_bytes())
            .expect("valid error response"),
    }
}

struct ErrorResponse {
    status: u16,
    message: String,
}

fn serve_favicon(
    registry: &FaviconRegistry,
    request: &http::Request<Vec<u8>>,
) -> Result<(Vec<u8>, String), ErrorResponse> {
    let store = registry.get().ok_or_else(|| ErrorResponse {
        // 503 rather than 500 — the service genuinely has not
        // finished initializing yet (setup() hasn't run). A
        // launcher that retries the request once the cache
        // entry materializes will succeed.
        status: 503,
        message: "favicon store not yet initialized".to_string(),
    })?;

    // Path shape: `/<key>.<ext>`. Strip the leading `/` and
    // split on the last `.` to handle keys that don't contain
    // dots (the blake3-hex output is alphanumeric — keys
    // never contain dots).
    let path = request.uri().path();
    let trimmed = path.trim_start_matches('/');
    let (key, ext) = trimmed.rsplit_once('.').ok_or_else(|| ErrorResponse {
        status: 400,
        message: format!("invalid path: expected /<key>.<ext>, got {path}"),
    })?;

    if key.is_empty() {
        return Err(ErrorResponse {
            status: 400,
            message: "missing key in path".to_string(),
        });
    }
    if ext.is_empty() {
        return Err(ErrorResponse {
            status: 400,
            message: "missing extension in path".to_string(),
        });
    }

    let resolved = store.resolve(key, ext).ok_or_else(|| ErrorResponse {
        status: 404,
        message: format!("favicon not found for key {key}.{ext}"),
    })?;

    let data = std::fs::read(&resolved).map_err(|e| ErrorResponse {
        status: 500,
        message: format!("read favicon file: {e}"),
    })?;

    Ok((data, content_type_for_ext(ext).to_string()))
}

/// Content-Type for a stored favicon extension. The set of
/// possible extensions is bounded by `FaviconStore`: rasters
/// always end up as `webp`; non-decodable formats keep their
/// content-type-derived extension (`svg`, `png`, `ico`,
/// `jpg`/`jpeg`, `gif`, `bmp`, `tiff`, `avif`, `heic`,
/// `heif`). Anything outside that closed set falls back to
/// the generic binary type and the browser still renders
/// images by sniffing.
fn content_type_for_ext(ext: &str) -> &'static str {
    match ext.to_ascii_lowercase().as_str() {
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "ico" => "image/vnd.microsoft.icon",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "tiff" | "tif" => "image/tiff",
        "avif" => "image/avif",
        "heic" | "heif" => "image/heif",
        _ => "application/octet-stream",
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_favicon_url_formats_scheme_authority_and_path() {
        let url = host_favicon_url("abc123", "webp");
        assert_eq!(url, "torchsnap-favicon://localhost/abc123.webp");
    }

    #[test]
    fn host_favicon_url_uses_constant_prefix() {
        let url = host_favicon_url("k", "ext");
        assert!(url.starts_with(HOST_FAVICON_SCHEME));
    }

    #[test]
    fn content_type_maps_known_extensions() {
        assert_eq!(content_type_for_ext("webp"), "image/webp");
        assert_eq!(content_type_for_ext("svg"), "image/svg+xml");
        assert_eq!(content_type_for_ext("png"), "image/png");
        assert_eq!(content_type_for_ext("ico"), "image/vnd.microsoft.icon");
        assert_eq!(content_type_for_ext("WEBP"), "image/webp");
    }

    #[test]
    fn content_type_falls_back_for_unknown_extension() {
        assert_eq!(content_type_for_ext("bogus"), "application/octet-stream");
    }
}
