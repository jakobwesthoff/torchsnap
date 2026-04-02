// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// HTTP Client
//
// Driver-hiding wrapper around reqwest. The public API is
// entirely synchronous — internally it uses an async
// reqwest::Client and bridges to sync via
// `tokio::runtime::Handle::current().block_on()`.
//
// reqwest::blocking::Client is deliberately avoided because
// it panics when used inside a tokio runtime, which is where
// both `spawn_blocking` (native plugins) and future WASM host
// functions execute.
//
// The response type wraps a live reqwest stream. Body data is
// read lazily via `read_chunk()`, allowing callers to process
// data incrementally and abort early when they have enough.
// Convenience methods like `bytes()`, `text()`, and `json()`
// consume the full stream for the common one-shot case.
//
// All public types use owned, primitive data (u16, String,
// Vec<u8>) so they can be mapped 1:1 onto WIT records and
// resources when WASM plugin support is added.
// =========================================================

use std::time::Duration;

use anyhow::{Context, Result, bail};

// =========================================================
// Default Configuration
// =========================================================

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// No size limit by default. Callers opt into limits per-plugin
/// or per-request via `HttpBuilder::max_size` or
/// `RequestBuilder::max_size`.
const DEFAULT_MAX_SIZE: u64 = u64::MAX;

/// Realistic Chrome-on-macOS user agent string. `Torchsnap/0.1`
/// is appended in the product token extension position, where
/// real browsers list extra engines and extensions. This avoids
/// triggering bot detection on sites that inspect the UA string.
const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) \
    AppleWebKit/537.36 (KHTML, like Gecko) Chrome/136.0.0.0 Safari/537.36 Torchsnap/0.1";

// =========================================================
// Http — the client handle
// =========================================================

/// Per-plugin HTTP client with configurable defaults.
///
/// Wraps a `reqwest::Client` (which is internally `Arc`-based,
/// so cloning `Http` is cheap). Each plugin constructs its own
/// instance in `setup()`, matching the `SqlStorage` ownership
/// pattern.
pub struct Http {
    client: reqwest::Client,
    default_timeout: Duration,
    default_max_size: u64,
}

impl Http {
    /// Create a client with default configuration (30s timeout,
    /// no size limit, Chrome-like user agent).
    pub fn new() -> Self {
        Self::builder()
            .build()
            .expect("default Http configuration is valid")
    }

    /// Start building a client with custom defaults.
    pub fn builder() -> HttpBuilder {
        HttpBuilder {
            timeout: DEFAULT_TIMEOUT,
            max_size: DEFAULT_MAX_SIZE,
            user_agent: DEFAULT_USER_AGENT.to_string(),
        }
    }

    pub fn get(&self, url: &str) -> RequestBuilder {
        self.request(reqwest::Method::GET, url)
    }

    pub fn post(&self, url: &str) -> RequestBuilder {
        self.request(reqwest::Method::POST, url)
    }

    pub fn put(&self, url: &str) -> RequestBuilder {
        self.request(reqwest::Method::PUT, url)
    }

    pub fn patch(&self, url: &str) -> RequestBuilder {
        self.request(reqwest::Method::PATCH, url)
    }

    pub fn delete(&self, url: &str) -> RequestBuilder {
        self.request(reqwest::Method::DELETE, url)
    }

    /// Build a request with an arbitrary HTTP method.
    pub fn request(&self, method: reqwest::Method, url: &str) -> RequestBuilder {
        RequestBuilder {
            client: self.client.clone(),
            method,
            url: url.to_string(),
            headers: Vec::new(),
            body: None,
            timeout: None,
            max_size: None,
            default_timeout: self.default_timeout,
            default_max_size: self.default_max_size,
        }
    }
}

// =========================================================
// HttpBuilder — construct Http with custom defaults
// =========================================================

/// Builder for configuring an `Http` client before use.
pub struct HttpBuilder {
    timeout: Duration,
    max_size: u64,
    user_agent: String,
}

impl HttpBuilder {
    /// Set the default request timeout. Individual requests can
    /// override this via `RequestBuilder::timeout`.
    pub fn default_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the default maximum response body size in bytes.
    /// Individual requests can override via
    /// `RequestBuilder::max_size`.
    pub fn max_size(mut self, bytes: u64) -> Self {
        self.max_size = bytes;
        self
    }

    /// Set the User-Agent header for all requests.
    pub fn user_agent(mut self, ua: &str) -> Self {
        self.user_agent = ua.to_string();
        self
    }

    /// Build the `Http` client. Fails if the underlying reqwest
    /// client cannot be constructed (e.g. invalid TLS config).
    pub fn build(self) -> Result<Http> {
        let client = reqwest::Client::builder()
            .user_agent(&self.user_agent)
            .build()
            .context("build HTTP client")?;

        Ok(Http {
            client,
            default_timeout: self.timeout,
            default_max_size: self.max_size,
        })
    }
}

// =========================================================
// RequestBuilder — configure a single request
// =========================================================

/// Builder for a single HTTP request. Obtained from `Http::get`,
/// `Http::post`, etc. The builder accumulates configuration and
/// executes the request on `send()`.
///
/// On the WASM boundary this flattens to a `request-options`
/// record — the builder pattern is Rust-side ergonomics only.
pub struct RequestBuilder {
    client: reqwest::Client,
    method: reqwest::Method,
    url: String,
    headers: Vec<(String, String)>,
    body: Option<Vec<u8>>,
    timeout: Option<Duration>,
    max_size: Option<u64>,
    default_timeout: Duration,
    default_max_size: u64,
}

impl RequestBuilder {
    /// Add a request header. Can be called multiple times for
    /// multiple headers (or duplicate header names).
    pub fn header(mut self, key: &str, value: &str) -> Self {
        self.headers.push((key.to_string(), value.to_string()));
        self
    }

    /// Set the request body to a JSON-serialized value. Also sets
    /// the `Content-Type` header to `application/json`.
    ///
    /// This is a Rust convenience — on the WIT boundary, callers
    /// serialize to bytes themselves and pass raw body data.
    pub fn json<T: serde::Serialize>(mut self, body: &T) -> Result<Self> {
        let bytes = serde_json::to_vec(body).context("serialize request body to JSON")?;
        self.body = Some(bytes);
        self.headers
            .push(("Content-Type".to_string(), "application/json".to_string()));
        Ok(self)
    }

    /// Set a raw byte body for the request.
    pub fn body(mut self, bytes: Vec<u8>) -> Self {
        self.body = Some(bytes);
        self
    }

    /// Override the timeout for this specific request.
    pub fn timeout(mut self, duration: Duration) -> Self {
        self.timeout = Some(duration);
        self
    }

    /// Override the maximum response body size for this request.
    pub fn max_size(mut self, bytes: u64) -> Self {
        self.max_size = Some(bytes);
        self
    }

    /// Execute the request and return a streaming response.
    ///
    /// This initiates the HTTP request, reads the status and
    /// headers, and checks Content-Length against the max size
    /// limit (rejecting early if the server advertises a body
    /// larger than allowed). The response body is not read yet —
    /// use `read_chunk()`, `bytes()`, `text()`, or `json()` on
    /// the returned `HttpResponse` to consume it.
    pub fn send(self) -> Result<HttpResponse> {
        let timeout = self.timeout.unwrap_or(self.default_timeout);
        let max_size = self.max_size.unwrap_or(self.default_max_size);

        // Build the reqwest request with all accumulated options.
        let mut request = self.client.request(self.method, &self.url);
        request = request.timeout(timeout);

        for (key, value) in &self.headers {
            request = request.header(key, value);
        }

        if let Some(body) = self.body {
            request = request.body(body);
        }

        // Bridge sync→async: execute the request on the current
        // tokio runtime. This is safe from `spawn_blocking` and
        // future WASM host function contexts.
        let handle = tokio::runtime::Handle::current();
        let response = handle
            .block_on(request.send())
            .context("send HTTP request")?;

        let status = response.status().as_u16();

        // Materialize response headers into owned pairs. HTTP
        // allows duplicate header names (e.g. Set-Cookie), so we
        // use a Vec rather than a HashMap.
        let headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(name, value)| {
                (
                    name.as_str().to_string(),
                    value.to_str().unwrap_or("").to_string(),
                )
            })
            .collect();

        // Early reject if Content-Length exceeds the size limit,
        // before reading any body bytes.
        if let Some(content_length) = response.content_length() {
            if content_length > max_size {
                bail!(
                    "response Content-Length ({content_length} bytes) exceeds \
                     max size limit ({max_size} bytes)"
                );
            }
        }

        Ok(HttpResponse {
            status,
            headers,
            inner: Some(response),
            buffer: Vec::new(),
            max_size,
        })
    }
}

// =========================================================
// HttpResponse — streaming response with lazy body consumption
// =========================================================

/// An HTTP response with a lazily-consumed body stream.
///
/// After `send()`, the status and headers are immediately
/// available. The body is read incrementally via `read_chunk()`,
/// or all at once via `bytes()` / `text()` / `json()`.
///
/// On the WASM boundary this maps to a WIT resource — the guest
/// drives the read loop, and `abort()` lets it stop early without
/// downloading the full response.
pub struct HttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    /// The live reqwest response stream. Set to `None` after the
    /// stream is fully consumed or aborted.
    inner: Option<reqwest::Response>,
    /// Running buffer of all chunks read so far.
    buffer: Vec<u8>,
    max_size: u64,
}

impl HttpResponse {
    // ---------------------------------------------------------
    // Metadata — available immediately after send()
    // ---------------------------------------------------------

    /// HTTP status code (e.g. 200, 404).
    pub fn status(&self) -> u16 {
        self.status
    }

    /// All response headers as name-value pairs. Duplicate header
    /// names are preserved as separate entries.
    pub fn headers(&self) -> &[(String, String)] {
        &self.headers
    }

    /// Look up the first header matching `name` (case-insensitive).
    pub fn header(&self, name: &str) -> Option<&str> {
        let lower = name.to_lowercase();
        self.headers
            .iter()
            .find(|(k, _)| k.to_lowercase() == lower)
            .map(|(_, v)| v.as_str())
    }

    // ---------------------------------------------------------
    // Incremental consumption
    // ---------------------------------------------------------

    /// Read the next network chunk from the response body.
    ///
    /// Returns `Ok(Some(chunk))` for each chunk of data received,
    /// `Ok(None)` when the body is fully consumed, or an error if
    /// the running total exceeds `max_size`.
    ///
    /// Each chunk is also appended to the internal buffer,
    /// accessible via `accumulated()`.
    pub fn read_chunk(&mut self) -> Result<Option<Vec<u8>>> {
        let Some(ref _response) = self.inner else {
            // Stream already consumed or aborted.
            return Ok(None);
        };

        // Bridge sync→async for the chunk read.
        let handle = tokio::runtime::Handle::current();

        // We need a mutable reference to the response for chunk(),
        // but self.inner is an Option — take it out temporarily.
        let mut response = self.inner.take().expect("checked above");
        let chunk_result = handle.block_on(response.chunk());

        match chunk_result {
            Ok(Some(data)) => {
                let new_total = self.buffer.len() + data.len();
                if new_total as u64 > self.max_size {
                    // Drop the response to close the connection.
                    bail!(
                        "response body ({new_total} bytes so far) exceeds \
                         max size limit ({} bytes)",
                        self.max_size
                    );
                }

                let chunk = data.to_vec();
                self.buffer.extend_from_slice(&chunk);
                // Put the response back for further reading.
                self.inner = Some(response);
                Ok(Some(chunk))
            }
            Ok(None) => {
                // Stream exhausted — don't put the response back.
                Ok(None)
            }
            Err(e) => Err(e).context("read response chunk"),
        }
    }

    /// Everything downloaded so far, across all prior `read_chunk()`
    /// calls. This is a view into the internal buffer, not a new
    /// allocation.
    pub fn accumulated(&self) -> &[u8] {
        &self.buffer
    }

    /// Close the connection and stop transferring data. After
    /// calling this, `read_chunk()` will return `Ok(None)` and the
    /// accumulated buffer remains available.
    pub fn abort(&mut self) {
        // Dropping the reqwest::Response closes the connection.
        self.inner = None;
    }

    // ---------------------------------------------------------
    // Full consumption — read all remaining data
    // ---------------------------------------------------------

    /// Read all remaining body data and return it as bytes.
    ///
    /// If `read_chunk()` was called before this, the returned
    /// `Vec` includes those earlier chunks too.
    pub fn bytes(&mut self) -> Result<Vec<u8>> {
        while self.read_chunk()?.is_some() {}
        Ok(std::mem::take(&mut self.buffer))
    }

    /// Read all remaining body data and decode as UTF-8 text.
    pub fn text(&mut self) -> Result<String> {
        let bytes = self.bytes()?;
        String::from_utf8(bytes).context("decode response body as UTF-8")
    }

    /// Read all remaining body data and deserialize as JSON.
    ///
    /// This is a Rust convenience method — on the WIT boundary,
    /// the guest receives raw bytes and deserializes on its side.
    pub fn json<T: serde::de::DeserializeOwned>(&mut self) -> Result<T> {
        let bytes = self.bytes()?;
        serde_json::from_slice(&bytes).context("deserialize response body as JSON")
    }
}

// =========================================================
// Tests
// =========================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_client_with_defaults() {
        let http = Http::new();
        assert_eq!(http.default_timeout, DEFAULT_TIMEOUT);
        assert_eq!(http.default_max_size, DEFAULT_MAX_SIZE);
    }

    #[test]
    fn builder_overrides_defaults() {
        let http = Http::builder()
            .default_timeout(Duration::from_secs(10))
            .max_size(1024)
            .user_agent("custom-agent/1.0")
            .build()
            .expect("build custom Http");

        assert_eq!(http.default_timeout, Duration::from_secs(10));
        assert_eq!(http.default_max_size, 1024);
    }

    #[test]
    fn request_builder_accumulates_headers() {
        let http = Http::new();
        let req = http
            .get("https://example.com")
            .header("Accept", "text/html")
            .header("X-Custom", "value");

        assert_eq!(req.headers.len(), 2);
        assert_eq!(
            req.headers[0],
            ("Accept".to_string(), "text/html".to_string())
        );
        assert_eq!(
            req.headers[1],
            ("X-Custom".to_string(), "value".to_string())
        );
    }

    #[test]
    fn request_builder_json_sets_content_type() {
        let http = Http::new();
        let req = http
            .post("https://example.com")
            .json(&serde_json::json!({"key": "value"}))
            .expect("serialize json");

        assert!(req.body.is_some());
        assert!(
            req.headers
                .iter()
                .any(|(k, v)| k == "Content-Type" && v == "application/json")
        );
    }

    #[test]
    fn request_builder_per_request_overrides() {
        let http = Http::new();
        let req = http
            .get("https://example.com")
            .timeout(Duration::from_secs(5))
            .max_size(2048);

        assert_eq!(req.timeout, Some(Duration::from_secs(5)));
        assert_eq!(req.max_size, Some(2048));
    }

    #[test]
    fn http_response_header_lookup_is_case_insensitive() {
        let resp = HttpResponse {
            status: 200,
            headers: vec![
                ("Content-Type".to_string(), "text/html".to_string()),
                ("X-Request-Id".to_string(), "abc123".to_string()),
            ],
            inner: None,
            buffer: Vec::new(),
            max_size: DEFAULT_MAX_SIZE,
        };

        assert_eq!(resp.header("content-type"), Some("text/html"));
        assert_eq!(resp.header("CONTENT-TYPE"), Some("text/html"));
        assert_eq!(resp.header("Content-Type"), Some("text/html"));
        assert_eq!(resp.header("x-request-id"), Some("abc123"));
        assert_eq!(resp.header("nonexistent"), None);
    }

    #[test]
    fn http_response_accumulated_empty_without_reads() {
        let resp = HttpResponse {
            status: 200,
            headers: Vec::new(),
            inner: None,
            buffer: Vec::new(),
            max_size: DEFAULT_MAX_SIZE,
        };

        assert!(resp.accumulated().is_empty());
    }

    #[test]
    fn http_response_read_chunk_returns_none_when_aborted() {
        let mut resp = HttpResponse {
            status: 200,
            headers: Vec::new(),
            inner: None,
            buffer: vec![1, 2, 3],
            max_size: DEFAULT_MAX_SIZE,
        };

        // No inner stream → read_chunk returns None.
        assert!(resp.read_chunk().expect("no error").is_none());
        // Buffer is still accessible.
        assert_eq!(resp.accumulated(), &[1, 2, 3]);
    }

    #[test]
    fn abort_preserves_buffer() {
        let mut resp = HttpResponse {
            status: 200,
            headers: Vec::new(),
            inner: None,
            buffer: vec![10, 20, 30],
            max_size: DEFAULT_MAX_SIZE,
        };

        resp.abort();
        assert_eq!(resp.accumulated(), &[10, 20, 30]);
        assert!(resp.read_chunk().expect("no error").is_none());
    }
}
