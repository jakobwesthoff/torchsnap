// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};

use crate::paths::{ParseTimeResolver, PathResolver};

/// `[permissions.fs]` — declares read-only filesystem paths
/// the gadget may access via the `fs` host import.
///
/// ```toml
/// [permissions.fs]
/// read = [
///     "${xdg-config}/myapp/config.toml",
///     "/var/lib/myapp/data/*.json",
/// ]
/// ```
///
/// Patterns accept `${...}` substitution tokens (validated
/// via `PathResolver`) and the glob metacharacters `*`
/// (single segment) and `**` (multi-segment).
///
/// An empty `read` list is a manifest authoring error.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FsPermissionsDef {
    /// Patterns the gadget may read from. `${...}` tokens are
    /// preserved verbatim — substitution and glob compilation
    /// happen at bridge construction, when a `PathResolver`
    /// is available.
    pub read: Vec<String>,
}

// ─── Domain conversions ──────────────────────────────────

impl From<FsPermissionsDef> for crate::caps::FilesystemPermissions {
    fn from(def: FsPermissionsDef) -> Self {
        Self {
            read_patterns: def.read,
        }
    }
}

impl From<FsPermissionsDef> for crate::caps::CapRequest {
    fn from(def: FsPermissionsDef) -> Self {
        Self::Filesystem {
            permissions: def.into(),
        }
    }
}

impl FsPermissionsDef {
    /// Reject an empty read list and validate each pattern for
    /// traversal attempts and unknown substitution variables.
    pub(super) fn validate(self) -> anyhow::Result<Self> {
        if self.read.is_empty() {
            anyhow::bail!(
                "`[permissions.fs]` declared with an empty `read` list — \
                 either add at least one path pattern or remove the section"
            );
        }
        for (index, pattern) in self.read.iter().enumerate() {
            validate_fs_pattern(pattern, index)?;
        }
        Ok(self)
    }
}

/// Parse-time syntactic validation of a `[permissions.fs]
/// read = [...]` entry. Covers the checks that depend only on
/// the literal string the user typed: non-empty, no `..`
/// traversal segments, well-formed `${...}` substitution
/// tokens. The unsupported-glob-metacharacter check lives in
/// `host::fs::compile_fs_patterns`, where it runs against the
/// post-substitution pattern — substitution variable braces
/// are then naturally distinguishable from glob braces.
fn validate_fs_pattern(pattern: &str, index: usize) -> anyhow::Result<()> {
    if pattern.is_empty() {
        anyhow::bail!(
            "`[permissions.fs]` read[{index}]: empty pattern; \
             remove the entry or supply a real path"
        );
    }

    for segment in pattern.split('/') {
        if segment == ".." {
            anyhow::bail!(
                "`[permissions.fs]` read[{index}]: pattern `{pattern}` \
                 contains a `..` traversal segment; declare absolute \
                 paths only"
            );
        }
    }

    ParseTimeResolver
        .validate_variable_references(pattern)
        .map_err(|e| anyhow::anyhow!("permissions.fs.read[{index}]: {e}"))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::{CapRequest, FilesystemPermissions};
    use crate::wasm::manifest::Manifest;
    use crate::wasm::manifest::test_helpers::minimal;

    // ─── Domain conversions ─────────────────────────────────

    #[test]
    fn into_filesystem_permissions_maps_read_to_read_patterns() {
        let def = FsPermissionsDef {
            read: vec!["${xdg-config}/*.txt".into(), "/etc/hosts".into()],
        };
        let perms: FilesystemPermissions = def.into();
        assert_eq!(
            perms.read_patterns,
            vec!["${xdg-config}/*.txt", "/etc/hosts"]
        );
    }

    #[test]
    fn into_filesystem_permissions_empty() {
        let def = FsPermissionsDef { read: vec![] };
        let perms: FilesystemPermissions = def.into();
        assert!(perms.read_patterns.is_empty());
    }

    #[test]
    fn into_cap_request_produces_filesystem_variant() {
        let def = FsPermissionsDef {
            read: vec!["/tmp/*.log".into()],
        };
        let req: CapRequest = def.into();
        match req {
            CapRequest::Filesystem { permissions } => {
                assert_eq!(permissions.read_patterns, vec!["/tmp/*.log"]);
            }
            _ => panic!("expected Filesystem variant"),
        }
    }

    // =====================================================
    // Permissions: fs
    // =====================================================

    #[test]
    fn fs_pattern_with_substitution_token_passes_metachar_check() {
        // Regression: the `${xdg-config}` token's literal `{`
        // and `}` must not trip the unsupported-glob-metachar
        // scan. The check should ignore characters inside
        // `${...}` substitutions.
        let m = Manifest::parse(&minimal(
            r#"[permissions.fs]
               read = ["${xdg-config}/ZeroTier/One/authtoken.secret"]"#,
        ))
        .expect("should parse");
        let fs = m.permissions.unwrap().fs.unwrap();
        assert_eq!(fs.read.len(), 1);
    }

    #[test]
    fn fs_pattern_rejects_empty_read_list() {
        let err = Manifest::parse(&minimal(
            r#"[permissions.fs]
               read = []"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("empty `read` list"));
    }

    #[test]
    fn fs_pattern_rejects_traversal() {
        let err = Manifest::parse(&minimal(
            r#"[permissions.fs]
               read = ["/etc/../etc/hosts"]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("traversal segment"));
    }

    #[test]
    fn fs_pattern_rejects_unknown_substitution_variable() {
        let err = Manifest::parse(&minimal(
            r#"[permissions.fs]
               read = ["${nope}/foo"]"#,
        ))
        .unwrap_err();
        assert!(err.to_string().contains("unknown variable"));
    }

    #[test]
    fn fs_pattern_accepts_glob_metachars_star_and_doublestar() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.fs]
               read = [
                   "${xdg-config}/myapp/*.toml",
                   "${xdg-data}/myapp/**/*.json",
               ]"#,
        ))
        .expect("should parse");
        let fs = m.permissions.unwrap().fs.unwrap();
        assert_eq!(fs.read.len(), 2);
    }
}
