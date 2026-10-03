// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use serde::{Deserialize, Serialize};

/// `[permissions.http]` — declares which origins the gadget
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
/// special value `"*"` opts the gadget into trust-all mode.
///
/// An empty `origins` list is a manifest authoring error.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct HttpPermissionsDef {
    /// Stored as normalized `ascii_serialization()` origins,
    /// except for the literal `"*"` which is preserved as-is.
    pub origins: Vec<String>,
}

// ─── Domain conversions ──────────────────────────────────

impl From<HttpPermissionsDef> for crate::caps::HttpPermissions {
    fn from(def: HttpPermissionsDef) -> Self {
        Self {
            origins: def.origins,
        }
    }
}

impl From<HttpPermissionsDef> for crate::caps::CapRequest {
    fn from(def: HttpPermissionsDef) -> Self {
        Self::Http {
            permissions: def.into(),
        }
    }
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
        // errors at gadget-load time rather than at the first fetch.
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
                // An opaque origin (`file:`, `data:`, ...) serializes as
                // `null`, and a path would drop silently. Grants are per
                // scheme, host and port only.
                if !parsed.origin().is_tuple() {
                    anyhow::bail!(
                        "`[permissions.http]` origin `{origin}` has no scheme and host; \
                         write an origin such as `https://api.example.com`"
                    );
                }
                if parsed.path() != "/" || parsed.query().is_some() || parsed.fragment().is_some() {
                    anyhow::bail!(
                        "`[permissions.http]` origin `{origin}` has a path, query or fragment; \
                         an origin grants its whole host, so write only scheme, host and port"
                    );
                }
                Ok(parsed.origin().ascii_serialization())
            })
            .collect::<anyhow::Result<Vec<_>>>()?;
        self.origins = origins;
        Ok(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caps::{CapRequest, HttpPermissions};
    use crate::wasm::manifest::Manifest;
    use crate::wasm::manifest::test_helpers::minimal;

    // ─── Domain conversions ─────────────────────────────────

    #[test]
    fn into_http_permissions_maps_origins() {
        let def = HttpPermissionsDef {
            origins: vec!["https://example.com".into(), "https://other.com".into()],
        };
        let perms: HttpPermissions = def.into();
        assert_eq!(
            perms.origins,
            vec!["https://example.com", "https://other.com"]
        );
    }

    #[test]
    fn into_http_permissions_wildcard() {
        let def = HttpPermissionsDef {
            origins: vec!["*".into()],
        };
        let perms: HttpPermissions = def.into();
        assert_eq!(perms.origins, vec!["*"]);
    }

    #[test]
    fn into_cap_request_produces_http_variant() {
        let def = HttpPermissionsDef {
            origins: vec!["https://api.example.com".into()],
        };
        let req: CapRequest = def.into();
        match req {
            CapRequest::Http { permissions } => {
                assert_eq!(permissions.origins, vec!["https://api.example.com"]);
            }
            _ => panic!("expected Http variant"),
        }
    }

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

    fn parse_origins(origins: &str) -> anyhow::Result<Vec<String>> {
        let m = Manifest::parse(&minimal(&format!(
            "[permissions.http]\norigins = {origins}"
        )))?;
        Ok(m.permissions.unwrap().http.unwrap().origins)
    }

    // A non-tuple scheme has an opaque origin, which serializes as
    // `null`. Storing that would keep a grant nobody wrote.
    #[test]
    fn reject_origin_without_host() {
        for origin in [
            "file:///etc",
            "data:text/plain,hi",
            "mailto:jane@example.com",
        ] {
            let err = parse_origins(&format!(r#"["https://example.com", "{origin}"]"#))
                .expect_err(origin);
            let msg = err.to_string();
            assert!(msg.contains(origin), "{origin}: {msg}");
            assert!(msg.contains("scheme and host"), "{origin}: {msg}");
        }
    }

    // Grants are per origin. A path would read as narrowing the grant
    // while the whole host is granted.
    #[test]
    fn reject_origin_with_path_query_or_fragment() {
        for origin in [
            "https://api.example.com/v1/only",
            "https://api.example.com/?key=1",
            "https://api.example.com/#top",
        ] {
            let err = parse_origins(&format!(r#"["{origin}"]"#)).expect_err(origin);
            let msg = err.to_string();
            assert!(msg.contains(origin), "{origin}: {msg}");
            assert!(msg.contains("path, query or fragment"), "{origin}: {msg}");
        }
    }

    #[test]
    fn accept_origin_with_port_and_loopback() {
        assert_eq!(
            parse_origins(r#"["http://127.0.0.1:9993", "https://example.com/"]"#).unwrap(),
            vec!["http://127.0.0.1:9993", "https://example.com"]
        );
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
