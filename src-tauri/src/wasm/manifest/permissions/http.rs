// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};

/// `[permissions.http]` — declares which origins the plugin
/// is allowed to reach via `http::fetch`.
///
/// ```toml
/// [permissions.http]
/// origins = ["https://api.example.com"]
///
/// # or trust-all:
/// origins = ["*"]
/// ```
///
/// Origins must be valid `scheme + host` pairs
/// (e.g. `"https://api.example.com"`). They are normalized
/// to `ascii_serialization()` form at parse time. The
/// special value `"*"` opts the plugin into trust-all mode.
///
/// An empty `origins` list is a manifest authoring error.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpPermissionsDef {
    /// Stored as normalized `ascii_serialization()` origins,
    /// except for the literal `"*"` which is preserved as-is.
    pub origins: Vec<String>,
}

impl HttpPermissionsDef {
    /// Reject an empty origins list; normalize all other
    /// entries to `ascii_serialization()` form so runtime
    /// checks are plain string equality.
    pub(super) fn validate(mut self) -> anyhow::Result<Self> {
        if self.origins.is_empty() {
            anyhow::bail!(
                "`[permissions.http]` declared with an empty `origins` list — \
                 either add at least one origin (or `\"*\"`) or remove the section"
            );
        }

        // Normalize each declared origin to ascii_serialization().
        // Reject malformed entries immediately so authors discover
        // errors at plugin-load time rather than at the first fetch.
        let origins = self
            .origins
            .into_iter()
            .map(|origin| {
                if origin == "*" {
                    return Ok(origin);
                }
                let parsed = url::Url::parse(&origin).map_err(|e| {
                    anyhow::anyhow!(
                        "`[permissions.http]` origin `{origin}` is not a valid URL: {e}"
                    )
                })?;
                Ok(parsed.origin().ascii_serialization())
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        self.origins = origins;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use crate::wasm::manifest::Manifest;
    use crate::wasm::manifest::test_helpers::minimal;

    // =====================================================
    // Permissions: origin normalization
    // =====================================================

    #[test]
    fn normalize_trailing_slash_in_origin() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["https://example.com/"]"#,
        ))
        .expect("should parse");
        let origins = m.permissions.unwrap().http.unwrap().origins;
        assert_eq!(origins[0], "https://example.com");
    }

    #[test]
    fn normalize_uppercase_scheme_in_origin() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["HTTPS://example.com"]"#,
        ))
        .expect("should parse");
        let origins = m.permissions.unwrap().http.unwrap().origins;
        assert_eq!(origins[0], "https://example.com");
    }

    #[test]
    fn normalize_default_port_in_origin() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["https://example.com:443"]"#,
        ))
        .expect("should parse");
        let origins = m.permissions.unwrap().http.unwrap().origins;
        assert_eq!(origins[0], "https://example.com");
    }

    #[test]
    fn preserve_non_default_port_in_origin() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["https://example.com:8443"]"#,
        ))
        .expect("should parse");
        let origins = m.permissions.unwrap().http.unwrap().origins;
        assert_eq!(origins[0], "https://example.com:8443");
    }

    #[test]
    fn preserve_wildcard_as_is() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["*"]"#,
        ))
        .expect("should parse");
        let origins = m.permissions.unwrap().http.unwrap().origins;
        assert_eq!(origins[0], "*");
    }

    #[test]
    fn reject_http_permission_with_empty_origins() {
        let err = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = []"#,
        ))
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("permissions.http") && msg.contains("origins"),
            "error should mention both fields: {msg}"
        );
    }

    #[test]
    fn reject_malformed_origin() {
        let err = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["not-a-url"]"#,
        ))
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("not-a-url"),
            "error should mention the offending value: {msg}"
        );
    }
}
