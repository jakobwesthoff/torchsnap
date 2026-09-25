// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Assets Test Gadget
//
// Test-only WASM gadget used by the Rust test suite in
// `src-tauri/src/wasm/runtime.rs`. Exercises the `assets`
// host interface via `messaging::handle-message` dispatch so
// tests get a clean `Result<String, String>` oracle to assert
// against without coupling the lifecycle exports to
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
    CatalogEntry, Guest as SearchGuest, PostAction, SearchResponse,
};
use exports::torchsnap::gadget::tasks::Guest as TasksGuest;
use torchsnap::gadget::assets::{exists as asset_exists, read as asset_read};

struct AssetsPlugin;

export!(AssetsPlugin);

impl LifecycleGuest for AssetsPlugin {
    fn enable() -> Result<(), String> { Ok(()) }
    fn disable() {}
    fn on_setting_changed(_key: String, _value: String) {}
}

impl SearchGuest for AssetsPlugin {
    fn entries() -> Vec<CatalogEntry> {
        Vec::new()
    }

    fn search(_query: String, _matched_prefix: Option<String>) -> SearchResponse {
        SearchResponse::Nothing
    }

    fn execute(_command: String) -> Result<PostAction, String> {
        Ok(PostAction::Nothing)
    }
}

impl MessagingGuest for AssetsPlugin {
    /// Dispatch table:
    ///
    /// - `"assets.read"` — payload is the path. On success returns
    ///   the bytes interpreted as UTF-8; on failure returns
    ///   `format!("{:?}", e)` so tests can match on the error
    ///   variant name.
    ///
    /// - `"assets.read-len"` — like `assets.read` but returns just
    ///   `format!("len:{}", bytes.len())`. Used to verify binary
    ///   fidelity without round-tripping through UTF-8.
    ///
    /// - `"assets.exists"` — payload is the path. Returns `"true"`
    ///   or `"false"`; on error returns `format!("{:?}", e)`.
    fn handle_message(method: String, payload: String) -> Result<String, String> {
        match method.as_str() {
            "assets.read" => {
                let bytes = asset_read(&payload).map_err(|e| format!("{e:?}"))?;
                String::from_utf8(bytes).map_err(|e| format!("utf-8 decode: {e}"))
            }
            "assets.read-len" => {
                let bytes = asset_read(&payload).map_err(|e| format!("{e:?}"))?;
                Ok(format!("len:{}", bytes.len()))
            }
            "assets.exists" => {
                let present = asset_exists(&payload).map_err(|e| format!("{e:?}"))?;
                Ok(if present { "true".to_string() } else { "false".to_string() })
            }
            _ => Err(format!("unknown method: {method}")),
        }
    }
}

impl TasksGuest for AssetsPlugin {
    fn run_task(_task_id: String) -> Result<(), String> {
        Ok(())
    }
}
