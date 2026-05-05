// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Utility Commands (macOS)
//
// Empty trash, start screen saver, and eject disc.
// =========================================================

use std::process::Command;

use anyhow::Context;

use crate::commands::types::{Action, ActionId, CatalogEntry, EntryIcon, PostAction};
use crate::gadgets::system_commands::SystemCommand;
use crate::platform::macos::osascript;

// =========================================================
// Empty Trash
// =========================================================

pub struct EmptyTrash;

impl SystemCommand for EmptyTrash {
    fn id(&self) -> &str {
        "empty-trash"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn entry(&self) -> CatalogEntry {
        CatalogEntry {
            id: self.id().into(),
            title: "Empty Trash".into(),
            subtitle: Some("Permanently delete all items in the Trash".into()),
            icon: Some(EntryIcon::HeroIcon("trash".into())),
            keywords: vec![
                "trash".into(),
                "empty".into(),
                "bin".into(),
                "delete".into(),
            ],
            actions: vec![Action {
                id: ActionId::Open,
                label: "Empty".into(),
                keybinding: None,
            }],
        }
    }

    fn execute(&self) -> anyhow::Result<PostAction> {
        osascript::run(r#"tell application "Finder" to empty trash"#)?;
        Ok(PostAction::Dismiss)
    }
}

// =========================================================
// Start Screen Saver
// =========================================================

pub struct StartScreenSaver;

impl SystemCommand for StartScreenSaver {
    fn id(&self) -> &str {
        "start-screen-saver"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn entry(&self) -> CatalogEntry {
        CatalogEntry {
            id: self.id().into(),
            title: "Start Screen Saver".into(),
            subtitle: Some("Activate the screen saver".into()),
            icon: Some(EntryIcon::HeroIcon("tv".into())),
            keywords: vec!["screensaver".into(), "screen saver".into()],
            actions: vec![Action {
                id: ActionId::Open,
                label: "Start".into(),
                keybinding: None,
            }],
        }
    }

    fn execute(&self) -> anyhow::Result<PostAction> {
        Command::new("open")
            .arg("-a")
            .arg("ScreenSaverEngine")
            .status()
            .context("start screen saver")?;

        Ok(PostAction::Dismiss)
    }
}

// =========================================================
// Eject Disc
// =========================================================

pub struct EjectDisc;

impl SystemCommand for EjectDisc {
    fn id(&self) -> &str {
        "eject-disc"
    }

    fn is_available(&self) -> bool {
        true
    }

    fn entry(&self) -> CatalogEntry {
        CatalogEntry {
            id: self.id().into(),
            title: "Eject Disc".into(),
            subtitle: Some("Eject the optical disc".into()),
            icon: Some(EntryIcon::HeroIcon("arrow-up-on-square".into())),
            keywords: vec![
                "eject".into(),
                "disc".into(),
                "disk".into(),
                "optical".into(),
            ],
            actions: vec![Action {
                id: ActionId::Open,
                label: "Eject".into(),
                keybinding: None,
            }],
        }
    }

    fn execute(&self) -> anyhow::Result<PostAction> {
        Command::new("drutil")
            .arg("eject")
            .status()
            .context("eject disc via drutil")?;

        Ok(PostAction::Dismiss)
    }
}
