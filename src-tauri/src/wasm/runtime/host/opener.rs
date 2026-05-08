// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Opener host import
//
// Three-function interface for OS-handler delegation:
//
//   - `open-url`     URL scheme handler (browser, mail, …)
//   - `open-path`    registered application for a path
//   - `reveal-path`  file manager "show in" navigation
//
// Permission model:
//
//   - `open-url` gates on a scheme allowlist
//     (`[permissions.opener] schemes = [...]`).
//   - `open-path` and `reveal-path` are boolean gates
//     (`open-path = true` / `reveal-path = true`).
//
// `check_opener_scheme` is a pure free function so the
// permission logic can be unit-tested without a running
// wasmtime instance.
// =========================================================

use std::sync::Arc;

use crate::gadgets::OpenerCaps;
use crate::wasm::bindings;

use super::super::GadgetState;

/// Opener state: manifest-declared permission gates plus
/// the capability closures. Both are always present when
/// caps exist — the `OpenerCaps` provides the raw ability,
/// the permission fields restrict it per-gadget.
pub(crate) struct OpenerState {
    pub(crate) schemes: Vec<String>,
    pub(crate) open_path: bool,
    pub(crate) reveal_path: bool,
    pub(crate) caps: Arc<OpenerCaps>,
}

impl Default for OpenerState {
    fn default() -> Self {
        Self {
            schemes: Vec::new(),
            open_path: false,
            reveal_path: false,
            caps: Arc::new(OpenerCaps {
                open_url: Box::new(|_| Err("opener not initialized".into())),
                open_path: Box::new(|_| Err("opener not initialized".into())),
                reveal_path: Box::new(|_| Err("opener not initialized".into())),
            }),
        }
    }
}

/// Outcome of a scheme-permission check on a URL passed to
/// `open-url`.
#[derive(Debug)]
pub(crate) enum OpenerSchemeCheckError {
    InvalidUrl,
    SchemeNotPermitted(String),
}

/// Verify that `url`'s scheme is in `allowed`.
pub(crate) fn check_opener_scheme(
    allowed: &[String],
    url: &str,
) -> Result<(), OpenerSchemeCheckError> {
    let parsed = url::Url::parse(url).map_err(|_| OpenerSchemeCheckError::InvalidUrl)?;
    let scheme = parsed.scheme();
    if allowed.iter().any(|s| s.eq_ignore_ascii_case(scheme)) {
        Ok(())
    } else {
        Err(OpenerSchemeCheckError::SchemeNotPermitted(
            scheme.to_string(),
        ))
    }
}

impl bindings::torchsnap::gadget::opener::Host for GadgetState {
    fn open_url(
        &mut self,
        url: String,
    ) -> Result<(), bindings::torchsnap::gadget::opener::OpenerError> {
        use bindings::torchsnap::gadget::opener::OpenerError;

        let caps = self.caps().map_err(OpenerError::BackendFailure)?;

        match check_opener_scheme(&caps.opener.schemes, &url) {
            Ok(()) => {}
            Err(OpenerSchemeCheckError::InvalidUrl) => {
                return Err(OpenerError::InvalidUrl(url));
            }
            Err(OpenerSchemeCheckError::SchemeNotPermitted(scheme)) => {
                return Err(OpenerError::PermissionDenied(format!(
                    "scheme not permitted: {scheme}"
                )));
            }
        }

        (caps.opener.caps.open_url)(&url).map_err(OpenerError::BackendFailure)
    }

    fn open_path(
        &mut self,
        path: String,
    ) -> Result<(), bindings::torchsnap::gadget::opener::OpenerError> {
        use bindings::torchsnap::gadget::opener::OpenerError;

        let caps = self.caps().map_err(OpenerError::BackendFailure)?;

        if !caps.opener.open_path {
            return Err(OpenerError::PermissionDenied(
                "open-path not granted: set `[permissions.opener] open-path = true`".into(),
            ));
        }
        (caps.opener.caps.open_path)(&path).map_err(OpenerError::BackendFailure)
    }

    fn reveal_path(
        &mut self,
        path: String,
    ) -> Result<(), bindings::torchsnap::gadget::opener::OpenerError> {
        use bindings::torchsnap::gadget::opener::OpenerError;

        let caps = self.caps().map_err(OpenerError::BackendFailure)?;

        if !caps.opener.reveal_path {
            return Err(OpenerError::PermissionDenied(
                "reveal-path not granted: set `[permissions.opener] reveal-path = true`".into(),
            ));
        }
        (caps.opener.caps.reveal_path)(&path).map_err(OpenerError::BackendFailure)
    }
}
