// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Opener/HTTP Test Plugin
//
// Test-only WASM plugin used by the Rust test suite in
// `src-tauri/src/wasm/runtime.rs`. Exercises the `opener`
// and `http` host interfaces via `messaging::handle-message`
// dispatch so tests get a clean `Result<String, String>`
// oracle to assert against without coupling lifecycle to
// test-only behavior.
//
// Rebuild via `just build-test-fixtures` whenever the shared
// WIT world changes.
// =========================================================

wit_bindgen::generate!({
    path: "../../../../plugins/plugin-sdk/wit",
    world: "plugin",
});

use exports::torchsnap::plugin::lifecycle::Guest as LifecycleGuest;
use exports::torchsnap::plugin::messaging::Guest as MessagingGuest;
use exports::torchsnap::plugin::search::{
    ActionId, CatalogEntry, Guest as SearchGuest, PostAction, SearchResponse,
};
use exports::torchsnap::plugin::tasks::Guest as TasksGuest;
use torchsnap::plugin::http::{
    HttpMethod, HttpRequest, fetch,
};
use torchsnap::plugin::opener::open_url;

struct OpenerHttpPlugin;

export!(OpenerHttpPlugin);

impl LifecycleGuest for OpenerHttpPlugin {
    fn enable() {}
    fn disable() {}
    fn on_setting_changed(_key: String, _value: String) {}
}

impl SearchGuest for OpenerHttpPlugin {
    fn entries() -> Vec<CatalogEntry> {
        Vec::new()
    }

    fn search(_query: String, _matched_prefix: Option<String>) -> SearchResponse {
        SearchResponse::Nothing
    }

    fn execute(_entry_id: String, _action_id: ActionId) -> Result<PostAction, String> {
        Ok(PostAction::Nothing)
    }
}

impl MessagingGuest for OpenerHttpPlugin {
    /// Dispatch table for test commands:
    ///
    /// - `"opener.open-url"` — opens `payload` as a URL via the
    ///   `opener` host interface. Returns `"ok"` on success.
    ///
    /// - `"http.get"` — issues a GET request to `payload` via the
    ///   `http` host interface. Returns `"status:<N>"` on success,
    ///   propagates the `http-error` message on failure.
    fn handle_message(method: String, payload: String) -> Result<String, String> {
        match method.as_str() {
            "opener.open-url" => {
                open_url(&payload).map_err(|e| e)?;
                Ok("ok".to_string())
            }
            "http.get" => {
                let request = HttpRequest {
                    url: payload,
                    method: HttpMethod::Get,
                    headers: vec![],
                    body: None,
                    timeout_ms: None,
                    max_body_size: None,
                };
                let resp = fetch(&request).map_err(|e| format!("{e:?}"))?;
                Ok(format!("status:{}", resp.status))
            }
            _ => Err(format!("unknown method: {method}")),
        }
    }
}

impl TasksGuest for OpenerHttpPlugin {
    fn run_task(_task_id: String) -> Result<(), String> {
        Ok(())
    }
}
