// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Whether an install or uninstall may proceed.
//!
//! Both decisions combine two views of a gadget id: how it was
//! registered at startup (frozen) and what happened to it since
//! (`PendingChanges`). They are pure functions so every combination
//! is pinned by a test.

use super::pending::PendingChange;
use super::registered::Registration;
use crate::wasm::manifest::Manifest;

#[derive(Debug)]
pub enum InstallDecision {
    Fresh,
    /// Swap the archive currently on disk, whose manifest is
    /// `previous`, for the incoming one.
    Replace {
        previous: Box<Manifest>,
        relation: VersionRelation,
    },
    Reject(String),
}

/// How the incoming version relates to the one it replaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VersionRelation {
    Upgrade,
    Same,
    Downgrade,
    /// At least one side is not a semver version, so no order is known.
    Unknown,
}

#[derive(Debug, PartialEq, Eq)]
pub enum UninstallDecision {
    Allowed,
    Reject(String),
}

pub fn decide_install(
    registered: Option<&Registration>,
    pending: Option<&PendingChange>,
    incoming: &Manifest,
) -> InstallDecision {
    let gadget_id = incoming.gadget.id.as_str();

    // Built-in, system and dev ids are never installable, whatever
    // happened in this session.
    let registered_user = match registered {
        Some(Registration::Builtin) => {
            return InstallDecision::Reject(format!(
                "A built-in gadget with id `{gadget_id}` already exists. Built-in gadgets cannot be replaced."
            ));
        }
        Some(Registration::System) => {
            return InstallDecision::Reject(format!(
                "A system gadget with id `{gadget_id}` is bundled with the app. Overriding system gadgets is not supported."
            ));
        }
        Some(Registration::Dev) => {
            return InstallDecision::Reject(format!(
                "A development gadget with id `{gadget_id}` is loaded from the repository. Edit the dev gadget directly or change its id before installing."
            ));
        }
        Some(Registration::User {
            manifest,
            is_directory,
        }) => Some((manifest.as_ref(), *is_directory)),
        None => None,
    };

    // What is on disk now decides what gets replaced: an archive
    // published in this session wins over the startup registration.
    let previous = match (pending, registered_user) {
        (Some(PendingChange::Uninstalled), _) => return InstallDecision::Fresh,
        (Some(change), _) => change
            .manifest()
            .expect("installed and replaced changes carry a manifest"),
        (None, Some((_, true))) => {
            return InstallDecision::Reject(format!(
                "The gadget `{gadget_id}` is installed as a directory under gadgets/. Remove that directory by hand to install this version."
            ));
        }
        (None, Some((manifest, false))) => manifest,
        (None, None) => return InstallDecision::Fresh,
    };

    // A gadget's SQLite database carries the number of migrations
    // applied to it. Opening it with fewer migrations fails, so the
    // gadget would not load after restart; that downgrade is refused
    // here with a way out instead.
    let previous_migrations = sql_migration_count(previous);
    let incoming_migrations = sql_migration_count(incoming);
    if incoming_migrations < previous_migrations {
        return InstallDecision::Reject(format!(
            "This version of `{gadget_id}` declares fewer storage migrations ({incoming_migrations}) than the installed one ({previous_migrations}), so it could not open the installed version's data. Uninstall the installed version first, then install this one."
        ));
    }

    InstallDecision::Replace {
        relation: version_relation(&previous.gadget.version, &incoming.gadget.version),
        previous: Box::new(previous.clone()),
    }
}

fn sql_migration_count(manifest: &Manifest) -> usize {
    manifest
        .storage
        .as_ref()
        .and_then(|storage| storage.sql.as_ref())
        .map_or(0, |sql| sql.migrations.len())
}

pub fn version_relation(installed: &str, incoming: &str) -> VersionRelation {
    match (
        semver::Version::parse(installed),
        semver::Version::parse(incoming),
    ) {
        (Ok(installed), Ok(incoming)) => match incoming.cmp(&installed) {
            std::cmp::Ordering::Greater => VersionRelation::Upgrade,
            std::cmp::Ordering::Equal => VersionRelation::Same,
            std::cmp::Ordering::Less => VersionRelation::Downgrade,
        },
        _ => VersionRelation::Unknown,
    }
}

pub fn decide_uninstall(
    registered: Option<&Registration>,
    pending: Option<&PendingChange>,
    gadget_id: &str,
) -> UninstallDecision {
    let kind = match registered {
        Some(Registration::Builtin) => Some("Builtin"),
        Some(Registration::System) => Some("System"),
        Some(Registration::Dev) => Some("Dev"),
        Some(Registration::User { .. }) | None => None,
    };
    if let Some(kind) = kind {
        return UninstallDecision::Reject(format!(
            "gadget `{gadget_id}` is a {kind} gadget; only user-installed gadgets can be uninstalled"
        ));
    }

    match (registered, pending) {
        (_, Some(PendingChange::Installed { .. } | PendingChange::Replaced { .. })) => {
            UninstallDecision::Allowed
        }
        (Some(_), Some(PendingChange::Uninstalled)) => UninstallDecision::Reject(format!(
            "gadget `{gadget_id}` is already uninstalled; restart Torchsnap to finish removing it"
        )),
        (Some(_), None) => UninstallDecision::Allowed,
        (None, _) => UninstallDecision::Reject(format!("unknown gadget id `{gadget_id}`")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm::manifest::Manifest;

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

    fn with_migrations(version: &str, count: usize) -> Manifest {
        let migrations: Vec<String> = (0..count)
            .map(|i| format!("\"migrations/{i:03}.sql\""))
            .collect();
        manifest(
            version,
            &format!(
                "[storage.sql]\nmigrations = [{}]\n[permissions]\nsql-storage = true\n",
                migrations.join(", ")
            ),
        )
    }

    fn user(version: &str) -> Registration {
        Registration::User {
            manifest: Box::new(manifest(version, "")),
            is_directory: false,
        }
    }

    fn installed(version: &str) -> PendingChange {
        PendingChange::Installed {
            manifest: manifest(version, ""),
        }
    }

    fn replaced(from: &str, to: &str) -> PendingChange {
        PendingChange::Replaced {
            previous_version: from.to_string(),
            manifest: manifest(to, ""),
        }
    }

    fn rejection(decision: InstallDecision) -> String {
        match decision {
            InstallDecision::Reject(message) => message,
            other => panic!("expected a rejection, got {other:?}"),
        }
    }

    fn replaced_version(decision: InstallDecision) -> (String, VersionRelation) {
        match decision {
            InstallDecision::Replace { previous, relation } => (previous.gadget.version, relation),
            other => panic!("expected a replace, got {other:?}"),
        }
    }

    // =========================================================
    // decide_install
    // =========================================================

    #[test]
    fn unknown_id_without_pending_change_installs_fresh() {
        assert!(matches!(
            decide_install(None, None, &manifest("1.0.0", "")),
            InstallDecision::Fresh
        ));
    }

    #[test]
    fn registered_user_gadget_that_was_uninstalled_installs_fresh() {
        assert!(matches!(
            decide_install(
                Some(&user("1.0.0")),
                Some(&PendingChange::Uninstalled),
                &manifest("1.0.0", "")
            ),
            InstallDecision::Fresh
        ));
    }

    #[test]
    fn registered_user_gadget_is_replaced_against_the_registered_version() {
        let decision = decide_install(Some(&user("1.0.0")), None, &manifest("1.1.0", ""));
        assert_eq!(
            replaced_version(decision),
            ("1.0.0".to_string(), VersionRelation::Upgrade)
        );
    }

    #[test]
    fn pending_install_is_replaced_against_the_pending_version() {
        for registered in [None, Some(user("1.0.0"))] {
            let decision = decide_install(
                registered.as_ref(),
                Some(&installed("1.2.0")),
                &manifest("1.3.0", ""),
            );
            assert_eq!(
                replaced_version(decision),
                ("1.2.0".to_string(), VersionRelation::Upgrade)
            );
        }
    }

    #[test]
    fn pending_replace_is_replaced_against_its_latest_version() {
        let decision = decide_install(
            Some(&user("1.0.0")),
            Some(&replaced("1.0.0", "1.2.0")),
            &manifest("1.2.0", ""),
        );
        assert_eq!(
            replaced_version(decision),
            ("1.2.0".to_string(), VersionRelation::Same)
        );
    }

    #[test]
    fn user_gadget_installed_as_a_directory_is_rejected() {
        let registration = Registration::User {
            manifest: Box::new(manifest("1.0.0", "")),
            is_directory: true,
        };
        let message = rejection(decide_install(
            Some(&registration),
            None,
            &manifest("1.1.0", ""),
        ));
        assert!(message.contains("installed as a directory"));
    }

    #[test]
    fn other_source_kinds_are_rejected_with_their_own_message() {
        let cases = [
            (
                Registration::Builtin,
                "A built-in gadget with id `weather` already exists",
            ),
            (
                Registration::System,
                "A system gadget with id `weather` is bundled with the app",
            ),
            (
                Registration::Dev,
                "A development gadget with id `weather` is loaded from the repository",
            ),
        ];
        for (registration, expected) in cases {
            for pending in [None, Some(PendingChange::Uninstalled)] {
                let message = rejection(decide_install(
                    Some(&registration),
                    pending.as_ref(),
                    &manifest("1.0.0", ""),
                ));
                assert!(message.contains(expected), "`{message}`");
            }
        }
    }

    // =========================================================
    // Storage migrations on replace
    // =========================================================

    #[test]
    fn replace_with_fewer_migrations_is_rejected() {
        let registration = Registration::User {
            manifest: Box::new(with_migrations("2.0.0", 3)),
            is_directory: false,
        };
        let message = rejection(decide_install(
            Some(&registration),
            None,
            &with_migrations("1.0.0", 2),
        ));
        assert!(message.contains("fewer storage migrations (2) than the installed one (3)"));
        assert!(message.contains("Uninstall"));
    }

    #[test]
    fn replace_with_equal_or_more_migrations_is_allowed() {
        let registration = Registration::User {
            manifest: Box::new(with_migrations("2.0.0", 3)),
            is_directory: false,
        };
        for count in [3, 4] {
            let decision =
                decide_install(Some(&registration), None, &with_migrations("1.0.0", count));
            assert!(
                matches!(decision, InstallDecision::Replace { .. }),
                "{count}"
            );
        }
    }

    #[test]
    fn replace_that_drops_sql_storage_is_rejected() {
        let registration = Registration::User {
            manifest: Box::new(with_migrations("2.0.0", 1)),
            is_directory: false,
        };
        let decision = decide_install(Some(&registration), None, &manifest("1.0.0", ""));
        assert!(matches!(decision, InstallDecision::Reject(_)));
    }

    #[test]
    fn replace_without_storage_on_either_side_is_allowed() {
        let decision = decide_install(Some(&user("2.0.0")), None, &manifest("1.0.0", ""));
        assert_eq!(
            replaced_version(decision),
            ("2.0.0".to_string(), VersionRelation::Downgrade)
        );
    }

    // =========================================================
    // Version relation
    // =========================================================

    #[test]
    fn version_relation_compares_semver() {
        assert_eq!(version_relation("1.0.0", "1.0.1"), VersionRelation::Upgrade);
        assert_eq!(version_relation("1.2.0", "1.2.0"), VersionRelation::Same);
        assert_eq!(
            version_relation("2.0.0", "1.9.9"),
            VersionRelation::Downgrade
        );
        assert_eq!(
            version_relation("1.0.0-beta.1", "1.0.0"),
            VersionRelation::Upgrade
        );
    }

    #[test]
    fn version_relation_is_unknown_for_unparsable_versions() {
        assert_eq!(
            version_relation("2024.1", "1.0.0"),
            VersionRelation::Unknown
        );
        assert_eq!(
            version_relation("1.0.0", "latest"),
            VersionRelation::Unknown
        );
    }

    // =========================================================
    // decide_uninstall
    // =========================================================

    #[test]
    fn registered_user_gadget_can_be_uninstalled() {
        assert_eq!(
            decide_uninstall(Some(&user("1.0.0")), None, "weather"),
            UninstallDecision::Allowed
        );
    }

    #[test]
    fn pending_installs_and_replaces_can_be_uninstalled() {
        assert_eq!(
            decide_uninstall(None, Some(&installed("1.0.0")), "weather"),
            UninstallDecision::Allowed
        );
        assert_eq!(
            decide_uninstall(Some(&user("1.0.0")), Some(&installed("1.0.0")), "weather"),
            UninstallDecision::Allowed
        );
        assert_eq!(
            decide_uninstall(
                Some(&user("1.0.0")),
                Some(&replaced("1.0.0", "1.1.0")),
                "weather"
            ),
            UninstallDecision::Allowed
        );
    }

    #[test]
    fn already_uninstalled_gadget_asks_for_a_restart() {
        let UninstallDecision::Reject(message) = decide_uninstall(
            Some(&user("1.0.0")),
            Some(&PendingChange::Uninstalled),
            "weather",
        ) else {
            panic!("expected a rejection");
        };
        assert!(message.contains("already uninstalled"));
        assert!(message.contains("restart"));
    }

    #[test]
    fn unknown_id_cannot_be_uninstalled() {
        let UninstallDecision::Reject(message) = decide_uninstall(None, None, "weather") else {
            panic!("expected a rejection");
        };
        assert!(message.contains("unknown gadget id `weather`"));
    }

    #[test]
    fn non_user_gadgets_cannot_be_uninstalled() {
        for registration in [
            Registration::Builtin,
            Registration::System,
            Registration::Dev,
        ] {
            let UninstallDecision::Reject(message) =
                decide_uninstall(Some(&registration), None, "weather")
            else {
                panic!("expected a rejection for {registration:?}");
            };
            assert!(message.contains("only user-installed gadgets can be uninstalled"));
        }
    }
}
