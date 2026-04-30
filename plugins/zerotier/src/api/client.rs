// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Hand-rolled HTTP client for the `zerotier-one` service API.
//!
//! Five endpoints (`GET /status`, `GET /network`,
//! `GET /network/{id}`, `POST /network/{id}`,
//! `DELETE /network/{id}`) — small enough that a code-gen
//! pipeline (progenitor, openapi-generator) would weigh more
//! than the hand-rolled types. Every call sets the
//! `X-ZT1-Auth` header from the `Client`'s configured token.

use torchsnap_plugin_sdk::http::{self, HttpMethod, HttpRequest};

use super::error::ApiError;
use super::types::{Network, Status};

/// Tight timeout for `search()`-time fetches. The daemon on
/// localhost normally answers in single-digit milliseconds;
/// 200 ms is a safety net against a hung daemon, sized so a
/// blocked request never freezes the launcher visibly.
const SEARCH_TIMEOUT_MS: u32 = 200;

/// Looser timeout for mutating actions (Connect / Disconnect)
/// and validation (`/status` at `enable()` time). These fire
/// from `execute()`, off the search hot path, so we tolerate
/// a longer ceiling but still bound.
const ACTION_TIMEOUT_MS: u32 = 2_000;

const BASE_URL: &str = "http://localhost:9993";

#[derive(Clone)]
pub struct Client {
    token: String,
}

impl Client {
    pub fn new(token: String) -> Self {
        Self { token }
    }

    /// Fast-path call into the daemon. The 200ms timeout
    /// enforces the launcher-responsiveness contract.
    pub fn list_networks(&self) -> Result<Vec<Network>, ApiError> {
        let body = self.fetch(HttpMethod::Get, "/network", None, SEARCH_TIMEOUT_MS)?;
        serde_json::from_slice(&body).map_err(|e| ApiError::MalformedResponse(e.to_string()))
    }

    /// Refresh a single network's snapshot. Used when the
    /// caller already has the id and wants the latest
    /// state without paying for the full list.
    pub fn get_network(&self, id: &str) -> Result<Network, ApiError> {
        let path = format!("/network/{id}");
        let body = self.fetch(HttpMethod::Get, &path, None, SEARCH_TIMEOUT_MS)?;
        serde_json::from_slice(&body).map_err(|e| ApiError::MalformedResponse(e.to_string()))
    }

    /// Status of the local node. Used at enable-time to
    /// validate that the token the resolver picked is
    /// actually accepted by the daemon (HTTP 200 means
    /// "auth works"; 401/403 means
    /// [`ApiError::AuthRejected`]).
    pub fn status(&self) -> Result<Status, ApiError> {
        let body = self.fetch(HttpMethod::Get, "/status", None, ACTION_TIMEOUT_MS)?;
        serde_json::from_slice(&body).map_err(|e| ApiError::MalformedResponse(e.to_string()))
    }

    /// Join a network. Empty body — the daemon picks defaults
    /// based on the network's controller config. Idempotent
    /// from the daemon's perspective: same call works for
    /// "first join" and "re-Connect to a previously-known
    /// network".
    pub fn join_network(&self, id: &str) -> Result<(), ApiError> {
        let path = format!("/network/{id}");
        // Empty JSON object is the canonical "use defaults" body.
        let body: Vec<u8> = b"{}".to_vec();
        self.fetch(HttpMethod::Post, &path, Some(body), ACTION_TIMEOUT_MS)?;
        Ok(())
    }

    /// Leave a network. The daemon drops its membership and
    /// removes the network from `GET /network`; the plugin
    /// keeps its history row (downgrades to "Stored").
    pub fn leave_network(&self, id: &str) -> Result<(), ApiError> {
        let path = format!("/network/{id}");
        self.fetch(HttpMethod::Delete, &path, None, ACTION_TIMEOUT_MS)?;
        Ok(())
    }

    fn fetch(
        &self,
        method: HttpMethod,
        path: &str,
        body: Option<Vec<u8>>,
        timeout_ms: u32,
    ) -> Result<Vec<u8>, ApiError> {
        let url = format!("{BASE_URL}{path}");
        let mut headers = vec![("X-ZT1-Auth".to_string(), self.token.clone())];
        if body.is_some() {
            headers.push(("Content-Type".to_string(), "application/json".to_string()));
        }
        let request = HttpRequest {
            url,
            method,
            headers,
            body,
            timeout_ms: Some(timeout_ms),
            max_body_size: None,
            insecure_tls: false,
        };
        let response = http::fetch(&request).map_err(ApiError::from_http)?;

        match response.status {
            200..=299 => Ok(response.body),
            401 | 403 => Err(ApiError::AuthRejected),
            status => Err(ApiError::HttpStatus {
                status,
                body: String::from_utf8_lossy(&response.body).to_string(),
            }),
        }
    }
}
