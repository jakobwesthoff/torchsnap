// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Search Module
//
// Tauri commands for searching catalog entries and executing
// actions. The `search` command streams results over a Tauri
// channel for progressive rendering on the frontend.
// =========================================================

pub mod catalog;
pub mod types;

use std::sync::Mutex;

use tauri::State;
use tauri::ipc::Channel;

use catalog::CatalogRegistry;
use types::{ActionId, PostAction, SearchMessage};
use serde_json::Value;

/// Search all registered catalogs and stream results to the frontend.
///
/// Synchronous because catalog search is sub-millisecond for small
/// entry sets. Will become async when query plugins are added.
#[tauri::command]
pub fn search(
    query: String,
    on_results: Channel<SearchMessage>,
    state: State<'_, Mutex<CatalogRegistry>>,
) {
    let registry = state.lock().expect("catalog registry lock");
    let result = registry.search(&query);

    let _ = on_results.send(SearchMessage::CatalogResults {
        entries: result.entries,
        custom_plugin_view: result.custom_plugin_view,
        matched_prefix: result.matched_prefix,
    });
    let _ = on_results.send(SearchMessage::Done);
}

/// Execute an action on a specific entry, routing to the plugin
/// that owns it. Returns the plugin's `PostAction` so the frontend
/// can decide whether to dismiss the launcher.
#[tauri::command]
pub fn execute_action(
    source: String,
    entry_id: String,
    action_id: ActionId,
    state: State<'_, Mutex<CatalogRegistry>>,
    app: tauri::AppHandle,
) -> Result<PostAction, String> {
    let registry = state.lock().expect("catalog registry lock");
    registry
        .execute(&source, &entry_id, &action_id, &app)
        .map_err(|e| format!("{e:#}"))
}

/// Send a custom message to a plugin and optionally receive
/// streamed updates over the channel.
///
/// The plugin is identified by `source` (its ID). The `method`
/// and `payload` are forwarded to the plugin's `handle_message`
/// implementation. The `channel` can be used by the plugin to
/// push live updates back to the frontend.
#[tauri::command]
pub fn plugin_message(
    source: String,
    method: String,
    payload: Value,
    channel: Channel<Value>,
    state: State<'_, Mutex<CatalogRegistry>>,
) -> Result<Value, String> {
    let registry = state.lock().expect("catalog registry lock");
    registry
        .handle_message(&source, &method, payload, channel)
        .map_err(|e| format!("{e:#}"))
}
