// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Appearance Command (macOS)
//
// Toggle between dark and light mode. A single command that
// is always available — no need to query the current state,
// which avoids osascript roundtrips on every keystroke.
// =========================================================

use crate::platform::macos::osascript;
use crate::plugins::system_commands::SystemCommand;
use crate::search::types::{Action, ActionId, CatalogEntry, EntryIcon, PostAction};

// =========================================================
// Toggle Dark / Light Mode
// =========================================================

pub struct ToggleAppearance;

impl SystemCommand for ToggleAppearance {
    fn id(&self) -> &str {
        "toggle-appearance"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn entry(&self) -> CatalogEntry {
        CatalogEntry {
            id: self.id().into(),
            title: "Toggle Dark / Light Mode".into(),
            subtitle: Some("Switch system appearance between dark and light".into()),
            icon: Some(EntryIcon::HeroIcon("moon".into())),
            keywords: vec![
                "dark mode".into(),
                "dark".into(),
                "light mode".into(),
                "light".into(),
                "bright".into(),
                "night".into(),
                "appearance".into(),
            ],
            actions: vec![Action {
                id: ActionId::Open,
                label: "Toggle".into(),
                keybinding: None,
            }],
        }
    }

    fn execute(&self) -> anyhow::Result<PostAction> {
        osascript::run(
            r#"tell application "System Events" to tell appearance preferences to set dark mode to not dark mode"#,
        )?;
        Ok(PostAction::Dismiss)
    }
}
