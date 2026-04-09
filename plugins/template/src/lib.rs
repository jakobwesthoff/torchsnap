// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Template Plugin
//
// Starting point for new Torchsnap WASM plugins. Showcases
// the full plugin authoring surface so you can copy this
// directory, rename the plugin ID in `manifest.toml` and
// `Cargo.toml`, and build from here.
//
// What this file demonstrates:
//
// - Catalog entries via `entries()`
// - Custom UI via prefix-triggered `search()`
// - Action execution via `execute()`
// - Structured logging with metadata
// - Reading plugin settings via the `settings::get` host
//   import and parsing the JSON-encoded values
// - Reacting to user setting changes via the
//   `on_setting_changed` lifecycle export
// - Custom frontend ↔ plugin RPC via the
//   `messaging::handle-message` guest export — your frontend
//   calls `sendMessage(method, payload)` and this method
//   dispatches by name
// - Per-plugin SQLite storage via the `sql::connection` host
//   import and a `[storage.sql]` block in `manifest.toml`
//   listing migration files. The host creates the database
//   and applies migrations before the guest's `enable()` runs
// - Scheduled background tasks via `[[tasks]]` entries in
//   `manifest.toml` and the `tasks::run-task` guest export.
//   The host runs a per-plugin tokio scheduler that fires
//   the export at the cron-scheduled time
// =========================================================

wit_bindgen::generate!({
    path: "../../wit",
    world: "plugin",
});

use exports::torchsnap::plugin::lifecycle::Guest as LifecycleGuest;
use exports::torchsnap::plugin::messaging::Guest as MessagingGuest;
use exports::torchsnap::plugin::search::{
    Action, ActionId, CatalogEntry, EntryIcon, Guest as SearchGuest, PostAction,
    SearchResponse, ViewResponse,
};
use exports::torchsnap::plugin::tasks::Guest as TasksGuest;
use torchsnap::plugin::{logging, settings, sql};

struct TemplatePlugin;

export!(TemplatePlugin);

impl LifecycleGuest for TemplatePlugin {
    fn enable() {
        // Read each setting once at startup so internal state
        // matches whatever the user configured (or whatever
        // `manifest.toml`'s `[settings]` defaults set). For
        // settings read frequently from a hot path, cache the
        // parsed value in a `static` `AtomicBool` / `OnceLock`
        // and refresh it from `on_setting_changed` below; this
        // example just logs the values to keep the wiring
        // visible.
        let greeting = read_string_setting("greeting").unwrap_or_else(|| "(unset)".into());
        let verbose = read_bool_setting("verbose").unwrap_or(false);

        // Append a row recording this enable. The host
        // initializes the database before enable() runs, so
        // sql::connection() is always ready.
        {
            let db = sql::connection();
            if let Err(e) = db.execute("INSERT INTO enable_log DEFAULT VALUES", &[]) {
                logging::log(
                    logging::LogLevel::Warn,
                    &format!("recording enable timestamp failed: {e}"),
                    &[],
                    None,
                );
            }
        }

        logging::log(
            logging::LogLevel::Info,
            "Template plugin enabled",
            &[
                ("greeting".into(), greeting),
                ("verbose".into(), verbose.to_string()),
            ],
            None,
        );
    }

    fn disable() {
        logging::log(
            logging::LogLevel::Info,
            "Template plugin disabled",
            &[],
            None,
        );
    }

    fn on_setting_changed(key: String, value: String) {
        // The host coalesces rapid same-key writes (e.g. a
        // slider being dragged) before this fires, so it is
        // safe to react to every invocation — there is no
        // need to debounce on the plugin side. Push the
        // parsed value into your in-memory cache here so
        // subsequent reads see the latest state. This example
        // just logs the change.
        logging::log(
            logging::LogLevel::Info,
            &format!("Setting `{key}` changed"),
            &[("key".into(), key), ("value".into(), value)],
            None,
        );
    }
}

// =========================================================
// Settings helpers
//
// `settings::get` returns the value as a JSON-encoded
// string (`"true"`, `"42"`, `"\"hello\""`, `"{\"a\":1}"`,
// …) wrapped in `Option`. Parse it into whatever shape your
// plugin uses via `serde_json::from_str`. The two helpers
// below cover the common bool and string cases — extend the
// pattern with your own typed accessors as needed.
// =========================================================

fn read_bool_setting(key: &str) -> Option<bool> {
    serde_json::from_str(&settings::get(key)?).ok()
}

fn read_string_setting(key: &str) -> Option<String> {
    serde_json::from_str(&settings::get(key)?).ok()
}

// =========================================================
// Custom frontend ↔ plugin messaging
//
// Your React frontend calls `sendMessage(method, payload)`
// (from `usePluginRuntime()`); the host routes the call to
// `handle_message` below. Dispatch by `method` and return a
// JSON-encoded response on success or `Err(string)` on
// failure — the frontend's promise will resolve / reject
// accordingly.
//
// Both the incoming `payload` and the outgoing success arm
// are JSON-encoded strings, so use `serde_json::from_str`
// for parsing and `serde_json::to_string` for serializing.
// Define dedicated request / response structs and derive
// `Serialize` / `Deserialize` for them — that gives you
// strong typing on both sides of the boundary.
// =========================================================

// =========================================================
// Scheduled background tasks
//
// Each `[[tasks]]` entry in `manifest.toml` declares a
// task `id` and a 5-field POSIX cron expression. The host
// fires `run_task(id)` at the cron-scheduled times. Match
// on `task_id` and dispatch to whatever work the task is
// supposed to do.
//
// Returning `Err(string)` is logged by the host but does
// NOT auto-disable the plugin — scheduled tasks are
// best-effort background work.
// =========================================================

impl TasksGuest for TemplatePlugin {
    fn run_task(task_id: String) -> Result<(), String> {
        match task_id.as_str() {
            // The `heartbeat` task fires every 5 minutes per
            // the manifest schedule. It records a row in the
            // SQL table and logs the timestamp so plugin
            // authors can see the scheduler firing in
            // devtools.
            "heartbeat" => {
                let db = sql::connection();
                db.execute("INSERT INTO enable_log DEFAULT VALUES", &[])
                    .map_err(|e| format!("insert: {e}"))?;
                logging::log(logging::LogLevel::Info, "Heartbeat fired", &[], None);
                Ok(())
            }
            other => Err(format!("unknown task: {other}")),
        }
    }
}

impl MessagingGuest for TemplatePlugin {
    fn handle_message(method: String, payload: String) -> Result<String, String> {
        match method.as_str() {
            // `echo` round-trips the payload back unchanged.
            // Use this as a smoke test from your frontend
            // when you first wire up `sendMessage` — if the
            // exact payload comes back, the bridge is
            // working end to end.
            "echo" => Ok(payload),

            // `current-greeting` reads the `greeting`
            // setting and returns it as a JSON object so the
            // frontend can render it without parsing the raw
            // setting itself. Demonstrates composing the
            // settings API with the messaging API.
            "current-greeting" => {
                let greeting =
                    read_string_setting("greeting").unwrap_or_else(|| "(unset)".into());
                serde_json::to_string(&serde_json::json!({ "greeting": greeting }))
                    .map_err(|e| format!("serialize response: {e}"))
            }

            // `enable-count` returns the number of rows in
            // `enable_log` — i.e. how many times this plugin
            // has been enabled. Demonstrates composing the
            // SQL API with the messaging API and round-
            // tripping a typed result row across the WIT
            // boundary.
            "enable-count" => {
                let db = sql::connection();
                let rows = db
                    .query("SELECT COUNT(*) FROM enable_log", &[])
                    .map_err(|e| format!("query: {e}"))?;
                let count = match rows.first().and_then(|row| row.first()) {
                    Some(sql::SqlValue::Integer(n)) => *n,
                    _ => 0,
                };
                serde_json::to_string(&serde_json::json!({ "enable_count": count }))
                    .map_err(|e| format!("serialize response: {e}"))
            }

            other => Err(format!("unknown method: {other}")),
        }
    }
}

impl SearchGuest for TemplatePlugin {
    fn entries() -> Vec<CatalogEntry> {
        vec![CatalogEntry {
            id: "template-demo".into(),
            title: "Template Demo".into(),
            subtitle: Some("Template WASM plugin demo".into()),
            icon: Some(EntryIcon::HeroIcon("puzzle-piece".into())),
            keywords: vec!["template".into(), "demo".into()],
            actions: vec![Action {
                id: ActionId::Open,
                label: "Open".into(),
            }],
        }]
    }

    fn search(query: String, matched_prefix: Option<String>) -> SearchResponse {
        if query.is_empty() {
            return SearchResponse::Nothing;
        }

        // When triggered via the "tpl:" prefix, show a custom
        // frontend view. This demonstrates the full dynamic
        // loading pipeline: WASM → host → torchsnap-plugin://
        // protocol → dynamic import() → React component.
        if matched_prefix.is_some() {
            let data = serde_json::json!({ "query": query });
            return SearchResponse::CustomUi(ViewResponse {
                view: "demo".into(),
                data: Some(data.to_string()),
                results: vec![],
            });
        }

        SearchResponse::Nothing
    }

    fn execute(entry_id: String, _action_id: ActionId) -> Result<PostAction, String> {
        logging::log(
            logging::LogLevel::Info,
            &format!("Executed entry: {entry_id}"),
            &[("entry_id".into(), entry_id)],
            None,
        );
        Ok(PostAction::Dismiss)
    }
}
