// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Built-in Commands Plugin
//
// Provides always-available app commands: Quit and Settings.
// These appear in the launcher's result list alongside results
// from other plugins.
// =========================================================

use super::Plugin;
use crate::search::types::{Action, ActionId, CatalogEntry, EntryIcon, PostAction};

pub struct BuiltInCommandsPlugin;

impl Plugin for BuiltInCommandsPlugin {
    fn id(&self) -> &str {
        "builtin-commands"
    }

    fn entries(&self) -> Vec<CatalogEntry> {
        vec![
            CatalogEntry {
                id: "quit".into(),
                title: "Quit Torchsnap".into(),
                subtitle: Some("Exit the application".into()),
                icon: Some(EntryIcon::HeroIcon("x-circle".into())),
                keywords: vec!["exit".into(), "close".into()],
                actions: vec![Action {
                    id: ActionId::Open,
                    label: "Quit".into(),
                    keybinding: None,
                }],
            },
            CatalogEntry {
                id: "settings".into(),
                title: "Settings".into(),
                subtitle: Some("Open Torchsnap preferences".into()),
                icon: Some(EntryIcon::HeroIcon("cog-6-tooth".into())),
                keywords: vec!["preferences".into(), "config".into(), "options".into()],
                actions: vec![Action {
                    id: ActionId::Open,
                    label: "Open".into(),
                    keybinding: None,
                }],
            },
        ]
    }

    fn execute(
        &self,
        entry_id: &str,
        _action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        match entry_id {
            "quit" => {
                app.exit(0);
            }
            "settings" => {
                crate::show_settings_window(app);
            }
            other => anyhow::bail!("unknown built-in command entry: {other}"),
        }
        Ok(PostAction::Dismiss)
    }
}
