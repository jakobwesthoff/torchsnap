// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Opener/HTTP Test Gadget
//
// Test-only WASM gadget used by the Rust test suite in
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
    path: "../../../../gadgets/gadget-sdk/wit",
    world: "gadget",
});

use exports::torchsnap::gadget::lifecycle::Guest as LifecycleGuest;
use exports::torchsnap::gadget::messaging::Guest as MessagingGuest;
use exports::torchsnap::gadget::search::{
    ActionId, CatalogEntry, Guest as SearchGuest, PostAction, SearchResponse,
};
use exports::torchsnap::gadget::tasks::Guest as TasksGuest;
use torchsnap::gadget::http::{
    HttpMethod, HttpRequest, fetch,
};
use torchsnap::gadget::opener::open_url;

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
                open_url(&payload).map_err(|e| format!("{e:?}"))?;
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
                    insecure_tls: false,
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
