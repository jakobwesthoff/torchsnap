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

use crate::gadget_host::GadgetHost;
use serde_json::Value;
use types::{PostAction, SearchMessage, Slot};

/// Search all registered gadgets and stream results to the
/// frontend as they become available.
#[tauri::command]
pub async fn search(
    query: String,
    on_results: Channel<SearchMessage>,
    state: State<'_, Arc<GadgetHost>>,
) -> Result<(), String> {
    state.search(&query, &on_results).await;
    Ok(())
}

/// Run the action in `slot` of a specific entry, routing its
/// command to the gadget that owns the entry. Returns the gadget's
/// `PostAction` so the frontend can decide whether to dismiss the
/// launcher.
///
/// `async fn` + `spawn_blocking` is required, not stylistic: a
/// synchronous `#[tauri::command]` runs on Tauri's IPC blocking
/// thread, which has no Tokio runtime context. Gadget actions can
/// reach the `http::fetch` host import, which requires
/// `Handle::current()` for reqwest's internal machinery.
/// `spawn_blocking` puts the call on a Tokio worker that satisfies
/// that precondition. See the comment on `GadgetHost::execute`.
#[tauri::command]
pub async fn search_execute(
    source: String,
    entry_id: String,
    slot: Slot,
    state: State<'_, Arc<GadgetHost>>,
    app: tauri::AppHandle,
) -> Result<PostAction, String> {
    let host = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        host.execute(&source, &entry_id, slot, &app)
            .map_err(|e| format!("{e:#}"))
    })
    .await
    .expect("search_execute task must not panic")
}

/// Send a custom message to a gadget and optionally receive
/// streamed updates over the channel.
///
/// This command is async so that the blocking gadget handler runs
/// on a Tokio `spawn_blocking` thread rather than the main thread.
/// This is necessary because gadget handlers may perform HTTP
/// requests (e.g. the bangs gadget's "refresh"), which require a
/// Tokio runtime context (`Handle::current()`) for reqwest's
/// internal async machinery.
#[tauri::command]
pub async fn gadget_message(
    source: String,
    method: String,
    payload: Value,
    channel: Channel<Value>,
    state: State<'_, Arc<GadgetHost>>,
) -> Result<Value, String> {
    let host = Arc::clone(&state);
    tokio::task::spawn_blocking(move || {
        // Format the outermost error only (not the full anyhow
        // chain) so the JS Promise rejection sees a clean
        // gadget-level message instead of bridge-internal
        // context labels. The full chain still appears in host
        // logs for debugging.
        host.handle_message(&source, &method, payload, channel)
            .map_err(|e| format!("{e}"))
    })
    .await
    .expect("gadget message task must not panic")
}
