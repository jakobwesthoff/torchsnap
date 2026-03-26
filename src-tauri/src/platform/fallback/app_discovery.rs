// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Application Discovery (Fallback)
//
// Stub implementation. Returns an empty list until platform-
// specific discovery is implemented for Linux and Windows.
// =========================================================

use crate::platform::app_discovery::{AppDiscovery, DiscoveredApp};

pub struct FallbackDiscovery;

impl AppDiscovery for FallbackDiscovery {
    fn discover(&self) -> anyhow::Result<Vec<DiscoveredApp>> {
        // TODO: Linux — scan .desktop files from XDG data dirs
        // TODO: Windows — enumerate Start Menu shortcuts / shell:AppsFolder
        Ok(Vec::new())
    }
}
