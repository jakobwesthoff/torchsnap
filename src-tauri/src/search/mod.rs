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

pub mod types;

use std::sync::Arc;

use tauri::State;
use tauri::ipc::Channel;

use crate::plugin_host::PluginHost;
use serde_json::Value;
use types::{ActionId, PostAction, SearchMessage};

/// Search all registered plugins and stream results to the
/// frontend as they become available.
#[tauri::command]
pub async fn search(
    query: String,
    on_results: Channel<SearchMessage>,
    state: State<'_, Arc<PluginHost>>,
) -> Result<(), String> {
    state.search(&query, &on_results).await;
    Ok(())
}

/// Execute an action on a specific entry, routing to the plugin
/// that owns it. Returns the plugin's `PostAction` so the frontend
/// can decide whether to dismiss the launcher.
#[tauri::command]
pub fn search_execute(
    source: String,
    entry_id: String,
    action_id: ActionId,
    state: State<'_, Arc<PluginHost>>,
    app: tauri::AppHandle,
) -> Result<PostAction, String> {
    state
        .execute(&source, &entry_id, &action_id, &app)
        .map_err(|e| format!("{e:#}"))
}

/// Send a custom message to a plugin and optionally receive
/// streamed updates over the channel.
#[tauri::command]
pub fn plugin_message(
    source: String,
    method: String,
    payload: Value,
    channel: Channel<Value>,
    state: State<'_, Arc<PluginHost>>,
) -> Result<Value, String> {
    state
        .handle_message(&source, &method, payload, channel)
        .map_err(|e| format!("{e:#}"))
}
