// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Power & Session Commands (macOS)
//
// Lock screen, sleep, restart, shutdown, and log out.
// =========================================================

use std::process::Command;

use anyhow::Context;

use crate::commands::types::{Action, ActionId, CatalogEntry, EntryIcon, PostAction};
use crate::platform::macos::osascript;
use crate::plugins::system_commands::SystemCommand;

// =========================================================
// Lock Screen
// =========================================================

pub struct LockScreen;

impl SystemCommand for LockScreen {
    fn id(&self) -> &str {
        "lock-screen"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn entry(&self) -> CatalogEntry {
        CatalogEntry {
            id: self.id().into(),
            title: "Lock Screen".into(),
            subtitle: Some("Lock this Mac".into()),
            icon: Some(EntryIcon::HeroIcon("lock-closed".into())),
            keywords: vec!["lock".into(), "screen".into(), "security".into()],
            actions: vec![Action {
                id: ActionId::Open,
                label: "Lock".into(),
                keybinding: None,
            }],
        }
    }

    fn execute(&self) -> anyhow::Result<PostAction> {
        // `pmset displaysleepnow` puts the display to sleep, which
        // triggers the lock screen if "require password" is enabled
        // in System Settings (the default on modern macOS).
        Command::new("pmset")
            .arg("displaysleepnow")
            .status()
            .context("lock screen via pmset")?;

        Ok(PostAction::Dismiss)
    }
}

// =========================================================
// Sleep
// =========================================================

pub struct Sleep;

impl SystemCommand for Sleep {
    fn id(&self) -> &str {
        "sleep"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn entry(&self) -> CatalogEntry {
        CatalogEntry {
            id: self.id().into(),
            title: "Sleep".into(),
            subtitle: Some("Put this Mac to sleep".into()),
            icon: Some(EntryIcon::HeroIcon("moon".into())),
            keywords: vec!["sleep".into(), "suspend".into()],
            actions: vec![Action {
                id: ActionId::Open,
                label: "Sleep".into(),
                keybinding: None,
            }],
        }
    }

    fn execute(&self) -> anyhow::Result<PostAction> {
        osascript::run(r#"tell application "System Events" to sleep"#)?;
        Ok(PostAction::Dismiss)
    }
}

// =========================================================
// Restart
// =========================================================

pub struct Restart;

impl SystemCommand for Restart {
    fn id(&self) -> &str {
        "restart"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn entry(&self) -> CatalogEntry {
        CatalogEntry {
            id: self.id().into(),
            title: "Restart".into(),
            subtitle: Some("Restart this Mac".into()),
            icon: Some(EntryIcon::HeroIcon("arrow-path".into())),
            keywords: vec!["restart".into(), "reboot".into()],
            actions: vec![Action {
                id: ActionId::Open,
                label: "Restart".into(),
                keybinding: None,
            }],
        }
    }

    fn execute(&self) -> anyhow::Result<PostAction> {
        osascript::run(r#"tell application "System Events" to restart"#)?;
        Ok(PostAction::Dismiss)
    }
}

// =========================================================
// Shutdown
// =========================================================

pub struct Shutdown;

impl SystemCommand for Shutdown {
    fn id(&self) -> &str {
        "shutdown"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn entry(&self) -> CatalogEntry {
        CatalogEntry {
            id: self.id().into(),
            title: "Shut Down".into(),
            subtitle: Some("Shut down this Mac".into()),
            icon: Some(EntryIcon::HeroIcon("power".into())),
            keywords: vec!["shutdown".into(), "power off".into(), "turn off".into()],
            actions: vec![Action {
                id: ActionId::Open,
                label: "Shut Down".into(),
                keybinding: None,
            }],
        }
    }

    fn execute(&self) -> anyhow::Result<PostAction> {
        osascript::run(r#"tell application "System Events" to shut down"#)?;
        Ok(PostAction::Dismiss)
    }
}

// =========================================================
// Log Out
// =========================================================

pub struct LogOut;

impl SystemCommand for LogOut {
    fn id(&self) -> &str {
        "log-out"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn entry(&self) -> CatalogEntry {
        CatalogEntry {
            id: self.id().into(),
            title: "Log Out".into(),
            subtitle: Some("Log out of this user account".into()),
            icon: Some(EntryIcon::HeroIcon("arrow-right-on-rectangle".into())),
            keywords: vec!["log out".into(), "logout".into(), "sign out".into()],
            actions: vec![Action {
                id: ActionId::Open,
                label: "Log Out".into(),
                keybinding: None,
            }],
        }
    }

    fn execute(&self) -> anyhow::Result<PostAction> {
        osascript::run(r#"tell application "System Events" to log out"#)?;
        Ok(PostAction::Dismiss)
    }
}
