// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Permissions
//
// Plugins opt into host capabilities by declaring the
// relevant sub-table or array under `[permissions]`.
// Omitting a sub-table or array means the capability is
// denied. This mirrors the Android/iOS permission model:
// no capability is implicitly granted.
//
// Origins in `[permissions.http]` are normalized to
// `url::Origin::ascii_serialization()` form at parse time
// (see `validate_permissions`) so runtime checks are plain
// string equality — no re-parsing at call time — and
// variant spellings like `"https://example.com/"`,
// `"HTTPS://example.com"`, and `"https://example.com:443"`
// all map to the same canonical form `"https://example.com"`.
//
// `[[permissions.command]]` rules carry a binary name plus
// a per-position argv constraint vocabulary (literal, enum,
// glob, regex, path-under, any-string, rest). Argv
// constraints validate the *shape* of invocations; they do
// not constrain what a binary does once invoked. See ADR
// 0040 for the full trust-model rationale.
// =========================================================

use serde::{Deserialize, Serialize};

pub(super) mod opener;
pub use opener::OpenerPermissionsDef;

pub(super) mod http;
pub use http::HttpPermissionsDef;

pub(super) mod fs;
pub use fs::FsPermissionsDef;

pub(super) mod command;
pub use command::{ArgvConstraint, CommandPermissionDef};

/// `[permissions]` block.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PermissionsDef {
    /// `[permissions.opener]` — URL-opening + path-handling
    /// capabilities.
    pub opener: Option<OpenerPermissionsDef>,
    /// `[permissions.http]` — HTTP fetch capability.
    pub http: Option<HttpPermissionsDef>,
    /// `[permissions.fs]` — read-only filesystem access via
    /// the `fs::read-file` / `file-exists` / `metadata` host
    /// imports.
    pub fs: Option<FsPermissionsDef>,
    /// `[[permissions.command]]` rules — process-execution
    /// capability with per-rule argv constraints. Empty
    /// vector when no rules are declared (deny by default).
    #[serde(default)]
    pub command: Vec<CommandPermissionDef>,
    /// `permissions.website-metadata` — read/write access to the
    /// host-managed website metadata cache. Boolean rather than a
    /// struct because there is no per-domain allowlist; the cache
    /// is shared host-wide and rate-limiting / disk-pressure
    /// concerns are absorbed by the cache layer itself.
    #[serde(default, rename = "website-metadata")]
    pub website_metadata: bool,
}

// =========================================================
// Dispatcher
// =========================================================

/// Validate and normalize `[permissions]` at manifest parse time.
///
/// HTTP origins are normalized to `ascii_serialization()` form so
/// runtime checks can use plain string equality — no re-parsing.
///
/// Rejects:
/// - `[permissions.opener]` declared without granting any
///   capability (no schemes, both booleans `false`).
/// - `[permissions.http]` with an empty `origins` list.
/// - Any `origins` entry that is not a parseable URL (and not `"*"`).
/// - `[[permissions.command]]` rules whose binary is empty,
///   contains a NUL byte, or whose argv constraints fail
///   shape validation (bad regex, empty enum/glob/path-under,
///   nested `rest`, unknown variable references).
pub(super) fn validate_permissions(permissions: PermissionsDef) -> anyhow::Result<PermissionsDef> {
    let opener = permissions.opener.map(|o| o.validate()).transpose()?;

    let http = permissions.http.map(|h| h.validate()).transpose()?;

    let fs = permissions.fs.map(|f| f.validate()).transpose()?;

    command::validate_rules(&permissions.command)?;

    Ok(PermissionsDef {
        opener,
        http,
        fs,
        command: permissions.command,
        website_metadata: permissions.website_metadata,
    })
}

#[cfg(test)]
mod tests {
    use crate::wasm::manifest::Manifest;
    use crate::wasm::manifest::test_helpers::minimal;

    // =====================================================
    // Permissions: happy paths
    // =====================================================

    #[test]
    fn accept_manifest_without_permissions_section() {
        let m = Manifest::parse(&minimal("")).expect("should parse");
        assert!(m.permissions.is_none());
    }

    #[test]
    fn accept_permissions_section_with_no_sub_tables() {
        let m = Manifest::parse(&minimal("[permissions]")).expect("should parse");
        let p = m.permissions.expect("permissions present");
        assert!(p.opener.is_none());
        assert!(p.http.is_none());
    }

    #[test]
    fn accept_opener_permission_with_schemes() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.opener]
               schemes = ["https", "http"]"#,
        ))
        .expect("should parse");
        let schemes = m
            .permissions
            .expect("permissions")
            .opener
            .expect("opener")
            .schemes;
        assert_eq!(schemes, vec!["https", "http"]);
    }

    #[test]
    fn accept_http_permission_with_specific_origin() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["https://api.example.com"]"#,
        ))
        .expect("should parse");
        let origins = m
            .permissions
            .expect("permissions")
            .http
            .expect("http")
            .origins;
        assert_eq!(origins, vec!["https://api.example.com"]);
    }

    #[test]
    fn accept_http_permission_with_wildcard() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["*"]"#,
        ))
        .expect("should parse");
        let origins = m
            .permissions
            .expect("permissions")
            .http
            .expect("http")
            .origins;
        assert_eq!(origins, vec!["*"]);
    }

    #[test]
    fn accept_opener_and_http_permissions_together() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.opener]
               schemes = ["https"]
               [permissions.http]
               origins = ["https://api.example.com"]"#,
        ))
        .expect("should parse");
        let p = m.permissions.expect("permissions");
        assert!(p.opener.is_some());
        assert!(p.http.is_some());
    }

    // =====================================================
    // Permissions: validation errors
    // =====================================================

    #[test]
    fn reject_opener_permission_with_no_capabilities_granted() {
        let err = Manifest::parse(&minimal(
            r#"[permissions.opener]
               schemes = []"#,
        ))
        .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("permissions.opener") && msg.contains("capability"),
            "error should mention permissions.opener and capability granting: {msg}"
        );
    }

    #[test]
    fn accept_opener_with_only_open_path() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.opener]
               open-path = true"#,
        ))
        .expect("should parse");
        let opener = m.permissions.unwrap().opener.unwrap();
        assert!(opener.schemes.is_empty());
        assert!(opener.open_path);
        assert!(!opener.reveal_path);
    }

    #[test]
    fn accept_opener_with_only_reveal_path() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.opener]
               reveal-path = true"#,
        ))
        .expect("should parse");
        let opener = m.permissions.unwrap().opener.unwrap();
        assert!(opener.reveal_path);
    }

    // =====================================================
    // Permissions: website-metadata flag
    // =====================================================

    #[test]
    fn website_metadata_perm_absent_when_no_permissions_section() {
        let m = Manifest::parse(&minimal("")).expect("parses");
        // No `[permissions]` section → `permissions` is `None`.
        assert!(m.permissions.is_none());
    }

    #[test]
    fn website_metadata_perm_defaults_to_false_when_other_perms_present() {
        let m = Manifest::parse(&minimal(
            r#"[permissions.http]
               origins = ["*"]"#,
        ))
        .expect("parses");
        let perms = m.permissions.expect("permissions section present");
        assert!(!perms.website_metadata);
    }

    #[test]
    fn website_metadata_perm_true_when_set() {
        let m = Manifest::parse(&minimal(
            r#"[permissions]
               website-metadata = true"#,
        ))
        .expect("parses");
        let perms = m.permissions.expect("permissions section present");
        assert!(perms.website_metadata);
    }

    #[test]
    fn website_metadata_perm_explicit_false() {
        let m = Manifest::parse(&minimal(
            r#"[permissions]
               website-metadata = false"#,
        ))
        .expect("parses");
        let perms = m.permissions.expect("permissions section present");
        assert!(!perms.website_metadata);
    }

    #[test]
    fn website_metadata_perm_rejects_non_bool() {
        let err = Manifest::parse(&minimal(
            r#"[permissions]
               website-metadata = "yes""#,
        ))
        .unwrap_err();
        // Confirm parsing fails on a type mismatch rather than
        // silently coercing the string to a bool.
        let msg = err.to_string();
        assert!(
            msg.contains("website-metadata") || msg.contains("bool") || msg.contains("type"),
            "expected type error for non-bool flag, got: {msg}"
        );
    }

    #[test]
    fn website_metadata_perm_coexists_with_http_and_command() {
        let m = Manifest::parse(&minimal(
            r#"[permissions]
               website-metadata = true

               [permissions.http]
               origins = ["https://api.example.com"]

               [[permissions.command]]
               binary = "git""#,
        ))
        .expect("parses");
        let perms = m.permissions.expect("permissions section present");
        assert!(perms.website_metadata);
        assert!(perms.http.is_some());
        assert_eq!(perms.command.len(), 1);
    }
}
