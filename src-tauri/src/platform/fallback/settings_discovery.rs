// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Settings Discovery (Fallback)
//
// Stub implementation. Returns an empty list, and opens nothing,
// until platform-specific discovery is implemented for Linux and
// Windows.
// =========================================================

use crate::platform::settings_discovery::{SettingsDiscovery, SettingsPane};

/// The stub finds no panes, so there is no icon data to carry. A unit
/// type alias rather than a struct: a struct nobody constructs would
/// be reported as dead code.
pub type FallbackPaneIcon = ();

pub struct FallbackSettingsDiscovery;

impl SettingsDiscovery for FallbackSettingsDiscovery {
    fn discover(&self) -> anyhow::Result<Vec<SettingsPane>> {
        // TODO: Linux — scan GNOME/KDE settings .desktop files
        // TODO: Windows — enumerate ms-settings: URIs
        Ok(Vec::new())
    }

    fn icon(&self, _source: &FallbackPaneIcon) -> anyhow::Result<Option<image::DynamicImage>> {
        Ok(None)
    }

    fn pane_url(&self, _pane_id: &str) -> anyhow::Result<String> {
        anyhow::bail!("system-preferences open not implemented on this platform")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_a_pane_is_not_supported() {
        let error = FallbackSettingsDiscovery
            .pane_url("gnome-network-panel")
            .expect_err("the stub opens nothing");

        assert!(error.to_string().contains("not implemented"));
    }

    #[test]
    fn the_stub_discovers_no_panes() {
        assert!(
            FallbackSettingsDiscovery
                .discover()
                .expect("discovery cannot fail")
                .is_empty()
        );
    }
}
