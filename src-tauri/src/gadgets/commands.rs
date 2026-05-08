// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Built-in Commands Gadget
//
// Provides always-available app commands: Quit, Settings, and
// Developer Tools.
// These appear in the launcher's result list alongside results
// from other gadgets.
// =========================================================

use super::{Gadget, ProvisioningContext};
use crate::commands::types::{Action, ActionId, CatalogEntry, EntryIcon, PostAction, ScoredEntry};

pub struct BuiltInCommandsGadget;

impl Gadget for BuiltInCommandsGadget {
    type Caps = ();

    fn id(&self) -> &str {
        "builtin-commands"
    }

    fn provision(&self, _ctx: &ProvisioningContext) -> anyhow::Result<()> {
        Ok(())
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
            CatalogEntry {
                id: "devtools".into(),
                title: "Developer Tools".into(),
                subtitle: Some("Open Torchsnap developer tools".into()),
                icon: Some(EntryIcon::HeroIcon("wrench-screwdriver".into())),
                keywords: vec![
                    "debug".into(),
                    "console".into(),
                    "logs".into(),
                    "dev".into(),
                ],
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
        entry: &ScoredEntry,
        _action_id: &ActionId,
        _app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        match entry.id.as_str() {
            "quit" => Ok(PostAction::Quit),
            "settings" => Ok(PostAction::ShowSettings),
            "devtools" => Ok(PostAction::ShowDevtools),
            other => anyhow::bail!("unknown built-in command entry: {other}"),
        }
    }
}
