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

use serde::{Deserialize, Serialize};

use super::{Gadget, Search};
use crate::commands::types::{CatalogEntry, EntryActions, EntryIcon, PostAction};

pub struct BuiltInCommandsGadget;

impl Gadget for BuiltInCommandsGadget {
    fn id(&self) -> &str {
        "builtin-commands"
    }
}

/// What each built-in entry does. Each maps to a host-level
/// post-action the host carries out itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Command {
    Quit,
    ShowSettings,
    ShowDevtools,
}

impl Search for BuiltInCommandsGadget {
    type Command = Command;

    fn entries(&self) -> Vec<CatalogEntry<Command>> {
        vec![
            CatalogEntry {
                id: "quit".into(),
                title: "Quit Torchsnap".into(),
                subtitle: Some("Exit the application".into()),
                icon: Some(EntryIcon::HeroIcon("x-circle".into())),
                keywords: vec!["exit".into(), "close".into()],
                actions: EntryActions::new().primary("Quit", Command::Quit),
            },
            CatalogEntry {
                id: "settings".into(),
                title: "Settings".into(),
                subtitle: Some("Open Torchsnap preferences".into()),
                icon: Some(EntryIcon::HeroIcon("cog-6-tooth".into())),
                keywords: vec!["preferences".into(), "config".into(), "options".into()],
                actions: EntryActions::new().primary("Open", Command::ShowSettings),
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
                actions: EntryActions::new().primary("Open", Command::ShowDevtools),
            },
        ]
    }

    fn execute(&self, command: Command) -> anyhow::Result<PostAction> {
        Ok(match command {
            Command::Quit => PostAction::Quit,
            Command::ShowSettings => PostAction::ShowSettings,
            Command::ShowDevtools => PostAction::ShowDevtools,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_entry_runs_its_own_command() {
        let commands: Vec<(String, Command)> = Search::entries(&BuiltInCommandsGadget)
            .into_iter()
            .map(|entry| {
                let command = entry.actions.primary.expect("primary action").command;
                (entry.id, command)
            })
            .collect();
        assert_eq!(
            commands,
            [
                ("quit".to_string(), Command::Quit),
                ("settings".to_string(), Command::ShowSettings),
                ("devtools".to_string(), Command::ShowDevtools),
            ]
        );
    }

    #[test]
    fn commands_map_to_host_post_actions() {
        let gadget = BuiltInCommandsGadget;
        assert!(matches!(
            Search::execute(&gadget, Command::Quit),
            Ok(PostAction::Quit)
        ));
        assert!(matches!(
            Search::execute(&gadget, Command::ShowSettings),
            Ok(PostAction::ShowSettings)
        ));
        assert!(matches!(
            Search::execute(&gadget, Command::ShowDevtools),
            Ok(PostAction::ShowDevtools)
        ));
    }
}
