// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// HTTP host import
//
// A minimal synchronous HTTP client for WASM gadgets. The
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

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::network::Http;
use crate::wasm::bindings;

use super::super::{GadgetState, WasmGadgetInstance};

/// HTTP state. Origin allowlist + per-gadget clients.
#[derive(Default)]
pub(crate) struct HttpState {
    /// Origins this gadget is permitted to fetch. Already
    /// normalized to `ascii_serialization()` at manifest
    /// parse. Empty means deny-all; `["*"]` means trust-all.
    pub(crate) origins: Vec<String>,
    /// HTTP client for this gadget, created on `enable()`
    /// and cleared on `disable()`. Used for normal requests
    /// (TLS verification on).
    pub(crate) client: Option<Arc<Http>>,
    /// Parallel client with TLS certificate verification
    /// disabled. Built lazily on the first request that sets
    /// `insecure-tls: true`, so gadgets that never use the
    /// flag don't pay the construction cost. Cleared on
    /// `disable()`.
    pub(crate) insecure_client: Arc<OnceLock<Arc<Http>>>,
}

/// Internal error type that classifies transport-level
/// failures before mapping to the WIT `http-error` variant.
/// One arm per WIT variant — the conversion is a 1:1 match
/// at the bottom of this module.
#[derive(Debug, thiserror::Error)]
pub(crate) enum WasmHttpError {
    #[error("origin not permitted: {0}")]
    PermissionDenied(String),
    #[error("connection refused: {0}")]
    ConnectionRefused(String),
    #[error("request timed out")]
    Timeout,
    #[error("dns lookup failed: {0}")]
    DnsFailed(String),
    #[error("tls handshake failed: {0}")]
    TlsFailed(String),
    #[error("invalid url: {0}")]
    InvalidUrl(String),
    #[error("transport error: {0}")]
    Other(String),
}

impl WasmHttpError {
    /// Classify an `anyhow::Error` from `Http::send()` or
    /// `HttpResponse::bytes()` into the matching WIT variant.
    ///
    /// `reqwest::Error` exposes `is_timeout()` directly but
    /// does not expose typed accessors for connection-refused,
    /// DNS, or TLS failures — those live in the source chain
    /// as opaque error types from hyper-util / rustls /
    /// std::io. Inspecting the chain by stringification is
    /// the only practical option.
    fn from_request_error(e: anyhow::Error) -> Self {
        // Direct reqwest classification when available.
        if let Some(re) = e.downcast_ref::<reqwest::Error>()
            && re.is_timeout()
        {
            return Self::Timeout;
        }

        // `Http` wraps reqwest errors in `anyhow::Context`,
        // hiding them from `downcast_ref` on the outer error.
        // The chain still carries the original message.
        let chain: Vec<String> = e.chain().map(|c| c.to_string()).collect();
        let chain_lower: Vec<String> = chain.iter().map(|s| s.to_lowercase()).collect();
        let any_contains = |needle: &str| chain_lower.iter().any(|s| s.contains(needle));

        if any_contains("timed out") || any_contains("deadline has elapsed") {
            return Self::Timeout;
        }
        if any_contains("connection refused")
            || any_contains("econnrefused")
            || any_contains("connection reset")
        {
            return Self::ConnectionRefused(e.to_string());
        }
        if any_contains("dns")
            || any_contains("name resolution")
            || any_contains("nodename nor servname")
            || any_contains("failed to lookup address")
        {
            return Self::DnsFailed(e.to_string());
        }
        if any_contains("tls")
            || any_contains("certificate")
            || any_contains("self-signed")
            || any_contains("self signed")
            || any_contains("ssl")
            || any_contains("handshake")
        {
            return Self::TlsFailed(e.to_string());
        }

        Self::Other(e.to_string())
    }
}

impl From<WasmHttpError> for bindings::torchsnap::gadget::http::HttpError {
    fn from(e: WasmHttpError) -> Self {
        use bindings::torchsnap::gadget::http::HttpError;
        match e {
            WasmHttpError::PermissionDenied(msg) => HttpError::PermissionDenied(msg),
            WasmHttpError::ConnectionRefused(msg) => HttpError::ConnectionRefused(msg),
            WasmHttpError::Timeout => HttpError::Timeout,
            WasmHttpError::DnsFailed(msg) => HttpError::DnsFailed(msg),
            WasmHttpError::TlsFailed(msg) => HttpError::TlsFailed(msg),
            WasmHttpError::InvalidUrl(msg) => HttpError::InvalidUrl(msg),
            WasmHttpError::Other(msg) => HttpError::Other(msg),
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
    let parsed = url::Url::parse(url)
        .map_err(|e| WasmHttpError::InvalidUrl(format!("could not parse URL: {e}")))?;
    let origin = parsed.origin().ascii_serialization();
    if allowed.iter().any(|o| o == &origin) {
        Ok(())
    } else {
        Err(WasmHttpError::PermissionDenied(origin))
    }
}

/// Map a WIT `http-method` variant to a `reqwest::Method`.
/// `other(string)` is validated via `Method::from_bytes` —
/// an invalid method string is rejected before the request
/// is sent.
pub(crate) fn wit_method_to_reqwest(
    method: bindings::torchsnap::gadget::http::HttpMethod,
) -> Result<reqwest::Method, WasmHttpError> {
    use bindings::torchsnap::gadget::http::HttpMethod;
    match method {
        HttpMethod::Get => Ok(reqwest::Method::GET),
        HttpMethod::Post => Ok(reqwest::Method::POST),
        HttpMethod::Put => Ok(reqwest::Method::PUT),
        HttpMethod::Patch => Ok(reqwest::Method::PATCH),
        HttpMethod::Delete => Ok(reqwest::Method::DELETE),
        HttpMethod::Head => Ok(reqwest::Method::HEAD),
        HttpMethod::Other(s) => reqwest::Method::from_bytes(s.as_bytes())
            .map_err(|e| WasmHttpError::Other(format!("invalid HTTP method `{s}`: {e}"))),
    }
}

impl bindings::torchsnap::gadget::http::Host for GadgetState {
    fn fetch(
        &mut self,
        request: bindings::torchsnap::gadget::http::HttpRequest,
    ) -> Result<
        bindings::torchsnap::gadget::http::HttpResponse,
        bindings::torchsnap::gadget::http::HttpError,
    > {
        use bindings::torchsnap::gadget::http::HttpError as WitHttpError;
        use bindings::torchsnap::gadget::http::HttpResponse as WitHttpResponse;

        check_http_origin(&self.http.origins, &request.url).map_err(WitHttpError::from)?;

        let method = wit_method_to_reqwest(request.method).map_err(WitHttpError::from)?;

        // Normal requests use the always-installed client; insecure-TLS
        // requests use a lazily-built parallel client whose underlying
        // reqwest client has `danger_accept_invalid_certs(true)`.
        let client = if request.insecure_tls {
            self.http
                .insecure_client
                .get_or_init(|| {
                    Arc::new(
                        Http::builder()
                            .accept_invalid_certs(true)
                            .build()
                            .expect("insecure-tls reqwest client construction"),
                    )
                })
                .clone()
        } else {
            self.http.client.clone().ok_or_else(|| {
                WitHttpError::from(WasmHttpError::Other("http client not initialized".into()))
            })?
        };

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

impl WasmGadgetInstance {
    /// Stash the origin allowlist for `http::fetch`. Called
    /// by the bridge at `enable()` from the manifest's
    /// `[permissions.http].origins` list (already normalized).
    pub fn set_http_origins(&self, origins: Vec<String>) {
        self.with_state_mut(|state| state.http.origins = origins);
    }

    /// Install the HTTP client for this gadget's `enable()`
    /// lifetime. Called by the bridge.
    pub fn set_http_client(&self, client: Arc<Http>) {
        self.with_state_mut(|state| {
            state.http.client = Some(client);
            // Reset the insecure-client cell so the next
            // insecure request rebuilds it for this enable
            // cycle; otherwise a previously-cached client
            // could leak across enable / disable boundaries.
            state.http.insecure_client = Arc::new(OnceLock::new());
        });
    }

    /// Drop the HTTP client on `disable()`.
    pub fn clear_http_client(&self) {
        self.with_state_mut(|state| {
            state.http.client = None;
            state.http.insecure_client = Arc::new(OnceLock::new());
        });
    }
}
