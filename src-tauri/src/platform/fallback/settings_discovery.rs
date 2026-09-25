// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Settings Discovery (Fallback)
//
// Stub implementation. Returns an empty list until platform-
// specific discovery is implemented for Linux and Windows.
// =========================================================

use crate::platform::settings_discovery::{SettingsDiscovery, SettingsPane};

pub struct FallbackSettingsDiscovery;

impl SettingsDiscovery for FallbackSettingsDiscovery {
    fn discover(&self) -> anyhow::Result<Vec<SettingsPane>> {
        // TODO: Linux — scan GNOME/KDE settings .desktop files
        // TODO: Windows — enumerate ms-settings: URIs
        Ok(Vec::new())
    }

    fn icon(&self, _pane: &SettingsPane) -> anyhow::Result<Option<image::DynamicImage>> {
        Ok(None)
    }
}
