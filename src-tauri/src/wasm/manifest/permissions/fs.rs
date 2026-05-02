// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};

use crate::wasm::permission_vars::validate_variable_references;

/// `[permissions.fs]` — declares read-only filesystem paths
/// the plugin may access via the `fs` host import.
///
/// ```toml
/// [permissions.fs]
/// read = [
///     "${xdg-config}/myapp/config.toml",
///     "/var/lib/myapp/data/*.json",
/// ]
/// ```
///
/// Patterns accept `${...}` substitution tokens from
/// [`RECOGNIZED_PERMISSION_VARIABLES`] and the glob
/// metacharacters `*` (single segment) and `**` (multi-segment).
///
/// An empty `read` list is a manifest authoring error.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct FsPermissionsDef {
    /// Patterns the plugin may read from. `${...}` tokens are
    /// preserved verbatim — substitution and glob compilation
    /// happen at bridge construction, when the per-instance
    /// `PathContext` is available.
    pub read: Vec<String>,
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

    validate_variable_references(pattern, &format!("permissions.fs.read[{index}]"), index)?;

    Ok(())
}
