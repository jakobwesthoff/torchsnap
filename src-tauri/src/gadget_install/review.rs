// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! What the user sees before confirming an install.
//!
//! The review lists every permission the incoming gadget declares, one
//! item per grant (each HTTP origin, each filesystem pattern, each
//! command rule and so on), with a severity for highlighting. On a
//! replace, each item also says whether the installed version already
//! had it, and grants the new version drops are listed separately.
//! Wording is left to the frontend; this module fixes the structure.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use serde::Serialize;

use super::decision::{InstallDecision, VersionRelation};
use super::provenance::Provenance;
use crate::wasm::manifest::{ArgvConstraint, Manifest};
use crate::wasm::source::GadgetSource;

// =========================================================
// Permissions
// =========================================================

/// A single grant a gadget asks for. Two permissions are the same
/// grant exactly when they compare equal, which is what change
/// detection between versions relies on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Permission {
    /// Run `binary` with arguments matching `argv`. Per-rule limits
    /// (`cwd`, timeouts, output and stdin caps) are parsed from the
    /// manifest but not enforced, so they are deliberately left out:
    /// see `todos/gadget-host/caps/01kwg1ajrvfsmxyjmm7bvcs04b-command-per-rule-limits-unenforced.md`.
    Command {
        binary: String,
        argv: Vec<ArgvRule>,
    },
    HttpOrigin {
        origin: String,
    },
    OpenerOpenPath,
    OpenerScheme {
        scheme: String,
    },
    OpenerRevealPath,
    FilesystemRead {
        pattern: String,
    },
    Clipboard,
    WebsiteMetadata,
    Settings,
    Frecency,
    SqlStorage,
    IconCache,
    PathResolver,
}

/// Mirror of `ArgvConstraint` with the review's own serialization,
/// so the frontend contract does not follow the manifest format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ArgvRule {
    Literal { value: String },
    Enum { values: Vec<String> },
    Glob { pattern: String },
    Regex { pattern: String },
    PathUnder { root: String },
    AnyString,
    Rest { constraint: Box<ArgvRule> },
}

impl From<&ArgvConstraint> for ArgvRule {
    fn from(constraint: &ArgvConstraint) -> Self {
        match constraint {
            ArgvConstraint::Literal { value } => ArgvRule::Literal {
                value: value.clone(),
            },
            ArgvConstraint::Enum { values } => ArgvRule::Enum {
                values: values.clone(),
            },
            ArgvConstraint::Glob { pattern } => ArgvRule::Glob {
                pattern: pattern.clone(),
            },
            ArgvConstraint::Regex { pattern } => ArgvRule::Regex {
                pattern: pattern.clone(),
            },
            ArgvConstraint::PathUnder { root } => ArgvRule::PathUnder { root: root.clone() },
            ArgvConstraint::AnyString => ArgvRule::AnyString,
            ArgvConstraint::Rest { constraint } => ArgvRule::Rest {
                constraint: Box::new(ArgvRule::from(constraint.as_ref())),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Info,
    Notice,
    Warning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Change {
    Added,
    Unchanged,
    Removed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionItem {
    pub permission: Permission,
    pub severity: Severity,
    pub change: Change,
}

/// Every grant in `manifest`, in display order: the broad ones
/// (processes, network, opening paths) first.
fn permissions_of(manifest: &Manifest) -> Vec<Permission> {
    let Some(declared) = manifest.permissions.as_ref() else {
        return Vec::new();
    };
    let mut permissions = Vec::new();

    for rule in &declared.command {
        permissions.push(Permission::Command {
            binary: rule.binary.clone(),
            argv: rule.argv.iter().map(ArgvRule::from).collect(),
        });
    }
    if let Some(http) = &declared.http {
        for origin in &http.origins {
            permissions.push(Permission::HttpOrigin {
                origin: origin.clone(),
            });
        }
    }
    if let Some(opener) = &declared.opener {
        if opener.open_path {
            permissions.push(Permission::OpenerOpenPath);
        }
        for scheme in &opener.schemes {
            permissions.push(Permission::OpenerScheme {
                scheme: scheme.clone(),
            });
        }
        if opener.reveal_path {
            permissions.push(Permission::OpenerRevealPath);
        }
    }
    if let Some(filesystem) = &declared.filesystem {
        for pattern in &filesystem.read {
            permissions.push(Permission::FilesystemRead {
                pattern: pattern.clone(),
            });
        }
    }

    let flags = [
        (declared.clipboard, Permission::Clipboard),
        (declared.website_metadata, Permission::WebsiteMetadata),
        (declared.settings, Permission::Settings),
        (declared.frecency, Permission::Frecency),
        (declared.sql_storage, Permission::SqlStorage),
        (declared.icon_cache, Permission::IconCache),
        (declared.path_resolver, Permission::PathResolver),
    ];
    permissions.extend(
        flags
            .into_iter()
            .filter_map(|(granted, permission)| granted.then_some(permission)),
    );

    permissions
}

/// How strongly a grant is highlighted. Spawning processes, talking to
/// any HTTP origin and opening arbitrary paths reach furthest outside
/// the sandbox; data the gadget only keeps for itself is informational.
pub fn severity(permission: &Permission) -> Severity {
    match permission {
        Permission::Command { .. } | Permission::OpenerOpenPath => Severity::Warning,
        Permission::HttpOrigin { origin } if origin == "*" => Severity::Warning,
        Permission::HttpOrigin { .. }
        | Permission::OpenerScheme { .. }
        | Permission::OpenerRevealPath
        | Permission::FilesystemRead { .. }
        | Permission::Clipboard
        | Permission::WebsiteMetadata => Severity::Notice,
        Permission::Settings
        | Permission::Frecency
        | Permission::SqlStorage
        | Permission::IconCache
        | Permission::PathResolver => Severity::Info,
    }
}

fn item(permission: Permission, change: Change) -> PermissionItem {
    PermissionItem {
        severity: severity(&permission),
        permission,
        change,
    }
}

/// The incoming gadget's permissions, marked against `previous` when
/// it replaces another version, and the permissions `previous` had
/// that the incoming version drops. Without `previous` every item is
/// `Added`.
pub fn permission_items(
    incoming: &Manifest,
    previous: Option<&Manifest>,
) -> (Vec<PermissionItem>, Vec<PermissionItem>) {
    let incoming_permissions = permissions_of(incoming);
    let previous_permissions = previous.map(permissions_of).unwrap_or_default();

    let items = incoming_permissions
        .iter()
        .map(|permission| {
            let change = if previous_permissions.contains(permission) {
                Change::Unchanged
            } else {
                Change::Added
            };
            item(permission.clone(), change)
        })
        .collect();
    let removed = previous_permissions
        .into_iter()
        .filter(|permission| !incoming_permissions.contains(permission))
        .map(|permission| item(permission, Change::Removed))
        .collect();

    (items, removed)
}

/// Permissions of every loaded WASM gadget, for the gadget cards in
/// the settings. Nothing is being compared there, so every item is
/// `Unchanged`.
pub fn installed_gadget_permissions(
    sources: &HashMap<String, Arc<dyn GadgetSource>>,
) -> HashMap<String, Vec<PermissionItem>> {
    sources
        .iter()
        .map(|(id, source)| {
            let items = permissions_of(source.manifest())
                .into_iter()
                .map(|permission| item(permission, Change::Unchanged))
                .collect();
            (id.clone(), items)
        })
        .collect()
}

// =========================================================
// The review
// =========================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GadgetSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ReviewAction {
    Install,
    #[serde(rename_all = "camelCase")]
    Replace {
        previous_version: String,
        relation: VersionRelation,
    },
    Reject {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallReview {
    pub gadget: GadgetSummary,
    pub source_path: String,
    pub provenance: Option<Provenance>,
    pub action: ReviewAction,
    pub permissions: Vec<PermissionItem>,
    pub removed_permissions: Vec<PermissionItem>,
}

pub fn build_review(
    incoming: &Manifest,
    source_path: &Path,
    provenance: Option<Provenance>,
    decision: &InstallDecision,
) -> InstallReview {
    let (action, previous) = match decision {
        InstallDecision::Fresh => (ReviewAction::Install, None),
        InstallDecision::Replace { previous, relation } => (
            ReviewAction::Replace {
                previous_version: previous.gadget.version.clone(),
                relation: *relation,
            },
            Some(previous.as_ref()),
        ),
        InstallDecision::Reject(reason) => (
            ReviewAction::Reject {
                reason: reason.clone(),
            },
            None,
        ),
    };
    let (permissions, removed_permissions) = permission_items(incoming, previous);

    InstallReview {
        gadget: GadgetSummary {
            id: incoming.gadget.id.as_str().to_string(),
            name: incoming.gadget.name.clone(),
            description: incoming.gadget.description.clone(),
            version: incoming.gadget.version.clone(),
        },
        source_path: source_path.display().to_string(),
        provenance,
        action,
        permissions,
        removed_permissions,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::path::Path;
    use std::sync::Arc;

    use serde_json::json;

    use super::*;
    use crate::gadget_install::decision::{InstallDecision, VersionRelation};
    use crate::wasm::manifest::Manifest;
    use crate::wasm::manifest::test_helpers::archive_with_manifest;
    use crate::wasm::source::{ArchiveSource, GadgetSource};

    fn manifest(version: &str, extra: &str) -> Manifest {
        Manifest::parse(&format!(
            r#"
            [gadget]
            id = "weather"
            name = "Weather"
            description = "Forecasts"
            version = "{version}"
            wasm = "gadget.wasm"
            icon = "heroicons:beaker"
            {extra}
            "#
        ))
        .expect("test manifest parses")
    }

    fn kinds(items: &[PermissionItem]) -> Vec<&Permission> {
        items.iter().map(|item| &item.permission).collect()
    }

    const EVERYTHING: &str = r#"
        [storage.sql]
        migrations = ["migrations/001.sql"]

        [permissions]
        website-metadata = true
        settings = true
        frecency = true
        sql-storage = true
        clipboard = true
        icon-cache = true
        path-resolver = true

        [permissions.opener]
        schemes = ["https", "mailto"]
        open-path = true
        reveal-path = true

        [permissions.http]
        origins = ["https://api.example.com", "https://cdn.example.com"]

        [permissions.filesystem]
        read = ["${home}/.config/weather/*"]

        [[permissions.command]]
        binary = "/usr/bin/mdfind"
        argv = [{ kind = "literal", value = "-name" }, { kind = "any-string" }]
        timeout-ms-max = 1000
    "#;

    // =========================================================
    // Permission items
    // =========================================================

    #[test]
    fn a_manifest_without_permissions_has_no_items() {
        let (items, removed) = permission_items(&manifest("1.0.0", ""), None);
        assert!(items.is_empty());
        assert!(removed.is_empty());
    }

    #[test]
    fn every_permission_becomes_its_own_item() {
        let (items, _) = permission_items(&manifest("1.0.0", EVERYTHING), None);

        assert_eq!(
            kinds(&items),
            vec![
                &Permission::Command {
                    binary: "/usr/bin/mdfind".to_string(),
                    argv: vec![
                        ArgvRule::Literal {
                            value: "-name".to_string()
                        },
                        ArgvRule::AnyString,
                    ],
                },
                &Permission::HttpOrigin {
                    origin: "https://api.example.com".to_string()
                },
                &Permission::HttpOrigin {
                    origin: "https://cdn.example.com".to_string()
                },
                &Permission::OpenerOpenPath,
                &Permission::OpenerScheme {
                    scheme: "https".to_string()
                },
                &Permission::OpenerScheme {
                    scheme: "mailto".to_string()
                },
                &Permission::OpenerRevealPath,
                &Permission::FilesystemRead {
                    pattern: "${home}/.config/weather/*".to_string()
                },
                &Permission::Clipboard,
                &Permission::WebsiteMetadata,
                &Permission::Settings,
                &Permission::Frecency,
                &Permission::SqlStorage,
                &Permission::IconCache,
                &Permission::PathResolver,
            ]
        );
    }

    #[test]
    fn argv_constraints_map_one_to_one() {
        let (items, _) = permission_items(
            &manifest(
                "1.0.0",
                r#"
                [[permissions.command]]
                binary = "git"
                argv = [
                    { kind = "enum", values = ["status", "log"] },
                    { kind = "glob", pattern = "*.md" },
                    { kind = "regex", pattern = "^[a-f0-9]+$" },
                    { kind = "path-under", root = "${home}" },
                    { kind = "rest", constraint = { kind = "any-string" } },
                ]
                "#,
            ),
            None,
        );

        assert_eq!(
            kinds(&items),
            vec![&Permission::Command {
                binary: "git".to_string(),
                argv: vec![
                    ArgvRule::Enum {
                        values: vec!["status".to_string(), "log".to_string()]
                    },
                    ArgvRule::Glob {
                        pattern: "*.md".to_string()
                    },
                    ArgvRule::Regex {
                        pattern: "^[a-f0-9]+$".to_string()
                    },
                    ArgvRule::PathUnder {
                        root: "${home}".to_string()
                    },
                    ArgvRule::Rest {
                        constraint: Box::new(ArgvRule::AnyString)
                    },
                ],
            }]
        );
    }

    // =========================================================
    // Severity
    // =========================================================

    #[test]
    fn severity_follows_the_documented_rules() {
        let cases = [
            (
                Permission::Command {
                    binary: "ls".to_string(),
                    argv: vec![],
                },
                Severity::Warning,
            ),
            (
                Permission::HttpOrigin {
                    origin: "*".to_string(),
                },
                Severity::Warning,
            ),
            (
                Permission::HttpOrigin {
                    origin: "https://api.example.com".to_string(),
                },
                Severity::Notice,
            ),
            (Permission::OpenerOpenPath, Severity::Warning),
            (
                Permission::OpenerScheme {
                    scheme: "https".to_string(),
                },
                Severity::Notice,
            ),
            (Permission::OpenerRevealPath, Severity::Notice),
            (
                Permission::FilesystemRead {
                    pattern: "/tmp/*".to_string(),
                },
                Severity::Notice,
            ),
            (Permission::Clipboard, Severity::Notice),
            (Permission::WebsiteMetadata, Severity::Notice),
            (Permission::Settings, Severity::Info),
            (Permission::Frecency, Severity::Info),
            (Permission::SqlStorage, Severity::Info),
            (Permission::IconCache, Severity::Info),
            (Permission::PathResolver, Severity::Info),
        ];
        for (permission, expected) in cases {
            assert_eq!(severity(&permission), expected, "{permission:?}");
        }
    }

    // =========================================================
    // Changes against the replaced version
    // =========================================================

    #[test]
    fn a_fresh_install_marks_every_item_added() {
        let (items, removed) = permission_items(&manifest("1.0.0", EVERYTHING), None);

        assert!(items.iter().all(|item| item.change == Change::Added));
        assert!(removed.is_empty());
    }

    #[test]
    fn a_replace_marks_unchanged_added_and_removed_items() {
        let previous = manifest(
            "1.0.0",
            r#"
            [permissions]
            clipboard = true
            [permissions.http]
            origins = ["https://old.example.com", "https://api.example.com"]
            "#,
        );
        let incoming = manifest(
            "2.0.0",
            r#"
            [permissions]
            clipboard = true
            [permissions.http]
            origins = ["https://api.example.com", "*"]
            "#,
        );

        let (items, removed) = permission_items(&incoming, Some(&previous));

        let changes: Vec<_> = items
            .iter()
            .map(|item| (&item.permission, item.change))
            .collect();
        assert_eq!(
            changes,
            vec![
                (
                    &Permission::HttpOrigin {
                        origin: "https://api.example.com".to_string()
                    },
                    Change::Unchanged
                ),
                (
                    &Permission::HttpOrigin {
                        origin: "*".to_string()
                    },
                    Change::Added
                ),
                (&Permission::Clipboard, Change::Unchanged),
            ]
        );
        assert_eq!(
            kinds(&removed),
            vec![&Permission::HttpOrigin {
                origin: "https://old.example.com".to_string()
            }]
        );
        assert!(removed.iter().all(|item| item.change == Change::Removed));
    }

    /// A rule for the same binary with a different argv is a different
    /// grant: the old one goes, the new one comes.
    #[test]
    fn a_changed_argv_on_the_same_binary_is_one_removal_and_one_addition() {
        let previous = manifest(
            "1.0.0",
            "[[permissions.command]]\nbinary = \"git\"\nargv = [{ kind = \"literal\", value = \"status\" }]\n",
        );
        let incoming = manifest(
            "2.0.0",
            "[[permissions.command]]\nbinary = \"git\"\nargv = [{ kind = \"any-string\" }]\n",
        );

        let (items, removed) = permission_items(&incoming, Some(&previous));

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].change, Change::Added);
        assert_eq!(removed.len(), 1);
    }

    /// Per-rule command limits are parsed but not enforced, so the
    /// review must not present them as guarantees.
    #[test]
    fn command_items_carry_no_per_rule_limits() {
        let (items, _) = permission_items(&manifest("1.0.0", EVERYTHING), None);
        let command = serde_json::to_value(&items[0]).expect("item serializes");

        assert!(command.to_string().contains("mdfind"));
        assert!(!command.to_string().contains("1000"));
        assert!(!command.to_string().to_lowercase().contains("timeout"));
    }

    // =========================================================
    // The review
    // =========================================================

    #[test]
    fn a_replace_review_carries_both_versions_and_their_relation() {
        let previous = manifest("1.0.0", "");
        let incoming = manifest("0.9.0", "");
        let decision = InstallDecision::Replace {
            previous: Box::new(previous),
            relation: VersionRelation::Downgrade,
        };

        let review = build_review(
            &incoming,
            Path::new("/downloads/weather.torchsnap"),
            None,
            &decision,
        );

        assert_eq!(
            review.action,
            ReviewAction::Replace {
                previous_version: "1.0.0".to_string(),
                relation: VersionRelation::Downgrade,
            }
        );
    }

    #[test]
    fn a_rejected_review_carries_the_reason() {
        let decision = InstallDecision::Reject("Built-in gadgets cannot be replaced.".to_string());

        let review = build_review(
            &manifest("1.0.0", ""),
            Path::new("/x.torchsnap"),
            None,
            &decision,
        );

        assert_eq!(
            review.action,
            ReviewAction::Reject {
                reason: "Built-in gadgets cannot be replaced.".to_string()
            }
        );
    }

    /// Pins the JSON shape the frontend mirrors in TypeScript.
    #[test]
    fn a_review_serializes_to_the_frontend_contract() {
        let previous = manifest("1.0.0", "[permissions]\nclipboard = true\n");
        let incoming = manifest(
            "1.1.0",
            "[permissions]\nsettings = true\n[permissions.http]\norigins = [\"*\"]\n",
        );
        let decision = InstallDecision::Replace {
            previous: Box::new(previous),
            relation: VersionRelation::Upgrade,
        };

        let review = build_review(
            &incoming,
            Path::new("/downloads/weather.torchsnap"),
            None,
            &decision,
        );

        assert_eq!(
            serde_json::to_value(&review).expect("review serializes"),
            json!({
                "gadget": {
                    "id": "weather",
                    "name": "Weather",
                    "description": "Forecasts",
                    "version": "1.1.0"
                },
                "sourcePath": "/downloads/weather.torchsnap",
                "provenance": null,
                "action": {
                    "kind": "replace",
                    "previousVersion": "1.0.0",
                    "relation": "upgrade"
                },
                "permissions": [
                    {
                        "permission": { "kind": "httpOrigin", "origin": "*" },
                        "severity": "warning",
                        "change": "added"
                    },
                    {
                        "permission": { "kind": "settings" },
                        "severity": "info",
                        "change": "added"
                    }
                ],
                "removedPermissions": [
                    {
                        "permission": { "kind": "clipboard" },
                        "severity": "notice",
                        "change": "removed"
                    }
                ]
            })
        );
    }

    // =========================================================
    // Installed gadget permissions
    // =========================================================

    #[test]
    fn installed_gadget_permissions_cover_every_loaded_source() {
        let (_dir, archive) =
            archive_with_manifest("weather", "1.0.0", "[permissions]\nclipboard = true\n");
        let source: Arc<dyn GadgetSource> =
            Arc::new(ArchiveSource::open(&archive).expect("archive opens"));
        let sources = HashMap::from([("weather".to_string(), source)]);

        let permissions = installed_gadget_permissions(&sources);

        let items = &permissions["weather"];
        assert_eq!(kinds(items), vec![&Permission::Clipboard]);
        assert_eq!(items[0].change, Change::Unchanged);
    }

    #[test]
    fn a_review_carries_the_download_provenance() {
        let provenance = Provenance {
            download_url: Some(
                "https://github.com/acme/weather/releases/download/v1/weather.torchsnap"
                    .to_string(),
            ),
            referrer_url: None,
            downloaded_by: Some("Safari".to_string()),
        };

        let review = build_review(
            &manifest("1.0.0", ""),
            Path::new("/downloads/weather.torchsnap"),
            Some(provenance.clone()),
            &InstallDecision::Fresh,
        );

        assert_eq!(review.provenance, Some(provenance));
        assert_eq!(review.action, ReviewAction::Install);
    }
}
