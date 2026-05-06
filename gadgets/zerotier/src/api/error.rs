// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Error categories the API client surfaces to callers.
//!
//! The host's `http-error` is too low-level for gadget-side
//! UX decisions ("daemon unreachable" vs. "auth rejected" vs.
//! "got a JSON we can't parse"), so we map it through a
//! domain-level enum. The launcher entry-rendering path
//! branches on these to pick a failure-state title.

use thiserror::Error;
use torchsnap_gadget_sdk::http::HttpError;

#[derive(Debug, Error)]
pub enum ApiError {
    /// `zerotier-one` is not reachable on the loopback port —
    /// the daemon is not running, or the loopback port has
    /// been blocked.
    #[error("zerotier-one is not reachable")]
    DaemonUnreachable(String),

    /// The auth token the gadget presented was rejected with
    /// HTTP 401 / 403. Surfaced as a "configure token" entry.
    #[error("authentication rejected by zerotier-one")]
    AuthRejected,

    /// `zerotier-one` returned a non-2xx status that isn't an
    /// auth failure. Carries the status code so the caller
    /// can pick a precise error message.
    #[error("zerotier-one returned HTTP {status}")]
    HttpStatus { status: u16, body: String },

    /// `zerotier-one` returned a body that didn't parse as the
    /// expected JSON shape. Likely a daemon-version skew.
    #[error("malformed response from zerotier-one: {0}")]
    MalformedResponse(String),

    /// The fetch host import failed for a reason that doesn't
    /// fit the categories above (timeout, DNS, transport).
    #[error("transport error: {0}")]
    Transport(String),
}

impl ApiError {
    /// Map a host `http-error` into the gadget's error model.
    /// `connection-refused` is the strong signal for "daemon
    /// not running", and `permission-denied` here means the
    /// manifest's origin allowlist rejected the request — that
    /// is a configuration bug, not a runtime failure.
    pub fn from_http(err: HttpError) -> Self {
        match err {
            HttpError::ConnectionRefused(msg) => Self::DaemonUnreachable(msg),
            HttpError::Timeout => {
                Self::DaemonUnreachable("zerotier-one did not respond in time".into())
            }
            HttpError::DnsFailed(msg) => Self::Transport(format!("dns: {msg}")),
            HttpError::TlsFailed(msg) => Self::Transport(format!("tls: {msg}")),
            HttpError::InvalidUrl(msg) => Self::Transport(format!("invalid url: {msg}")),
            HttpError::PermissionDenied(msg) => {
                Self::Transport(format!("origin not in manifest allowlist: {msg}"))
            }
            HttpError::Other(msg) => Self::Transport(msg),
        }
    }
}
