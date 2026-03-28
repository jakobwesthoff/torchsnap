// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Shortcut Manager
//
// Registers global keyboard shortcuts declared by plugins and
// routes activations back to the owning plugin. When a plugin's
// shortcut handler returns `ShowCustomUI`, the manager shows
// the launcher and emits an `activate-plugin-custom-ui` event
// so the frontend switches to that plugin's view.
//
// The manager also handles the built-in launcher toggle
// shortcut, consolidating all global shortcut logic in one
// place.
// =========================================================

use std::sync::Arc;

use tauri::Emitter;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_store::Store;

use crate::platform::{LauncherPanel as _, PlatformLauncherPanel};
use crate::plugins::{CatalogPlugin, QueryPlugin};
use crate::search::catalog::CatalogRegistry;
use crate::search::types::PostAction;

// =========================================================
// Types
// =========================================================

/// A registered shortcut with enough context to route the
/// activation back to the owning plugin.
enum ShortcutOwner {
    Catalog(Arc<dyn CatalogPlugin>),
    Query(Arc<dyn QueryPlugin>),
}

struct RegisteredShortcut {
    shortcut: Shortcut,
    plugin_id: String,
    shortcut_id: String,
    owner: ShortcutOwner,
}

/// Payload emitted with the `activate-plugin-custom-ui` event.
#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ActivatePluginPayload {
    plugin_id: String,
}

// =========================================================
// Registration
// =========================================================

/// Register all global shortcuts: the built-in launcher toggle
/// and all plugin-declared shortcuts.
///
/// Called once during app setup after plugin settings have been
/// initialized and the catalog registry is populated.
pub fn register_all(
    app: &tauri::AppHandle,
    store: &Arc<Store<tauri::Wry>>,
    registry: &CatalogRegistry,
) {
    let mut registered: Vec<RegisteredShortcut> = Vec::new();

    // Collect shortcuts from catalog plugins.
    for plugin in registry.catalog_plugins() {
        let plugin_id = plugin.id().to_string();
        for decl in plugin.shortcuts() {
            if let Some(r) = resolve_shortcut(
                store,
                &plugin_id,
                &decl,
                ShortcutOwner::Catalog(Arc::clone(plugin)),
            ) {
                registered.push(r);
            }
        }
    }

    // Collect shortcuts from query plugins.
    for plugin in registry.query_plugins() {
        let plugin_id = plugin.id().to_string();
        for decl in plugin.shortcuts() {
            if let Some(r) = resolve_shortcut(
                store,
                &plugin_id,
                &decl,
                ShortcutOwner::Query(Arc::clone(plugin)),
            ) {
                registered.push(r);
            }
        }
    }

    // Read the launcher toggle shortcut.
    let launcher_combo_str = store
        .get("globalShortcut")
        .and_then(|v| v.as_str().map(String::from))
        .expect("globalShortcut initialized by settings defaults");

    let launcher_shortcut = launcher_combo_str
        .parse::<Shortcut>()
        .expect("valid launcher shortcut");

    // Collect all combos for bulk registration.
    let mut all_combos: Vec<Shortcut> = vec![launcher_shortcut];
    for r in &registered {
        all_combos.push(r.shortcut);
    }

    let registered = Arc::new(registered);
    let handle = app.clone();

    app.global_shortcut()
        .on_shortcuts(all_combos, move |_app, shortcut, event| {
            if event.state != ShortcutState::Pressed {
                return;
            }

            // Launcher toggle.
            if *shortcut == launcher_shortcut {
                crate::toggle_launcher_window(&handle);
                return;
            }

            // Plugin shortcut routing.
            let Some(r) = registered.iter().find(|r| r.shortcut == *shortcut) else {
                return;
            };

            let result = match &r.owner {
                ShortcutOwner::Catalog(p) => p.handle_shortcut(&r.shortcut_id, &handle),
                ShortcutOwner::Query(p) => p.handle_shortcut(&r.shortcut_id, &handle),
            };

            match result {
                Ok(PostAction::ShowCustomUI) => {
                    show_launcher_with_plugin(&handle, &r.plugin_id);
                }
                Ok(_) => {}
                Err(e) => {
                    eprintln!(
                        "shortcut: {}.{} handler failed: {e:#}",
                        r.plugin_id, r.shortcut_id
                    );
                }
            }
        })
        .expect("register global shortcuts");
}

/// Read the configured combo from settings and parse it.
/// Falls back to the declared default if the setting is missing.
fn resolve_shortcut(
    store: &Arc<Store<tauri::Wry>>,
    plugin_id: &str,
    decl: &crate::plugins::PluginShortcut,
    owner: ShortcutOwner,
) -> Option<RegisteredShortcut> {
    let full_key = format!("plugins.{plugin_id}.{}", decl.settings_key);

    let combo_str = store
        .get(&full_key)
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| decl.default_shortcut.to_string());

    let shortcut = match combo_str.parse::<Shortcut>() {
        Ok(s) => s,
        Err(e) => {
            eprintln!(
                "shortcut: invalid combo '{combo_str}' for {plugin_id}.{}: {e}",
                decl.id
            );
            return None;
        }
    };

    Some(RegisteredShortcut {
        shortcut,
        plugin_id: plugin_id.to_string(),
        shortcut_id: decl.id.to_string(),
        owner,
    })
}

/// Show the launcher and emit `activate-plugin-custom-ui` so the
/// frontend switches to the plugin's view.
fn show_launcher_with_plugin(app: &tauri::AppHandle, plugin_id: &str) {
    crate::position_launcher_on_cursor_monitor(app);

    if let Err(e) = PlatformLauncherPanel::show(app) {
        eprintln!("shortcut: failed to show launcher: {e:#}");
        return;
    }

    if let Err(e) = app.emit(
        "activate-plugin-custom-ui",
        ActivatePluginPayload {
            plugin_id: plugin_id.to_string(),
        },
    ) {
        eprintln!("shortcut: failed to emit activate-plugin-custom-ui: {e:#}");
    }
}
