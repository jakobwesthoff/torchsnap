// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Install and uninstall operations since startup.
//!
//! The registry snapshot describes what loaded at startup and never
//! changes; the disk changes with every install and uninstall. This
//! record bridges the two until the next restart, so decisions see
//! "installed a moment ago" and "uninstalled a moment ago" instead of
//! the stale startup state.

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub enum PendingChange {
    /// An archive was published for this id since startup.
    Installed,
    /// The gadget registered at startup was uninstalled. It keeps
    /// running until restart.
    Uninstalled,
}

#[derive(Debug, Default)]
pub struct PendingChanges {
    changes: HashMap<String, PendingChange>,
}

impl PendingChanges {
    pub fn get(&self, gadget_id: &str) -> Option<&PendingChange> {
        self.changes.get(gadget_id)
    }

    pub fn record_install(&mut self, gadget_id: &str) {
        self.changes
            .insert(gadget_id.to_string(), PendingChange::Installed);
    }

    /// Record an uninstall. `registered` says whether the gadget was
    /// loaded at startup: such a gadget still runs and is remembered
    /// as `Uninstalled`, while an id that only ever existed as a
    /// pending install goes back to having no record at all.
    pub fn record_uninstall(&mut self, gadget_id: &str, registered: bool) {
        if registered {
            self.changes
                .insert(gadget_id.to_string(), PendingChange::Uninstalled);
        } else {
            self.changes.remove(gadget_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_install_is_recorded() {
        let mut pending = PendingChanges::default();

        pending.record_install("test-gadget");

        assert!(matches!(
            pending.get("test-gadget"),
            Some(PendingChange::Installed)
        ));
    }

    #[test]
    fn uninstalling_a_pending_install_removes_the_record() {
        let mut pending = PendingChanges::default();
        pending.record_install("test-gadget");

        pending.record_uninstall("test-gadget", false);

        assert!(pending.get("test-gadget").is_none());
    }

    #[test]
    fn uninstalling_a_registered_gadget_is_remembered() {
        let mut pending = PendingChanges::default();

        pending.record_uninstall("test-gadget", true);

        assert!(matches!(
            pending.get("test-gadget"),
            Some(PendingChange::Uninstalled)
        ));
    }

    #[test]
    fn reinstalling_after_uninstall_records_the_install() {
        let mut pending = PendingChanges::default();
        pending.record_uninstall("test-gadget", true);

        pending.record_install("test-gadget");

        assert!(matches!(
            pending.get("test-gadget"),
            Some(PendingChange::Installed)
        ));
    }

    /// A registered gadget that was uninstalled, reinstalled and
    /// uninstalled again is still running from startup, so it has to
    /// end up as `Uninstalled`, not forgotten.
    #[test]
    fn uninstalling_a_reinstalled_registered_gadget_returns_to_uninstalled() {
        let mut pending = PendingChanges::default();
        pending.record_uninstall("test-gadget", true);
        pending.record_install("test-gadget");

        pending.record_uninstall("test-gadget", true);

        assert!(matches!(
            pending.get("test-gadget"),
            Some(PendingChange::Uninstalled)
        ));
    }
}
