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
use crate::commands::types::{ActionId, CatalogEntry, PostAction, ScoredEntry};
use crate::gadgets::Gadget;

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

    /// The catalog entry for this command (title, subtitle, icon, keywords, actions).
    fn entry(&self) -> CatalogEntry;

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

impl Gadget for SystemCommandsGadget {
    fn id(&self) -> &str {
        "system-commands"
    }

    fn entries(&self) -> Vec<CatalogEntry> {
        self.commands
            .iter()
            .filter(|cmd| cmd.is_available())
            .map(|cmd| cmd.entry())
            .collect()
    }

    fn execute(&self, entry: &ScoredEntry, _action_id: &ActionId) -> anyhow::Result<PostAction> {
        let cmd = self
            .commands
            .iter()
            .find(|cmd| cmd.id() == entry.id)
            .ok_or_else(|| anyhow::anyhow!("unknown system command: {}", entry.id))?;

        cmd.execute()
    }
}
