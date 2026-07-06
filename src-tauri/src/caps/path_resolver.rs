// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// PathResolverCap
//
// Wraps an `Arc<dyn PathResolver>` to provide runtime
// `${...}` variable resolution as a capability. WASM gadgets
// access this through the `paths::resolve` host import;
// native gadgets can use it directly if they need variable
// substitution at runtime.
//
// The cap delegates through the `PathResolver` trait — it
// does not know or care about the concrete type behind the
// trait object.
// =========================================================

use std::path::Path;
use std::sync::Arc;

use crate::paths::{PathResolver, ResolveError};

pub struct PathResolverCap {
    resolver: Arc<dyn PathResolver + Send + Sync>,
}

impl PathResolverCap {
    pub fn new(resolver: Arc<dyn PathResolver + Send + Sync>) -> Self {
        Self { resolver }
    }

    pub fn substitute_variables(&self, template: &str) -> Result<String, ResolveError> {
        self.resolver.substitute_variables(template)
    }

    // Delegates alongside `substitute_variables` and `resolve`; this one
    // is exercised only by this module's tests.
    #[allow(dead_code)]
    pub fn is_recognized(&self, name: &str) -> bool {
        self.resolver.is_recognized(name)
    }

    pub fn resolve(&self, name: &str) -> Option<&Path> {
        self.resolver.resolve(name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::{GadgetPaths, PlatformPaths};
    use std::path::PathBuf;

    fn test_gadget_paths() -> GadgetPaths {
        GadgetPaths {
            platform: Arc::new(PlatformPaths {
                home: PathBuf::from("/home/user"),
                xdg_config: PathBuf::from("/home/user/.config"),
                xdg_data: PathBuf::from("/home/user/.local/share"),
            }),
            gadget_data: PathBuf::from("/data/my-gadget"),
            gadget_archive: PathBuf::from("/archive/my-gadget"),
        }
    }

    fn test_cap() -> PathResolverCap {
        PathResolverCap::new(Arc::new(test_gadget_paths()))
    }

    // ─── Delegation through trait object ──────────────────

    #[test]
    fn substitute_resolves_platform_variable() {
        let cap = test_cap();
        let result = cap.substitute_variables("${home}/docs").expect("resolve");
        assert_eq!(result, "/home/user/docs");
    }

    #[test]
    fn substitute_resolves_gadget_variable() {
        let cap = test_cap();
        let result = cap
            .substitute_variables("${gadget-data}/db")
            .expect("resolve");
        assert_eq!(result, "/data/my-gadget/db");
    }

    #[test]
    fn substitute_resolves_multiple_variables() {
        let cap = test_cap();
        let result = cap
            .substitute_variables("${home}/a/${gadget-data}/b")
            .expect("resolve");
        assert_eq!(result, "/home/user/a//data/my-gadget/b");
    }

    #[test]
    fn substitute_passes_through_plain_text() {
        let cap = test_cap();
        let result = cap
            .substitute_variables("/plain/path/no/vars")
            .expect("resolve");
        assert_eq!(result, "/plain/path/no/vars");
    }

    #[test]
    fn substitute_rejects_unknown_variable() {
        let cap = test_cap();
        let err = cap.substitute_variables("${unknown}").unwrap_err();
        assert!(matches!(err, ResolveError::UnknownVariable(_)));
    }

    #[test]
    fn substitute_rejects_unterminated_variable() {
        let cap = test_cap();
        let err = cap.substitute_variables("${home").unwrap_err();
        assert!(matches!(err, ResolveError::Unterminated(_)));
    }

    // ─── is_recognized ────────────────────────────────────

    #[test]
    fn is_recognized_covers_all_variables() {
        let cap = test_cap();
        for name in [
            "home",
            "xdg-config",
            "xdg-data",
            "gadget-data",
            "gadget-archive",
        ] {
            assert!(cap.is_recognized(name), "{name} should be recognized");
        }
    }

    #[test]
    fn is_recognized_rejects_unknown() {
        assert!(!test_cap().is_recognized("unknown"));
    }

    #[test]
    fn is_recognized_rejects_empty_string() {
        assert!(!test_cap().is_recognized(""));
    }

    // ─── resolve ──────────────────────────────────────────

    #[test]
    fn resolve_returns_correct_paths() {
        let cap = test_cap();
        assert_eq!(cap.resolve("home"), Some(Path::new("/home/user")));
        assert_eq!(
            cap.resolve("gadget-data"),
            Some(Path::new("/data/my-gadget"))
        );
    }

    #[test]
    fn resolve_returns_none_for_unknown() {
        assert_eq!(test_cap().resolve("unknown"), None);
    }

    // ─── Trait object behavior ────────────────────────────

    #[test]
    fn works_with_any_path_resolver_impl() {
        struct CustomResolver;
        impl PathResolver for CustomResolver {
            fn is_recognized(&self, name: &str) -> bool {
                name == "custom"
            }
            fn resolve(&self, name: &str) -> Option<&Path> {
                if name == "custom" {
                    Some(Path::new("/custom/path"))
                } else {
                    None
                }
            }
        }

        let cap = PathResolverCap::new(Arc::new(CustomResolver));
        assert!(cap.is_recognized("custom"));
        assert!(!cap.is_recognized("home"));
        assert_eq!(cap.resolve("custom"), Some(Path::new("/custom/path")));
        assert_eq!(cap.resolve("home"), None);
    }
}
