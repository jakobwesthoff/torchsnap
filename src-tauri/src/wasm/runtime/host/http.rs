// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// HTTP host import
//
// Thin bridge layer converting between WIT types and the
// native `HttpCap` capability. Permission checking and
// request execution happen inside `HttpCap`.
// =========================================================

use crate::caps::{HttpCap, HttpCapError, HttpMethod, HttpRequest, HttpResponse};
use crate::wasm::bindings;

use super::super::GadgetState;

// =========================================================
// From impls: WIT → Cap (inputs)
// =========================================================

impl From<bindings::torchsnap::gadget::http::HttpMethod> for HttpMethod {
    fn from(m: bindings::torchsnap::gadget::http::HttpMethod) -> Self {
        use bindings::torchsnap::gadget::http::HttpMethod as WitMethod;
        match m {
            WitMethod::Get => HttpMethod::Get,
            WitMethod::Post => HttpMethod::Post,
            WitMethod::Put => HttpMethod::Put,
            WitMethod::Patch => HttpMethod::Patch,
            WitMethod::Delete => HttpMethod::Delete,
            WitMethod::Head => HttpMethod::Head,
            WitMethod::Other(s) => HttpMethod::Other(s),
        }
    }
}

impl From<bindings::torchsnap::gadget::http::HttpRequest> for HttpRequest {
    fn from(r: bindings::torchsnap::gadget::http::HttpRequest) -> Self {
        HttpRequest {
            url: r.url,
            method: r.method.into(),
            headers: r.headers,
            body: r.body,
            timeout_ms: r.timeout_ms,
            max_body_size: r.max_body_size,
            insecure_tls: r.insecure_tls,
        }
    }
}

// =========================================================
// From impls: Cap → WIT (outputs + errors)
// =========================================================

impl From<HttpResponse> for bindings::torchsnap::gadget::http::HttpResponse {
    fn from(r: HttpResponse) -> Self {
        Self {
            status: r.status,
            headers: r.headers,
            body: r.body,
        }
    }
}

impl From<HttpCapError> for bindings::torchsnap::gadget::http::HttpError {
    fn from(e: HttpCapError) -> Self {
        match e {
            HttpCapError::PermissionDenied(msg) => Self::PermissionDenied(msg),
            HttpCapError::ConnectionRefused(msg) => Self::ConnectionRefused(msg),
            HttpCapError::Timeout => Self::Timeout,
            HttpCapError::DnsFailed(msg) => Self::DnsFailed(msg),
            HttpCapError::TlsFailed(msg) => Self::TlsFailed(msg),
            HttpCapError::InvalidUrl(msg) => Self::InvalidUrl(msg),
            HttpCapError::Other(msg) => Self::Other(msg),
        }
    }
}

// =========================================================
// Host trait impl
// =========================================================

impl bindings::torchsnap::gadget::http::Host for GadgetState {
    fn fetch(
        &mut self,
        request: bindings::torchsnap::gadget::http::HttpRequest,
    ) -> Result<
        bindings::torchsnap::gadget::http::HttpResponse,
        bindings::torchsnap::gadget::http::HttpError,
    > {
        let caps = self
            .caps()
            .map_err(|e| bindings::torchsnap::gadget::http::HttpError::Other(e))?;

        let native_request: HttpRequest = request.into();
        let response = caps
            .http
            .fetch(native_request)
            .map_err(bindings::torchsnap::gadget::http::HttpError::from)?;
        Ok(response.into())
    }
}
