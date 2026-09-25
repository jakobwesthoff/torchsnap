// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// System Commands Gadget
//
// Provides platform-specific system commands (lock screen,
// sleep, restart, shutdown, appearance toggle, etc.) as
// launcher entries. Each command is a self-contained unit
// implementing the `SystemCommand` trait, assembled by a
// platform-specific factory function.
// =========================================================

use std::sync::Arc;

use crate::caps::{CapRequest, ProvisionedCaps};
use serde::{Deserialize, Serialize};

use crate::commands::types::{CatalogEntry, EntryActions, PostAction};
use crate::gadgets::{Gadget, Search};

// =========================================================
// SystemCommand Trait
//
// Each system command declares its own identity, availability,
// catalog entry (title, icon, keywords), and execution logic.
// The gadget simply collects and delegates to these.
// =========================================================

pub(crate) trait SystemCommand: Send + Sync {
    /// Unique identifier within the gadget (e.g., "sleep", "lock-screen").
    fn id(&self) -> &str;

    /// Whether this command should appear in the launcher right now.
    /// Called on every search keystroke — implementations should be
    /// cheap (no I/O, or cached I/O).
    fn is_available(&self) -> bool;

    /// The catalog entry for this command (title, subtitle, icon,
    /// keywords, and actions built with [`run_on_enter`]).
    fn entry(&self) -> CatalogEntry<RunCommand>;

    /// Whether the command requires user confirmation before executing.
    /// Deferred — always returns false for now.
    #[allow(dead_code)]
    fn needs_confirmation(&self) -> bool {
        false
    }

    /// Execute the command and return the post-action (dismiss, keep open, etc.).
    fn execute(&self) -> anyhow::Result<PostAction>;
}

// =========================================================
// Platform-Specific Command Factories
//
// On macOS, the `macos_commands` module provides the full
// set of system commands. On other platforms, the factory
// returns an empty list — no commands, no entries.
// =========================================================

#[cfg(target_os = "macos")]
mod macos_commands;

#[cfg(target_os = "macos")]
use macos_commands::system_commands;

#[cfg(not(target_os = "macos"))]
fn system_commands() -> Vec<Box<dyn SystemCommand>> {
    vec![]
}

// =========================================================
// SystemCommandsGadget
// =========================================================

pub struct SystemCommandsGadget {
    #[allow(dead_code)]
    caps: Arc<ProvisionedCaps>,
    commands: Vec<Box<dyn SystemCommand>>,
}

impl SystemCommandsGadget {
    pub fn cap_requests() -> Vec<CapRequest> {
        vec![]
    }

    pub fn new(caps: Arc<ProvisionedCaps>) -> Self {
        Self {
            caps,
            commands: system_commands(),
        }
    }
}

/// What a system command entry does: run the command with this id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunCommand(pub String);

/// The actions of a system command entry: Enter runs it.
pub(crate) fn run_on_enter(id: &str, label: &str) -> EntryActions<RunCommand> {
    EntryActions::new().primary(label, RunCommand(id.to_string()))
}

impl Gadget for SystemCommandsGadget {
    fn id(&self) -> &str {
        "system-commands"
    }
}

impl Search for SystemCommandsGadget {
    type Command = RunCommand;

    fn entries(&self) -> Vec<CatalogEntry<RunCommand>> {
        self.commands
            .iter()
            .filter(|cmd| cmd.is_available())
            .map(|cmd| cmd.entry())
            .collect()
    }

    fn execute(&self, RunCommand(id): RunCommand) -> anyhow::Result<PostAction> {
        let cmd = self
            .commands
            .iter()
            .find(|cmd| cmd.id() == id)
            .ok_or_else(|| anyhow::anyhow!("unknown system command: {id}"))?;

        cmd.execute()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::types::{Action, EntryIcon};

    struct FakeCommand {
        available: bool,
    }

    impl SystemCommand for FakeCommand {
        fn id(&self) -> &str {
            "fake"
        }

        fn is_available(&self) -> bool {
            self.available
        }

        fn entry(&self) -> CatalogEntry<RunCommand> {
            CatalogEntry {
                id: self.id().into(),
                title: "Fake".into(),
                subtitle: None,
                icon: Some(EntryIcon::HeroIcon("bolt".into())),
                keywords: vec![],
                actions: run_on_enter(self.id(), "Run"),
            }
        }

        fn execute(&self) -> anyhow::Result<PostAction> {
            Ok(PostAction::KeepOpen)
        }
    }

    fn gadget_with(commands: Vec<Box<dyn SystemCommand>>) -> SystemCommandsGadget {
        SystemCommandsGadget {
            caps: Arc::new(ProvisionedCaps {
                opener: None,
                http: None,
                filesystem: None,
                command: None,
                clipboard: None,
                sql_storage: None,
                website_metadata: None,
                icon_cache: None,
                settings: None,
                frecency: None,
                path_resolver: None,
            }),
            commands,
        }
    }

    #[test]
    fn run_on_enter_puts_the_command_id_in_the_primary_slot() {
        let actions = run_on_enter("sleep", "Sleep");
        assert_eq!(
            actions.primary,
            Some(Action::labeled("Sleep", RunCommand("sleep".into())))
        );
        assert_eq!(actions.iter().count(), 1);
    }

    #[test]
    fn only_available_commands_become_entries() {
        let gadget = gadget_with(vec![
            Box::new(FakeCommand { available: true }),
            Box::new(FakeCommand { available: false }),
        ]);
        assert_eq!(Search::entries(&gadget).len(), 1);
    }

    #[test]
    fn execute_runs_the_command_with_the_id() {
        let gadget = gadget_with(vec![Box::new(FakeCommand { available: true })]);
        let post_action = Search::execute(&gadget, RunCommand("fake".into())).expect("runs");
        assert!(matches!(post_action, PostAction::KeepOpen));
    }

    #[test]
    fn execute_rejects_an_unknown_id() {
        let gadget = gadget_with(vec![]);
        let error = Search::execute(&gadget, RunCommand("gone".into())).expect_err("unknown");
        assert_eq!(error.to_string(), "unknown system command: gone");
    }
}
