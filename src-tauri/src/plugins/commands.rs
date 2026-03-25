// =========================================================
// Built-in Commands Plugin
//
// Provides always-available system commands: Quit, Settings,
// and Toggle Theme. These appear in the launcher's result list
// alongside results from other plugins.
// =========================================================

use anyhow::Context;
use tauri::Emitter;

use super::CatalogPlugin;
use crate::search::types::{Action, ActionId, CatalogEntry, EntryIcon};

pub struct BuiltInCommandsPlugin;

impl CatalogPlugin for BuiltInCommandsPlugin {
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
            CatalogEntry {
                id: "toggle-theme".into(),
                title: "Toggle Theme".into(),
                subtitle: Some("Switch between light and dark mode".into()),
                icon: Some(EntryIcon::HeroIcon("sun".into())),
                keywords: vec![
                    "dark".into(),
                    "light".into(),
                    "appearance".into(),
                    "mode".into(),
                ],
                actions: vec![Action {
                    id: ActionId::Open,
                    label: "Toggle".into(),
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
    ) -> anyhow::Result<()> {
        match entry_id {
            "quit" => {
                app.exit(0);
            }
            "settings" => {
                crate::show_settings_window(app);
            }
            "toggle-theme" => {
                app.emit("toggle-theme", ())
                    .context("emit toggle-theme event")?;
            }
            other => anyhow::bail!("unknown built-in command entry: {other}"),
        }
        Ok(())
    }
}
