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

use crate::wasm::bindings;

use super::super::GadgetState;

/// Closure type for any of the opener writer slots.
///
/// Boxed and stored on `OpenerState` rather than holding a
/// `tauri::AppHandle` so this module never imports
/// Tauri-specific types directly. The bridge constructs the
/// closure from its own `AppHandle` at `enable()` time and
/// stashes it on the per-gadget `OpenerState`.
pub type UrlOpenerFn = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// Opener state. Aggregates the URL scheme allowlist, the
/// `open-path` / `reveal-path` capability flags, and the
/// three writer closures.
#[derive(Default)]
pub(crate) struct OpenerState {
    pub(crate) schemes: Vec<String>,
    pub(crate) open_path: bool,
    pub(crate) reveal_path: bool,
    pub(crate) open_url_writer: Option<UrlOpenerFn>,
    pub(crate) open_path_writer: Option<UrlOpenerFn>,
    pub(crate) reveal_path_writer: Option<UrlOpenerFn>,
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

        let writer = caps
            .opener
            .open_url_writer
            .as_ref()
            .ok_or_else(|| OpenerError::BackendFailure("opener not initialized".into()))?;
        writer(&url).map_err(OpenerError::BackendFailure)
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
        let writer = caps
            .opener
            .open_path_writer
            .as_ref()
            .ok_or_else(|| OpenerError::BackendFailure("open-path not initialized".into()))?;
        writer(&path).map_err(OpenerError::BackendFailure)
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
        let writer = caps
            .opener
            .reveal_path_writer
            .as_ref()
            .ok_or_else(|| OpenerError::BackendFailure("reveal-path not initialized".into()))?;
        writer(&path).map_err(OpenerError::BackendFailure)
    }
}
