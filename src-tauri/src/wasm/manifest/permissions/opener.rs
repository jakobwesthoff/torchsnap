// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};

/// `[permissions.opener]` — declares the gadget's
/// `opener` capabilities: which URL schemes it may pass to
/// `open-url`, and whether it may invoke `open-path` /
/// `reveal-path`.
///
/// ```toml
/// [permissions.opener]
/// schemes      = ["https", "http"]
/// open-path    = true
/// reveal-path  = true
/// ```
///
/// At least one capability must be granted: an
/// `[permissions.opener]` block with an empty `schemes`
/// list and both booleans `false` is a manifest authoring
/// error (the section is declared without granting
/// anything).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenerPermissionsDef {
    /// URL schemes accepted by `opener::open-url`.
    #[serde(default)]
    pub schemes: Vec<String>,

    /// Whether the gadget may invoke `opener::open-path`
    /// (open a filesystem path with the registered
    /// application).
    #[serde(default, rename = "open-path")]
    pub open_path: bool,

    /// Whether the gadget may invoke `opener::reveal-path`
    /// (reveal a filesystem path in the OS file manager).
    #[serde(default, rename = "reveal-path")]
    pub reveal_path: bool,
}

// ─── Domain conversions ──────────────────────────────────

impl From<OpenerPermissionsDef> for crate::caps::OpenerPermissions {
    fn from(def: OpenerPermissionsDef) -> Self {
        Self {
            schemes: def.schemes,
            open_path: def.open_path,
            reveal_path: def.reveal_path,
        }
    }
}

impl From<OpenerPermissionsDef> for crate::caps::CapRequest {
    fn from(def: OpenerPermissionsDef) -> Self {
        Self::Opener {
            permissions: def.into(),
        }
    }
}

impl OpenerPermissionsDef {
    /// Reject an opener section declared without granting any
    /// capability (no schemes, both booleans `false`).
    pub(super) fn validate(self) -> anyhow::Result<Self> {
        let any_capability = !self.schemes.is_empty() || self.open_path || self.reveal_path;
        if !any_capability {
            anyhow::bail!(
                "`[permissions.opener]` declared without granting any capability — \
                 add at least one scheme or set `open-path` / `reveal-path` to `true`"
            );
        }
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::{CapRequest, OpenerPermissions};

    #[test]
    fn into_opener_permissions_maps_all_fields() {
        let def = OpenerPermissionsDef {
            schemes: vec!["https".into(), "http".into()],
            open_path: true,
            reveal_path: false,
        };
        let perms: OpenerPermissions = def.into();
        assert_eq!(perms.schemes, vec!["https", "http"]);
        assert!(perms.open_path);
        assert!(!perms.reveal_path);
    }

    #[test]
    fn into_opener_permissions_empty_schemes() {
        let def = OpenerPermissionsDef {
            schemes: vec![],
            open_path: false,
            reveal_path: true,
        };
        let perms: OpenerPermissions = def.into();
        assert!(perms.schemes.is_empty());
        assert!(perms.reveal_path);
    }

    #[test]
    fn into_cap_request_produces_opener_variant() {
        let def = OpenerPermissionsDef {
            schemes: vec!["https".into()],
            open_path: true,
            reveal_path: false,
        };
        let req: CapRequest = def.into();
        match req {
            CapRequest::Opener { permissions } => {
                assert_eq!(permissions.schemes, vec!["https"]);
                assert!(permissions.open_path);
            }
            _ => panic!("expected Opener variant"),
        }
    }
}
