// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Install, replace and uninstall operations since startup.
//!
//! The registry snapshot describes what loaded at startup and never
//! changes; the disk changes with every operation. This record bridges
//! the two until the next restart, so decisions see "installed a
//! moment ago" or "replaced a moment ago" instead of the stale startup
//! state.
//!
//! Transitions, each pinned by a test below:
//!
//! | Before | Event | After |
//! |---|---|---|
//! | none | fresh install | `Installed` |
//! | `Installed` | replace | `Installed` with the new manifest |
//! | none (registered) | replace | `Replaced` from the registered version |
//! | `Replaced` | replace | `Replaced`, same `previous_version` |
//! | `Replaced` | undo | none |
//! | `Installed` | uninstall, not registered | none |
//! | any | uninstall, registered | `Uninstalled` |
//! | `Uninstalled` | fresh install | `Installed` |
//!
//! "Uninstalled but not registered" never exists: an id that only
//! lived as a pending install simply has no record after uninstall.

use std::collections::HashMap;

use crate::wasm::manifest::Manifest;

#[derive(Debug, Clone)]
pub enum PendingChange {
    /// An archive was published for an id that had no running gadget
    /// to replace. The manifest is the one on disk now.
    Installed { manifest: Manifest },
    /// The gadget registered at startup was replaced. It keeps running
    /// in `previous_version` until restart; the manifest is the one on
    /// disk now.
    Replaced {
        previous_version: String,
        manifest: Manifest,
    },
    /// The gadget registered at startup was uninstalled. It keeps
    /// running until restart.
    Uninstalled,
}

impl PendingChange {
    /// The manifest of the archive currently on disk, if any.
    pub fn manifest(&self) -> Option<&Manifest> {
        match self {
            PendingChange::Installed { manifest } | PendingChange::Replaced { manifest, .. } => {
                Some(manifest)
            }
            PendingChange::Uninstalled => None,
        }
    }
}

#[derive(Debug, Default)]
pub struct PendingChanges {
    changes: HashMap<String, PendingChange>,
}

impl PendingChanges {
    pub fn get(&self, gadget_id: &str) -> Option<&PendingChange> {
        self.changes.get(gadget_id)
    }

    pub fn record_install(&mut self, gadget_id: &str, manifest: Manifest) {
        self.changes
            .insert(gadget_id.to_string(), PendingChange::Installed { manifest });
    }

    /// Record that the archive for `gadget_id` was swapped for one with
    /// `manifest`. `previous_version` is the version being replaced;
    /// it is kept only when nothing was replaced yet in this session,
    /// so it always names what actually runs until restart.
    pub fn record_replace(&mut self, gadget_id: &str, previous_version: &str, manifest: Manifest) {
        let change = match self.changes.remove(gadget_id) {
            Some(PendingChange::Installed { .. }) => PendingChange::Installed { manifest },
            Some(PendingChange::Replaced {
                previous_version, ..
            }) => PendingChange::Replaced {
                previous_version,
                manifest,
            },
            None | Some(PendingChange::Uninstalled) => PendingChange::Replaced {
                previous_version: previous_version.to_string(),
                manifest,
            },
        };
        self.changes.insert(gadget_id.to_string(), change);
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

    /// Drop the record, used when undo restored the startup state.
    pub fn forget(&mut self, gadget_id: &str) {
        self.changes.remove(gadget_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(version: &str) -> Manifest {
        Manifest::parse(&format!(
            r#"
            [gadget]
            id = "weather"
            name = "Weather"
            description = "Forecasts"
            version = "{version}"
            wasm = "gadget.wasm"
            icon = "heroicons:beaker"
            "#
        ))
        .expect("test manifest parses")
    }

    fn version_on_disk(pending: &PendingChanges) -> Option<String> {
        pending
            .get("weather")
            .and_then(PendingChange::manifest)
            .map(|m| m.gadget.version.clone())
    }

    #[test]
    fn fresh_install_is_recorded_with_its_manifest() {
        let mut pending = PendingChanges::default();

        pending.record_install("weather", manifest("1.0.0"));

        assert!(matches!(
            pending.get("weather"),
            Some(PendingChange::Installed { .. })
        ));
        assert_eq!(version_on_disk(&pending).as_deref(), Some("1.0.0"));
    }

    #[test]
    fn replacing_a_pending_install_stays_an_install() {
        let mut pending = PendingChanges::default();
        pending.record_install("weather", manifest("1.0.0"));

        pending.record_replace("weather", "1.0.0", manifest("1.1.0"));

        assert!(matches!(
            pending.get("weather"),
            Some(PendingChange::Installed { .. })
        ));
        assert_eq!(version_on_disk(&pending).as_deref(), Some("1.1.0"));
    }

    #[test]
    fn replacing_a_registered_gadget_remembers_the_running_version() {
        let mut pending = PendingChanges::default();

        pending.record_replace("weather", "1.0.0", manifest("1.1.0"));

        assert!(matches!(
            pending.get("weather"),
            Some(PendingChange::Replaced { previous_version, .. }) if previous_version == "1.0.0"
        ));
        assert_eq!(version_on_disk(&pending).as_deref(), Some("1.1.0"));
    }

    #[test]
    fn a_second_replace_keeps_the_running_version() {
        let mut pending = PendingChanges::default();
        pending.record_replace("weather", "1.0.0", manifest("1.1.0"));

        pending.record_replace("weather", "1.1.0", manifest("1.2.0"));

        assert!(matches!(
            pending.get("weather"),
            Some(PendingChange::Replaced { previous_version, .. }) if previous_version == "1.0.0"
        ));
        assert_eq!(version_on_disk(&pending).as_deref(), Some("1.2.0"));
    }

    #[test]
    fn forgetting_a_replace_removes_the_record() {
        let mut pending = PendingChanges::default();
        pending.record_replace("weather", "1.0.0", manifest("1.1.0"));

        pending.forget("weather");

        assert!(pending.get("weather").is_none());
    }

    #[test]
    fn uninstalling_a_pending_install_removes_the_record() {
        let mut pending = PendingChanges::default();
        pending.record_install("weather", manifest("1.0.0"));

        pending.record_uninstall("weather", false);

        assert!(pending.get("weather").is_none());
    }

    #[test]
    fn uninstalling_a_registered_gadget_is_remembered() {
        for before in [None, Some("replaced")] {
            let mut pending = PendingChanges::default();
            if before.is_some() {
                pending.record_replace("weather", "1.0.0", manifest("1.1.0"));
            }

            pending.record_uninstall("weather", true);

            assert!(matches!(
                pending.get("weather"),
                Some(PendingChange::Uninstalled)
            ));
            assert_eq!(version_on_disk(&pending), None);
        }
    }

    #[test]
    fn reinstalling_after_uninstall_records_the_install() {
        let mut pending = PendingChanges::default();
        pending.record_uninstall("weather", true);

        pending.record_install("weather", manifest("1.0.0"));

        assert!(matches!(
            pending.get("weather"),
            Some(PendingChange::Installed { .. })
        ));
    }

    /// A registered gadget that was uninstalled, reinstalled and
    /// uninstalled again is still running from startup, so it has to
    /// end up as `Uninstalled`, not forgotten.
    #[test]
    fn uninstalling_a_reinstalled_registered_gadget_returns_to_uninstalled() {
        let mut pending = PendingChanges::default();
        pending.record_uninstall("weather", true);
        pending.record_install("weather", manifest("1.0.0"));

        pending.record_uninstall("weather", true);

        assert!(matches!(
            pending.get("weather"),
            Some(PendingChange::Uninstalled)
        ));
    }
}
