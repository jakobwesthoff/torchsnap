// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// HTTP host import
//
// A minimal synchronous HTTP client for WASM plugins. The
// host function body must NOT call `Http::send()` directly
// from within a tokio async task — that would cause
// `Handle::block_on()` inside `Http::send()` to panic.
//
// WASM guest calls are dispatched inline from tokio tasks
// (the bridge task scheduler calls `instance.run_task()`
// without `spawn_blocking`), so we use
// `tokio::task::block_in_place` here. This moves the
// current thread out of the async pool temporarily,
// making the inner `block_on()` safe while letting tokio
// schedule other tasks on remaining threads.
//
// Both `check_http_origin` and `wit_method_to_reqwest` are
// pure free functions for the same testability reason as
// `check_opener_scheme`.
// =========================================================

use std::sync::Arc;
use std::time::Duration;

use crate::network::Http;
use crate::wasm::bindings;

use super::super::{PluginState, WasmPluginInstance};

/// HTTP state. Origin allowlist + the per-plugin client.
#[derive(Default)]
pub(crate) struct HttpState {
    /// Origins this plugin is permitted to fetch. Already
    /// normalized to `ascii_serialization()` at manifest
    /// parse. Empty means deny-all; `["*"]` means trust-all.
    pub(crate) origins: Vec<String>,
    /// HTTP client for this plugin, created on `enable()`
    /// and cleared on `disable()`.
    pub(crate) client: Option<Arc<Http>>,
}

/// Internal error type that classifies transport-level
/// failures before mapping to the WIT `http-error` variant.
#[derive(Debug, thiserror::Error)]
pub(crate) enum WasmHttpError {
    #[error("origin not permitted: {0}")]
    PermissionDenied(String),
    #[error("request timed out")]
    Timeout,
    #[error("network error: {0}")]
    Network(String),
}

impl WasmHttpError {
    /// Classify an `anyhow::Error` from `Http::send()` or
    /// `HttpResponse::bytes()` into the three WIT variants.
    ///
    /// `Http` adds context wrappers so `downcast_ref` on the
    /// outer `anyhow::Error` won't find `reqwest::Error`
    /// directly. We try both the direct downcast (for the
    /// unwrapped case) and scan the formatted chain for
    /// timeout indicators (for context-wrapped errors).
    fn from_request_error(e: anyhow::Error) -> Self {
        if e.downcast_ref::<reqwest::Error>()
            .map(|re| re.is_timeout())
            .unwrap_or(false)
        {
            return Self::Timeout;
        }
        let is_timeout = e.chain().any(|cause| {
            let s = cause.to_string();
            s.contains("timed out") || s.contains("deadline has elapsed")
        });
        if is_timeout {
            Self::Timeout
        } else {
            Self::Network(e.to_string())
        }
    }
}

impl From<WasmHttpError> for bindings::torchsnap::plugin::http::HttpError {
    fn from(e: WasmHttpError) -> Self {
        use bindings::torchsnap::plugin::http::HttpError;
        match e {
            WasmHttpError::PermissionDenied(msg) => HttpError::PermissionDenied(msg),
            WasmHttpError::Timeout => HttpError::Timeout,
            WasmHttpError::Network(msg) => HttpError::Network(msg),
        }
    }
}

/// Verify that `url`'s origin is in the `allowed` list.
///
/// `"*"` short-circuits before URL parsing (trust-all). All
/// other entries are compared against the request URL's
/// ASCII-serialized origin, which normalizes scheme,
/// host, and port identically to how the manifest validation
/// stored them.
pub(crate) fn check_http_origin(allowed: &[String], url: &str) -> Result<(), WasmHttpError> {
    if allowed.iter().any(|o| o == "*") {
        return Ok(());
    }
    let parsed =
        url::Url::parse(url).map_err(|e| WasmHttpError::Network(format!("invalid URL: {e}")))?;
    let origin = parsed.origin().ascii_serialization();
    if allowed.iter().any(|o| o == &origin) {
        Ok(())
    } else {
        Err(WasmHttpError::PermissionDenied(origin))
    }
}

/// Map a WIT `http-method` variant to a `reqwest::Method`.
///
/// `other(string)` is validated via `Method::from_bytes` —
/// an invalid method string maps to `WasmHttpError::Network`
/// rather than being sent over the wire.
pub(crate) fn wit_method_to_reqwest(
    method: bindings::torchsnap::plugin::http::HttpMethod,
) -> Result<reqwest::Method, WasmHttpError> {
    use bindings::torchsnap::plugin::http::HttpMethod;
    match method {
        HttpMethod::Get => Ok(reqwest::Method::GET),
        HttpMethod::Post => Ok(reqwest::Method::POST),
        HttpMethod::Put => Ok(reqwest::Method::PUT),
        HttpMethod::Patch => Ok(reqwest::Method::PATCH),
        HttpMethod::Delete => Ok(reqwest::Method::DELETE),
        HttpMethod::Head => Ok(reqwest::Method::HEAD),
        HttpMethod::Other(s) => reqwest::Method::from_bytes(s.as_bytes())
            .map_err(|e| WasmHttpError::Network(format!("invalid HTTP method: {e}"))),
    }
}

impl bindings::torchsnap::plugin::http::Host for PluginState {
    fn fetch(
        &mut self,
        request: bindings::torchsnap::plugin::http::HttpRequest,
    ) -> Result<
        bindings::torchsnap::plugin::http::HttpResponse,
        bindings::torchsnap::plugin::http::HttpError,
    > {
        use bindings::torchsnap::plugin::http::HttpError as WitHttpError;
        use bindings::torchsnap::plugin::http::HttpResponse as WitHttpResponse;

        check_http_origin(&self.http.origins, &request.url).map_err(WitHttpError::from)?;

        let method = wit_method_to_reqwest(request.method).map_err(WitHttpError::from)?;

        let client = self.http.client.as_ref().ok_or_else(|| {
            WitHttpError::from(WasmHttpError::Network("http client not initialized".into()))
        })?;

        let mut builder = client.request(method, &request.url);
        for (k, v) in request.headers {
            builder = builder.header(&k, &v);
        }
        if let Some(body) = request.body {
            builder = builder.body(body);
        }
        if let Some(ms) = request.timeout_ms {
            builder = builder.timeout(Duration::from_millis(ms as u64));
        }
        if let Some(max) = request.max_body_size {
            builder = builder.max_size(max);
        }

        // `Http::send()` uses `Handle::block_on()` internally, which panics
        // if called from within a tokio async task. WASM guest calls are
        // dispatched inline from async tasks (see bridge.rs task scheduler),
        // so we use `block_in_place` to move this thread out of the async
        // pool before blocking.
        let (status, headers, body) =
            tokio::task::block_in_place(|| -> Result<_, WasmHttpError> {
                let mut response = builder.send().map_err(WasmHttpError::from_request_error)?;
                let status = response.status();
                let headers = response.headers().to_vec();
                let body = response
                    .bytes()
                    .map_err(WasmHttpError::from_request_error)?;
                Ok((status, headers, body))
            })
            .map_err(WitHttpError::from)?;

        Ok(WitHttpResponse {
            status,
            headers,
            body,
        })
    }
}

impl WasmPluginInstance {
    /// Stash the origin allowlist for `http::fetch`. Called
    /// by the bridge at `enable()` from the manifest's
    /// `[permissions.http].origins` list (already normalized).
    pub fn set_http_origins(&self, origins: Vec<String>) {
        self.with_state_mut(|state| state.http.origins = origins);
    }

    /// Install the HTTP client for this plugin's `enable()`
    /// lifetime. Called by the bridge.
    pub fn set_http_client(&self, client: Arc<Http>) {
        self.with_state_mut(|state| state.http.client = Some(client));
    }

    /// Drop the HTTP client on `disable()`.
    pub fn clear_http_client(&self) {
        self.with_state_mut(|state| state.http.client = None);
    }
}
