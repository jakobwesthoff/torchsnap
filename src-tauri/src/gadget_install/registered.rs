// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Snapshot of the gadgets registered at startup.
//!
//! The `GadgetHost` slot list is frozen once `setup` finishes, so a
//! copy taken there stays accurate for the whole process lifetime.
//! Install decisions read this copy instead of the host, which keeps
//! them independent of the Tauri runtime the host is tied to.

use std::collections::HashMap;
use std::sync::Arc;

use crate::wasm::manifest::Manifest;
use crate::wasm::source::{GadgetSource, GadgetSourceKind};

/// How a gadget id was registered at startup.
#[derive(Debug, Clone)]
pub enum Registration {
    Builtin,
    System,
    Dev,
    /// A user-installed gadget. Replacing it compares against this
    /// manifest; `is_directory` marks the hand-placed directory form,
    /// which install never writes and never replaces.
    User {
        manifest: Box<Manifest>,
        is_directory: bool,
    },
}

#[derive(Debug, Clone, Default)]
pub struct RegisteredGadgets {
    entries: HashMap<String, Registration>,
}

impl RegisteredGadgets {
    pub fn from_entries(entries: impl IntoIterator<Item = (String, Registration)>) -> Self {
        Self {
            entries: entries.into_iter().collect(),
        }
    }

    /// Build the snapshot from the host's source kinds and the loaded
    /// WASM sources. A user gadget always comes from a WASM source; if
    /// one is missing from the source map it did not load, so it is
    /// left out rather than registered without a manifest.
    pub fn from_host(
        kinds: HashMap<String, GadgetSourceKind>,
        sources: &HashMap<String, Arc<dyn GadgetSource>>,
    ) -> Self {
        let entries = kinds.into_iter().filter_map(|(id, kind)| {
            let registration = match kind {
                GadgetSourceKind::Builtin => Registration::Builtin,
                GadgetSourceKind::System => Registration::System,
                GadgetSourceKind::Dev => Registration::Dev,
                GadgetSourceKind::User => {
                    let source = sources.get(&id)?;
                    Registration::User {
                        manifest: Box::new(source.manifest().clone()),
                        is_directory: source.root_path().is_dir(),
                    }
                }
            };
            Some((id, registration))
        });
        Self::from_entries(entries)
    }

    pub fn get(&self, gadget_id: &str) -> Option<&Registration> {
        self.entries.get(gadget_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm::manifest::test_helpers::archive_with_manifest;
    use crate::wasm::source::ArchiveSource;

    #[test]
    fn user_gadgets_carry_their_manifest_and_source_form() {
        let (_dir, archive) = archive_with_manifest("weather", "1.2.0", "");
        let source: Arc<dyn GadgetSource> =
            Arc::new(ArchiveSource::open(&archive).expect("archive opens"));
        let kinds = HashMap::from([
            ("weather".to_string(), GadgetSourceKind::User),
            ("clipboard-manager".to_string(), GadgetSourceKind::Builtin),
        ]);
        let sources = HashMap::from([("weather".to_string(), source)]);

        let registered = RegisteredGadgets::from_host(kinds, &sources);

        assert!(matches!(
            registered.get("weather"),
            Some(Registration::User { manifest, is_directory: false }) if manifest.gadget.version == "1.2.0"
        ));
        assert!(matches!(
            registered.get("clipboard-manager"),
            Some(Registration::Builtin)
        ));
    }

    #[test]
    fn user_gadget_without_a_loaded_source_is_left_out() {
        let kinds = HashMap::from([("weather".to_string(), GadgetSourceKind::User)]);

        let registered = RegisteredGadgets::from_host(kinds, &HashMap::new());

        assert!(registered.get("weather").is_none());
    }
}
