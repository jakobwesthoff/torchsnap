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

use crate::wasm::source::GadgetSourceKind;

#[derive(Debug, Clone, Default)]
pub struct RegisteredGadgets {
    kinds: HashMap<String, GadgetSourceKind>,
}

impl RegisteredGadgets {
    pub fn from_kinds(kinds: HashMap<String, GadgetSourceKind>) -> Self {
        Self { kinds }
    }

    pub fn kind(&self, gadget_id: &str) -> Option<GadgetSourceKind> {
        self.kinds.get(gadget_id).copied()
    }
}
