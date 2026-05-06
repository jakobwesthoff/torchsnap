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
