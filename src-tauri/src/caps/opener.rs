// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// OpenerCap
//
// Unified opener capability for URL, path, and reveal
// operations. Permission checking is internal:
//
//   - `open_url` gates on a scheme allowlist. A wildcard
//     entry `"*"` permits any scheme.
//   - `open_path` and `reveal_path` are boolean gates.
//
// Constructed with both permission config and platform
// closures. The closures wrap the Tauri opener plugin;
// tests inject mock closures for isolation.
// =========================================================

/// Error type for opener operations.
#[derive(Debug)]
pub enum OpenerError {
    PermissionDenied(String),
    InvalidUrl(String),
    BackendFailure(String),
}

impl std::fmt::Display for OpenerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OpenerError::PermissionDenied(msg) => write!(f, "permission denied: {msg}"),
            OpenerError::InvalidUrl(msg) => write!(f, "invalid URL: {msg}"),
            OpenerError::BackendFailure(msg) => write!(f, "backend failure: {msg}"),
        }
    }
}

impl std::error::Error for OpenerError {}

/// Permission configuration for constructing an `OpenerCap`.
pub struct OpenerPermissions {
    pub schemes: Vec<String>,
    pub open_path: bool,
    pub reveal_path: bool,
}

/// Platform opener backend: takes the URL or path to act on,
/// returns a backend error message on failure. Shared shape for
/// `open_url_fn`, `open_path_fn`, and `reveal_path_fn`.
type OpenerBackendFn = Box<dyn Fn(&str) -> Result<(), String> + Send + Sync>;

pub struct OpenerCap {
    schemes: Vec<String>,
    allow_open_path: bool,
    allow_reveal_path: bool,
    open_url_fn: OpenerBackendFn,
    open_path_fn: OpenerBackendFn,
    reveal_path_fn: OpenerBackendFn,
}

impl OpenerCap {
    /// Construct from permission config and an `AppHandle`.
    pub fn from_app(app: &tauri::AppHandle, permissions: OpenerPermissions) -> Self {
        use tauri_plugin_opener::OpenerExt;

        let url_app = app.clone();
        let path_app = app.clone();
        let reveal_app = app.clone();

        Self::from_closures(
            permissions,
            Box::new(move |url: &str| {
                url_app
                    .opener()
                    .open_url(url, None::<&str>)
                    .map_err(|e| e.to_string())
            }),
            Box::new(move |path: &str| {
                path_app
                    .opener()
                    .open_path(path, None::<&str>)
                    .map_err(|e| e.to_string())
            }),
            Box::new(move |path: &str| {
                reveal_app
                    .opener()
                    .reveal_item_in_dir(path)
                    .map_err(|e| e.to_string())
            }),
        )
    }

    /// Construct from permission config and explicit closures.
    pub fn from_closures(
        permissions: OpenerPermissions,
        open_url_fn: OpenerBackendFn,
        open_path_fn: OpenerBackendFn,
        reveal_path_fn: OpenerBackendFn,
    ) -> Self {
        Self {
            schemes: permissions.schemes,
            allow_open_path: permissions.open_path,
            allow_reveal_path: permissions.reveal_path,
            open_url_fn,
            open_path_fn,
            reveal_path_fn,
        }
    }

    pub fn open_url(&self, url: &str) -> Result<(), OpenerError> {
        check_scheme(&self.schemes, url)?;
        (self.open_url_fn)(url).map_err(OpenerError::BackendFailure)
    }

    pub fn open_path(&self, path: &str) -> Result<(), OpenerError> {
        if !self.allow_open_path {
            return Err(OpenerError::PermissionDenied(
                "open-path not granted".into(),
            ));
        }
        (self.open_path_fn)(path).map_err(OpenerError::BackendFailure)
    }

    pub fn reveal_path(&self, path: &str) -> Result<(), OpenerError> {
        if !self.allow_reveal_path {
            return Err(OpenerError::PermissionDenied(
                "reveal-path not granted".into(),
            ));
        }
        (self.reveal_path_fn)(path).map_err(OpenerError::BackendFailure)
    }
}

fn check_scheme(allowed: &[String], url: &str) -> Result<(), OpenerError> {
    if allowed.iter().any(|s| s == "*") {
        return Ok(());
    }
    let parsed = url::Url::parse(url).map_err(|_| OpenerError::InvalidUrl(url.to_string()))?;
    let scheme = parsed.scheme();
    if allowed.iter().any(|s| s.eq_ignore_ascii_case(scheme)) {
        Ok(())
    } else {
        Err(OpenerError::PermissionDenied(format!(
            "scheme not permitted: {scheme}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn noop_closures() -> (OpenerBackendFn, OpenerBackendFn, OpenerBackendFn) {
        (
            Box::new(|_| Ok(())),
            Box::new(|_| Ok(())),
            Box::new(|_| Ok(())),
        )
    }

    fn cap_with_schemes(schemes: &[&str]) -> OpenerCap {
        let (url_fn, path_fn, reveal_fn) = noop_closures();
        OpenerCap::from_closures(
            OpenerPermissions {
                schemes: schemes.iter().map(|s| s.to_string()).collect(),
                open_path: true,
                reveal_path: true,
            },
            url_fn,
            path_fn,
            reveal_fn,
        )
    }

    fn cap_with_path_permissions(open_path: bool, reveal_path: bool) -> OpenerCap {
        let (url_fn, path_fn, reveal_fn) = noop_closures();
        OpenerCap::from_closures(
            OpenerPermissions {
                schemes: vec!["*".to_string()],
                open_path,
                reveal_path,
            },
            url_fn,
            path_fn,
            reveal_fn,
        )
    }

    // ─── Scheme checking ─────────────────────────────────────

    #[test]
    fn open_url_permitted_scheme_passes() {
        assert!(
            cap_with_schemes(&["https"])
                .open_url("https://example.com")
                .is_ok()
        );
    }

    #[test]
    fn open_url_forbidden_scheme_blocked() {
        let err = cap_with_schemes(&["https"])
            .open_url("ftp://example.com")
            .unwrap_err();
        assert!(matches!(err, OpenerError::PermissionDenied(_)));
    }

    #[test]
    fn open_url_empty_allowlist_denies_everything() {
        assert!(
            cap_with_schemes(&[])
                .open_url("https://example.com")
                .is_err()
        );
    }

    #[test]
    fn open_url_unparseable_url_returns_error() {
        let err = cap_with_schemes(&["https"])
            .open_url("not-a-url")
            .unwrap_err();
        assert!(matches!(err, OpenerError::InvalidUrl(_)));
    }

    #[test]
    fn open_url_scheme_check_is_case_insensitive() {
        assert!(
            cap_with_schemes(&["https"])
                .open_url("HTTPS://example.com")
                .is_ok()
        );
    }

    #[test]
    fn open_url_multiple_schemes_second_matches() {
        assert!(
            cap_with_schemes(&["https", "mailto"])
                .open_url("mailto:user@x.com")
                .is_ok()
        );
    }

    #[test]
    fn open_url_wildcard_allows_any_scheme() {
        let cap = cap_with_schemes(&["*"]);
        assert!(cap.open_url("https://example.com").is_ok());
        assert!(cap.open_url("ftp://example.com").is_ok());
        assert!(cap.open_url("mailto:user@x.com").is_ok());
        assert!(cap.open_url("custom://anything").is_ok());
    }

    #[test]
    fn open_url_wildcard_short_circuits_before_url_parse() {
        assert!(cap_with_schemes(&["*"]).open_url("not-a-url").is_ok());
    }

    #[test]
    fn open_url_wildcard_among_other_schemes() {
        assert!(
            cap_with_schemes(&["https", "*"])
                .open_url("ftp://example.com")
                .is_ok()
        );
    }

    // ─── open_path permission ────────────────────────────────

    #[test]
    fn open_path_denied_when_not_granted() {
        let err = cap_with_path_permissions(false, true)
            .open_path("/some/path")
            .unwrap_err();
        assert!(matches!(err, OpenerError::PermissionDenied(_)));
    }

    #[test]
    fn open_path_succeeds_when_granted() {
        assert!(
            cap_with_path_permissions(true, true)
                .open_path("/some/path")
                .is_ok()
        );
    }

    // ─── reveal_path permission ──────────────────────────────

    #[test]
    fn reveal_path_denied_when_not_granted() {
        let err = cap_with_path_permissions(true, false)
            .reveal_path("/some/path")
            .unwrap_err();
        assert!(matches!(err, OpenerError::PermissionDenied(_)));
    }

    #[test]
    fn reveal_path_succeeds_when_granted() {
        assert!(
            cap_with_path_permissions(true, true)
                .reveal_path("/some/path")
                .is_ok()
        );
    }

    // ─── Closure delegation ──────────────────────────────────

    #[test]
    fn open_url_delegates_to_closure() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&call_count);
        let cap = OpenerCap::from_closures(
            OpenerPermissions {
                schemes: vec!["https".into()],
                open_path: false,
                reveal_path: false,
            },
            Box::new(move |url| {
                counter.fetch_add(1, Ordering::Relaxed);
                assert_eq!(url, "https://example.com");
                Ok(())
            }),
            Box::new(|_| Ok(())),
            Box::new(|_| Ok(())),
        );
        cap.open_url("https://example.com").expect("should succeed");
        assert_eq!(call_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn open_url_propagates_backend_error() {
        let cap = OpenerCap::from_closures(
            OpenerPermissions {
                schemes: vec!["https".into()],
                open_path: false,
                reveal_path: false,
            },
            Box::new(|_| Err("simulated failure".into())),
            Box::new(|_| Ok(())),
            Box::new(|_| Ok(())),
        );
        let err = cap.open_url("https://example.com").unwrap_err();
        assert!(matches!(err, OpenerError::BackendFailure(_)));
    }

    #[test]
    fn open_path_delegates_to_closure() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&call_count);
        let cap = OpenerCap::from_closures(
            OpenerPermissions {
                schemes: vec![],
                open_path: true,
                reveal_path: false,
            },
            Box::new(|_| Ok(())),
            Box::new(move |path| {
                counter.fetch_add(1, Ordering::Relaxed);
                assert_eq!(path, "/test/path");
                Ok(())
            }),
            Box::new(|_| Ok(())),
        );
        cap.open_path("/test/path").expect("should succeed");
        assert_eq!(call_count.load(Ordering::Relaxed), 1);
    }

    #[test]
    fn reveal_path_delegates_to_closure() {
        let call_count = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&call_count);
        let cap = OpenerCap::from_closures(
            OpenerPermissions {
                schemes: vec![],
                open_path: false,
                reveal_path: true,
            },
            Box::new(|_| Ok(())),
            Box::new(|_| Ok(())),
            Box::new(move |path| {
                counter.fetch_add(1, Ordering::Relaxed);
                assert_eq!(path, "/test/path");
                Ok(())
            }),
        );
        cap.reveal_path("/test/path").expect("should succeed");
        assert_eq!(call_count.load(Ordering::Relaxed), 1);
    }

    // ─── Error display ───────────────────────────────────────

    #[test]
    fn error_display_includes_message() {
        let err = OpenerError::PermissionDenied("test".into());
        assert!(err.to_string().contains("test"));

        let err = OpenerError::InvalidUrl("bad-url".into());
        assert!(err.to_string().contains("bad-url"));

        let err = OpenerError::BackendFailure("oops".into());
        assert!(err.to_string().contains("oops"));
    }
}
