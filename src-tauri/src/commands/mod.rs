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
///
/// `async fn` + `spawn_blocking` is required, not stylistic: a
/// synchronous `#[tauri::command]` runs on Tauri's IPC blocking
/// thread, which has no Tokio runtime context. Plugin actions can
/// reach the `http::fetch` host import, which requires
/// `Handle::current()` for reqwest's internal machinery.
/// `spawn_blocking` puts the call on a Tokio worker that satisfies
/// that precondition. See the comment on `PluginHost::execute`.
#[tauri::command]
pub async fn search_execute(
    source: String,
    entry_id: String,
    action_id: ActionId,
    state: State<'_, Arc<PluginHost>>,
    app: tauri::AppHandle,
) -> Result<PostAction, String> {
    let host = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        host.execute(&source, &entry_id, &action_id, &app)
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .expect("search_execute task must not panic")
}

/// Send a custom message to a plugin and optionally receive
/// streamed updates over the channel.
///
/// This command is async so that the blocking plugin handler runs
/// on a Tokio `spawn_blocking` thread rather than the main thread.
/// This is necessary because plugin handlers may perform HTTP
/// requests (e.g. the bangs plugin's "refresh"), which require a
/// Tokio runtime context (`Handle::current()`) for reqwest's
/// internal async machinery.
#[tauri::command]
pub async fn plugin_message(
    source: String,
    method: String,
    payload: Value,
    channel: Channel<Value>,
    state: State<'_, Arc<PluginHost>>,
) -> Result<Value, String> {
    let host = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        // Format the outermost error only (not the full anyhow
        // chain) so the JS Promise rejection sees a clean
        // plugin-level message instead of bridge-internal
        // context labels. The full chain still appears in host
        // logs for debugging.
        host.handle_message(&source, &method, payload, channel)
            .map_err(|e| format!("{e}"))
    })
    .await
    .expect("plugin message task must not panic")
}
