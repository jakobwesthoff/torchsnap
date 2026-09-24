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
use crate::wasm::source::GadgetSourceKind;

#[derive(Debug, PartialEq, Eq)]
pub enum InstallDecision {
    Fresh,
    Reject(String),
}

#[derive(Debug, PartialEq, Eq)]
pub enum UninstallDecision {
    Allowed,
    Reject(String),
}

pub fn decide_install(
    registered: Option<GadgetSourceKind>,
    pending: Option<&PendingChange>,
    gadget_id: &str,
) -> InstallDecision {
    // Built-in, system and dev ids are never installable, whatever
    // happened in this session.
    match registered {
        Some(GadgetSourceKind::Builtin) => {
            return InstallDecision::Reject(format!(
                "A built-in gadget with id `{gadget_id}` already exists. Built-in gadgets cannot be replaced."
            ));
        }
        Some(GadgetSourceKind::System) => {
            return InstallDecision::Reject(format!(
                "A system gadget with id `{gadget_id}` is bundled with the app. Overriding system gadgets is not supported."
            ));
        }
        Some(GadgetSourceKind::Dev) => {
            return InstallDecision::Reject(format!(
                "A development gadget with id `{gadget_id}` is loaded from the repository. Edit the dev gadget directly or change its id before installing."
            ));
        }
        Some(GadgetSourceKind::User) | None => {}
    }

    match (registered, pending) {
        (_, Some(PendingChange::Installed)) => InstallDecision::Reject(format!(
            "A gadget with id `{gadget_id}` was installed in this session. Uninstall it first, then retry."
        )),
        (Some(GadgetSourceKind::User), Some(PendingChange::Uninstalled)) => InstallDecision::Fresh,
        (Some(GadgetSourceKind::User), None) => InstallDecision::Reject(format!(
            "A user gadget with id `{gadget_id}` is already installed. Uninstall the existing version, then retry."
        )),
        _ => InstallDecision::Fresh,
    }
}

pub fn decide_uninstall(
    registered: Option<GadgetSourceKind>,
    pending: Option<&PendingChange>,
    gadget_id: &str,
) -> UninstallDecision {
    if let Some(kind) = registered
        && kind != GadgetSourceKind::User
    {
        return UninstallDecision::Reject(format!(
            "gadget `{gadget_id}` is a {kind:?} gadget; only user-installed gadgets can be uninstalled"
        ));
    }

    match (registered, pending) {
        (_, Some(PendingChange::Installed)) => UninstallDecision::Allowed,
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

    fn installed() -> PendingChange {
        PendingChange::Installed
    }

    fn rejection(decision: InstallDecision) -> String {
        match decision {
            InstallDecision::Reject(message) => message,
            other => panic!("expected a rejection, got {other:?}"),
        }
    }

    // =========================================================
    // decide_install
    // =========================================================

    #[test]
    fn unknown_id_without_pending_change_installs_fresh() {
        assert_eq!(
            decide_install(None, None, "weather"),
            InstallDecision::Fresh
        );
    }

    #[test]
    fn registered_user_gadget_that_was_uninstalled_installs_fresh() {
        assert_eq!(
            decide_install(
                Some(GadgetSourceKind::User),
                Some(&PendingChange::Uninstalled),
                "weather"
            ),
            InstallDecision::Fresh
        );
    }

    #[test]
    fn registered_user_gadget_is_rejected_as_already_installed() {
        let message = rejection(decide_install(
            Some(GadgetSourceKind::User),
            None,
            "weather",
        ));
        assert!(message.contains("A user gadget with id `weather` is already installed"));
    }

    #[test]
    fn pending_install_is_rejected_as_installed_in_this_session() {
        let pending = installed();
        let message = rejection(decide_install(None, Some(&pending), "weather"));
        assert!(message.contains("`weather` was installed in this session"));

        let message = rejection(decide_install(
            Some(GadgetSourceKind::User),
            Some(&pending),
            "weather",
        ));
        assert!(message.contains("`weather` was installed in this session"));
    }

    #[test]
    fn other_source_kinds_are_rejected_with_their_own_message() {
        let cases = [
            (
                GadgetSourceKind::Builtin,
                "A built-in gadget with id `weather` already exists",
            ),
            (
                GadgetSourceKind::System,
                "A system gadget with id `weather` is bundled with the app",
            ),
            (
                GadgetSourceKind::Dev,
                "A development gadget with id `weather` is loaded from the repository",
            ),
        ];
        for (kind, expected) in cases {
            for pending in [None, Some(PendingChange::Uninstalled)] {
                let message = rejection(decide_install(Some(kind), pending.as_ref(), "weather"));
                assert!(message.contains(expected), "{kind:?}: `{message}`");
            }
        }
    }

    // =========================================================
    // decide_uninstall
    // =========================================================

    #[test]
    fn registered_user_gadget_can_be_uninstalled() {
        assert_eq!(
            decide_uninstall(Some(GadgetSourceKind::User), None, "weather"),
            UninstallDecision::Allowed
        );
    }

    #[test]
    fn pending_install_can_be_uninstalled() {
        let pending = installed();
        assert_eq!(
            decide_uninstall(None, Some(&pending), "weather"),
            UninstallDecision::Allowed
        );
        assert_eq!(
            decide_uninstall(Some(GadgetSourceKind::User), Some(&pending), "weather"),
            UninstallDecision::Allowed
        );
    }

    #[test]
    fn already_uninstalled_gadget_asks_for_a_restart() {
        let UninstallDecision::Reject(message) = decide_uninstall(
            Some(GadgetSourceKind::User),
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
        for kind in [
            GadgetSourceKind::Builtin,
            GadgetSourceKind::System,
            GadgetSourceKind::Dev,
        ] {
            let UninstallDecision::Reject(message) = decide_uninstall(Some(kind), None, "weather")
            else {
                panic!("expected a rejection for {kind:?}");
            };
            assert!(message.contains("only user-installed gadgets can be uninstalled"));
        }
    }
}
