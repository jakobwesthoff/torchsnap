// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// PathResolverCap
//
// Wraps a GadgetPaths instance to provide runtime `${...}`
// variable resolution as a capability. WASM gadgets access
// this through the `paths::resolve` host import; native
// gadgets can use it directly if they need variable
// substitution at runtime.
// =========================================================

use std::path::Path;

use crate::paths::{GadgetPaths, PathResolver, ResolveError};

pub struct PathResolverCap {
    paths: GadgetPaths,
}

impl PathResolverCap {
    pub fn new(paths: GadgetPaths) -> Self {
        Self { paths }
    }

    pub fn substitute_variables(&self, template: &str) -> Result<String, ResolveError> {
        self.paths.substitute_variables(template)
    }

    pub fn is_recognized(&self, name: &str) -> bool {
        self.paths.is_recognized(name)
    }

    pub fn resolve(&self, name: &str) -> Option<&Path> {
        self.paths.resolve(name)
    }

    pub fn paths(&self) -> &GadgetPaths {
        &self.paths
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::PlatformPaths;
    use std::path::PathBuf;
    use std::sync::Arc;

    fn test_cap() -> PathResolverCap {
        PathResolverCap::new(GadgetPaths {
            platform: Arc::new(PlatformPaths {
                home: PathBuf::from("/home/user"),
                xdg_config: PathBuf::from("/home/user/.config"),
                xdg_data: PathBuf::from("/home/user/.local/share"),
            }),
            gadget_data: PathBuf::from("/data/my-gadget"),
            gadget_archive: PathBuf::from("/archive/my-gadget"),
        })
    }

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
    fn substitute_rejects_unknown_variable() {
        let cap = test_cap();
        let err = cap.substitute_variables("${unknown}").unwrap_err();
        assert!(matches!(err, ResolveError::UnknownVariable(_)));
    }

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
    fn resolve_returns_correct_paths() {
        let cap = test_cap();
        assert_eq!(cap.resolve("home"), Some(Path::new("/home/user")));
        assert_eq!(
            cap.resolve("gadget-data"),
            Some(Path::new("/data/my-gadget"))
        );
        assert_eq!(cap.resolve("unknown"), None);
    }

    #[test]
    fn paths_accessor_returns_inner() {
        let cap = test_cap();
        assert_eq!(cap.paths().gadget_data, PathBuf::from("/data/my-gadget"));
    }
}
