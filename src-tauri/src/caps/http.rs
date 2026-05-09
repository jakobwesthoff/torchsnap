// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// HttpCap
//
// Permissioned HTTP client capability. Origin checking is
// internal — a wildcard `"*"` entry permits any origin, and
// the check short-circuits before URL parsing.
//
// Two clients are maintained: a default one and a lazily
// initialized insecure-TLS variant for `insecure_tls`
// requests.
// =========================================================

use std::sync::{Arc, OnceLock};
use std::time::Duration;

use crate::network::Http;

// =========================================================
// Native types
// =========================================================

#[derive(Debug)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Other(String),
}

pub struct HttpRequest {
    pub url: String,
    pub method: HttpMethod,
    pub headers: Vec<(String, String)>,
    pub body: Option<Vec<u8>>,
    pub timeout_ms: Option<u32>,
    pub max_body_size: Option<u64>,
    pub insecure_tls: bool,
}

#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum HttpCapError {
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

impl HttpCapError {
    fn from_request_error(e: anyhow::Error) -> Self {
        if let Some(re) = e.downcast_ref::<reqwest::Error>()
            && re.is_timeout()
        {
            return Self::Timeout;
        }

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

// =========================================================
// HttpCap
// =========================================================

pub struct HttpCap {
    origins: Vec<String>,
    client: Arc<Http>,
    insecure_client: Arc<OnceLock<Arc<Http>>>,
}

impl HttpCap {
    pub fn new(origins: Vec<String>) -> Self {
        Self {
            origins,
            client: Arc::new(Http::new()),
            insecure_client: Arc::new(OnceLock::new()),
        }
    }

    /// Construct with a pre-built client. Used when the default
    /// client configuration is not appropriate (e.g., tests that
    /// need a client pointing at a mock server).
    pub fn with_client(origins: Vec<String>, client: Arc<Http>) -> Self {
        Self {
            origins,
            client,
            insecure_client: Arc::new(OnceLock::new()),
        }
    }

    pub fn fetch(&self, request: HttpRequest) -> Result<HttpResponse, HttpCapError> {
        check_origin(&self.origins, &request.url)?;

        let method = to_reqwest_method(request.method)?;

        let client = if request.insecure_tls {
            self.insecure_client
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
            Arc::clone(&self.client)
        };

        let mut builder = client.request(method, &request.url);
        for (k, v) in &request.headers {
            builder = builder.header(k, v);
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

        let (status, headers, body) =
            tokio::task::block_in_place(|| -> Result<_, HttpCapError> {
                let mut response = builder.send().map_err(HttpCapError::from_request_error)?;
                let status = response.status();
                let headers = response.headers().to_vec();
                let body = response
                    .bytes()
                    .map_err(HttpCapError::from_request_error)?;
                Ok((status, headers, body))
            })?;

        Ok(HttpResponse {
            status,
            headers,
            body,
        })
    }
}

fn check_origin(allowed: &[String], url: &str) -> Result<(), HttpCapError> {
    if allowed.iter().any(|o| o == "*") {
        return Ok(());
    }
    let parsed = url::Url::parse(url)
        .map_err(|e| HttpCapError::InvalidUrl(format!("could not parse URL: {e}")))?;
    let origin = parsed.origin().ascii_serialization();
    if allowed.iter().any(|o| o == &origin) {
        Ok(())
    } else {
        Err(HttpCapError::PermissionDenied(origin))
    }
}

fn to_reqwest_method(method: HttpMethod) -> Result<reqwest::Method, HttpCapError> {
    match method {
        HttpMethod::Get => Ok(reqwest::Method::GET),
        HttpMethod::Post => Ok(reqwest::Method::POST),
        HttpMethod::Put => Ok(reqwest::Method::PUT),
        HttpMethod::Patch => Ok(reqwest::Method::PATCH),
        HttpMethod::Delete => Ok(reqwest::Method::DELETE),
        HttpMethod::Head => Ok(reqwest::Method::HEAD),
        HttpMethod::Other(s) => reqwest::Method::from_bytes(s.as_bytes())
            .map_err(|e| HttpCapError::Other(format!("invalid HTTP method `{s}`: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap_with_origins(origins: &[&str]) -> HttpCap {
        HttpCap::new(origins.iter().map(|s| s.to_string()).collect())
    }

    // ─── Origin checking ─────────────────────────────────────

    #[test]
    fn exact_origin_match_passes() {
        assert!(check_origin(
            &["https://example.com".into()],
            "https://example.com/path"
        )
        .is_ok());
    }

    #[test]
    fn non_matching_origin_denied() {
        let err = check_origin(
            &["https://example.com".into()],
            "https://other.com/x",
        )
        .unwrap_err();
        assert!(matches!(err, HttpCapError::PermissionDenied(_)));
    }

    #[test]
    fn wildcard_allows_any_origin() {
        assert!(check_origin(&["*".into()], "https://any-host.example/path").is_ok());
    }

    #[test]
    fn empty_origins_denies_everything() {
        assert!(check_origin(&[], "https://example.com").is_err());
    }

    #[test]
    fn non_default_port_included_in_origin() {
        assert!(check_origin(
            &["https://example.com:8443".into()],
            "https://example.com:8443/api"
        )
        .is_ok());
    }

    #[test]
    fn default_port_stripped_from_origin() {
        assert!(check_origin(
            &["https://example.com".into()],
            "https://example.com:443/api"
        )
        .is_ok());
    }

    #[test]
    fn wrong_port_is_different_origin() {
        let err = check_origin(
            &["https://example.com".into()],
            "https://example.com:8080/api",
        )
        .unwrap_err();
        assert!(matches!(err, HttpCapError::PermissionDenied(_)));
    }

    #[test]
    fn unparseable_url_returns_invalid_url() {
        let err = check_origin(&["https://example.com".into()], "not-a-url").unwrap_err();
        assert!(matches!(err, HttpCapError::InvalidUrl(_)));
    }

    #[test]
    fn wildcard_short_circuits_before_url_parse() {
        assert!(check_origin(&["*".into()], "not-a-url").is_ok());
    }

    // ─── Method conversion ───────────────────────────────────

    #[test]
    fn named_methods_map_correctly() {
        assert_eq!(to_reqwest_method(HttpMethod::Get).unwrap(), reqwest::Method::GET);
        assert_eq!(to_reqwest_method(HttpMethod::Post).unwrap(), reqwest::Method::POST);
        assert_eq!(to_reqwest_method(HttpMethod::Put).unwrap(), reqwest::Method::PUT);
        assert_eq!(to_reqwest_method(HttpMethod::Patch).unwrap(), reqwest::Method::PATCH);
        assert_eq!(to_reqwest_method(HttpMethod::Delete).unwrap(), reqwest::Method::DELETE);
        assert_eq!(to_reqwest_method(HttpMethod::Head).unwrap(), reqwest::Method::HEAD);
    }

    #[test]
    fn other_valid_method() {
        let m = to_reqwest_method(HttpMethod::Other("PROPFIND".into())).unwrap();
        assert_eq!(m.as_str(), "PROPFIND");
    }

    #[test]
    fn other_invalid_method() {
        assert!(to_reqwest_method(HttpMethod::Other("BAD METHOD".into())).is_err());
    }

    // ─── Error classification ────────────────────────────────

    #[test]
    fn error_display_includes_message() {
        let err = HttpCapError::PermissionDenied("test-origin".into());
        assert!(err.to_string().contains("test-origin"));

        let err = HttpCapError::InvalidUrl("bad".into());
        assert!(err.to_string().contains("bad"));
    }

    // ─── HttpCap construction ────────────────────────────────

    #[test]
    fn new_creates_with_origins() {
        let cap = cap_with_origins(&["https://example.com"]);
        assert_eq!(cap.origins, vec!["https://example.com".to_string()]);
    }

    // ─── Insecure client lazy init ───────────────────────────

    #[test]
    fn insecure_client_not_initialized_until_needed() {
        let cap = cap_with_origins(&["*"]);
        assert!(
            cap.insecure_client.get().is_none(),
            "insecure client should not exist before first insecure request"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn insecure_client_initialized_on_first_insecure_request() {
        use httpmock::prelude::*;
        let server = MockServer::start();
        let _mock = server.mock(|when, then| {
            when.method(GET).path("/test");
            then.status(200).body("ok");
        });

        let client = Arc::new(Http::new());
        let cap = HttpCap::with_client(vec!["*".into()], client);

        assert!(cap.insecure_client.get().is_none());

        let _ = cap.fetch(HttpRequest {
            url: server.url("/test"),
            method: HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout_ms: None,
            max_body_size: None,
            insecure_tls: true,
        });

        assert!(
            cap.insecure_client.get().is_some(),
            "insecure client should be initialized after insecure request"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn normal_request_does_not_init_insecure_client() {
        use httpmock::prelude::*;
        let server = MockServer::start();
        let _mock = server.mock(|when, then| {
            when.method(GET).path("/test");
            then.status(200).body("ok");
        });

        let client = Arc::new(Http::new());
        let cap = HttpCap::with_client(vec!["*".into()], client);

        let _ = cap.fetch(HttpRequest {
            url: server.url("/test"),
            method: HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout_ms: None,
            max_body_size: None,
            insecure_tls: false,
        });

        assert!(
            cap.insecure_client.get().is_none(),
            "insecure client should not be initialized for normal requests"
        );
    }

    // ─── Timeout forwarding ──────────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn timeout_ms_causes_timeout_error_on_slow_server() {
        use httpmock::prelude::*;
        let server = MockServer::start();
        let _mock = server.mock(|when, then| {
            when.method(GET).path("/slow");
            then.status(200)
                .body("ok")
                .delay(std::time::Duration::from_secs(5));
        });

        let client = Arc::new(Http::new());
        let cap = HttpCap::with_client(vec!["*".into()], client);

        let result = cap.fetch(HttpRequest {
            url: server.url("/slow"),
            method: HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout_ms: Some(50),
            max_body_size: None,
            insecure_tls: false,
        });

        assert!(
            matches!(result, Err(HttpCapError::Timeout)),
            "expected Timeout error, got {result:?}"
        );
    }

    // ─── Max body size forwarding ────────────────────────────

    #[tokio::test(flavor = "multi_thread")]
    async fn max_body_size_limits_response() {
        use httpmock::prelude::*;
        let server = MockServer::start();
        let large_body = "x".repeat(10_000);
        let _mock = server.mock(|when, then| {
            when.method(GET).path("/large");
            then.status(200).body(&large_body);
        });

        let client = Arc::new(Http::new());
        let cap = HttpCap::with_client(vec!["*".into()], client);

        let result = cap.fetch(HttpRequest {
            url: server.url("/large"),
            method: HttpMethod::Get,
            headers: vec![],
            body: None,
            timeout_ms: None,
            max_body_size: Some(100),
            insecure_tls: false,
        });

        assert!(result.is_err(), "should fail when response exceeds max_body_size");
    }
}
