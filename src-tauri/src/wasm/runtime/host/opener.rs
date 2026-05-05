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

use super::super::{GadgetState, WasmGadgetInstance};

/// Closure type for any of the opener writer slots.
///
/// Boxed and stored on `OpenerState` rather than holding a
/// `tauri::AppHandle` so this module never imports
/// Tauri-specific types directly. The bridge constructs the
/// closure from its own `AppHandle` at `enable()` time and
/// stashes it on the per-plugin `OpenerState`.
pub type UrlOpenerFn = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

/// Opener state. Aggregates the URL scheme allowlist, the
/// `open-path` / `reveal-path` capability flags, and the
/// three writer closures. The largest capability surface in
/// the runtime, and the primary motivation for the per-
/// capability grouping.
#[derive(Default)]
pub(crate) struct OpenerState {
    /// URL schemes this plugin is permitted to open via
    /// `opener::open-url`. Populated by the bridge from
    /// `[permissions.opener].schemes` in the manifest. Empty
    /// means deny-all. Compared case-insensitively against
    /// the scheme extracted by the `url` crate (which always
    /// lowercases per RFC 3986).
    pub(crate) schemes: Vec<String>,
    /// Whether the plugin's manifest grants
    /// `opener::open-path`. Drives the gate; the actual
    /// host-side delegation goes through `open_path_writer`.
    pub(crate) open_path: bool,
    /// Whether the plugin's manifest grants
    /// `opener::reveal-path`. Same shape as `open_path`.
    pub(crate) reveal_path: bool,
    /// Closure that opens a URL in the OS default handler.
    /// Bridge constructs from `tauri::AppHandle` on
    /// `enable()` and clears on `disable()`.
    pub(crate) open_url_writer: Option<UrlOpenerFn>,
    /// Closure that opens a filesystem path with the
    /// OS-registered application.
    pub(crate) open_path_writer: Option<UrlOpenerFn>,
    /// Closure that reveals a filesystem path in the OS
    /// file manager.
    pub(crate) reveal_path_writer: Option<UrlOpenerFn>,
}

/// Outcome of a scheme-permission check on a URL passed to
/// `open-url`. Distinct error cases let the Host impl map
/// each into the right `OpenerError` variant for the WIT
/// boundary.
#[derive(Debug)]
pub(crate) enum OpenerSchemeCheckError {
    /// The URL string did not parse.
    InvalidUrl,
    /// The URL parsed but its scheme is not in `allowed`.
    SchemeNotPermitted(String),
}

/// Verify that `url`'s scheme is in `allowed`.
///
/// `allowed` strings are compared case-insensitively against
/// the scheme extracted by the `url` crate (which always
/// lowercases it per RFC 3986).
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

impl bindings::torchsnap::plugin::opener::Host for GadgetState {
    fn open_url(
        &mut self,
        url: String,
    ) -> Result<(), bindings::torchsnap::plugin::opener::OpenerError> {
        use bindings::torchsnap::plugin::opener::OpenerError;

        match check_opener_scheme(&self.opener.schemes, &url) {
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

        let writer = self
            .opener
            .open_url_writer
            .as_ref()
            .ok_or_else(|| OpenerError::BackendFailure("opener not initialized".into()))?;
        writer(&url).map_err(OpenerError::BackendFailure)
    }

    fn open_path(
        &mut self,
        path: String,
    ) -> Result<(), bindings::torchsnap::plugin::opener::OpenerError> {
        use bindings::torchsnap::plugin::opener::OpenerError;

        if !self.opener.open_path {
            return Err(OpenerError::PermissionDenied(
                "open-path not granted: set `[permissions.opener] open-path = true`".into(),
            ));
        }
        let writer = self
            .opener
            .open_path_writer
            .as_ref()
            .ok_or_else(|| OpenerError::BackendFailure("open-path not initialized".into()))?;
        writer(&path).map_err(OpenerError::BackendFailure)
    }

    fn reveal_path(
        &mut self,
        path: String,
    ) -> Result<(), bindings::torchsnap::plugin::opener::OpenerError> {
        use bindings::torchsnap::plugin::opener::OpenerError;

        if !self.opener.reveal_path {
            return Err(OpenerError::PermissionDenied(
                "reveal-path not granted: set `[permissions.opener] reveal-path = true`".into(),
            ));
        }
        let writer = self
            .opener
            .reveal_path_writer
            .as_ref()
            .ok_or_else(|| OpenerError::BackendFailure("reveal-path not initialized".into()))?;
        writer(&path).map_err(OpenerError::BackendFailure)
    }
}

impl WasmGadgetInstance {
    /// Stash the URL scheme allowlist for `opener::open-url`.
    /// Called by the bridge at `enable()` from the manifest's
    /// `[permissions.opener].schemes` list.
    pub fn set_opener_schemes(&self, schemes: Vec<String>) {
        self.with_state_mut(|state| state.opener.schemes = schemes);
    }

    /// Install the closure that opens a URL via the OS default
    /// handler. Called by the bridge at `enable()`.
    pub fn set_opener_writer(&self, writer: UrlOpenerFn) {
        self.with_state_mut(|state| state.opener.open_url_writer = Some(writer));
    }

    /// Drop the opener closure on `disable()`.
    pub fn clear_opener_writer(&self) {
        self.with_state_mut(|state| state.opener.open_url_writer = None);
    }

    /// Stash the `open-path` / `reveal-path` capability flags from
    /// `[permissions.opener]`. Called by the bridge at `enable()`.
    pub fn set_opener_path_capabilities(&self, open_path: bool, reveal_path: bool) {
        self.with_state_mut(|state| {
            state.opener.open_path = open_path;
            state.opener.reveal_path = reveal_path;
        });
    }

    /// Install the closure that opens a filesystem path via the
    /// OS-registered application. Called by the bridge at
    /// `enable()`.
    pub fn set_open_path_writer(&self, writer: UrlOpenerFn) {
        self.with_state_mut(|state| state.opener.open_path_writer = Some(writer));
    }

    /// Drop the `open-path` closure on `disable()`.
    pub fn clear_open_path_writer(&self) {
        self.with_state_mut(|state| state.opener.open_path_writer = None);
    }

    /// Install the closure that reveals a filesystem path in the
    /// OS file manager. Called by the bridge at `enable()`.
    pub fn set_reveal_path_writer(&self, writer: UrlOpenerFn) {
        self.with_state_mut(|state| state.opener.reveal_path_writer = Some(writer));
    }

    /// Drop the `reveal-path` closure on `disable()`.
    pub fn clear_reveal_path_writer(&self) {
        self.with_state_mut(|state| state.opener.reveal_path_writer = None);
    }
}
