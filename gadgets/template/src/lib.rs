// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

// =========================================================
// Template Gadget
//
// Starting point for new Torchsnap WASM gadgets. Showcases
// the full gadget authoring surface so you can copy this
// directory, rename the gadget ID in `manifest.toml` and
// `Cargo.toml`, and build from here.
//
// What this file demonstrates:
//
// - Catalog entries via `entries()`
// - Custom UI via prefix-triggered `search()`
// - Action execution via `execute()`
// - Structured logging with metadata
// - Reading gadget settings via the `settings::get` host
//   import and parsing the JSON-encoded values
// - Reacting to user setting changes via the
//   `on_setting_changed` lifecycle export
// - Custom frontend ↔ gadget RPC via the
//   `messaging::handle-message` guest export — your frontend
//   calls `sendMessage(method, payload)` and this method
//   dispatches by name
// - Per-gadget SQLite storage via the `sql::connection` host
//   import and a `[storage.sql]` block in `manifest.toml`
//   listing migration files. The host creates the database
//   and applies migrations before the guest's `enable()` runs
// - Scheduled background tasks via `[[tasks]]` entries in
//   `manifest.toml` and the `tasks::run-task` guest export.
//   The host runs a per-gadget tokio scheduler that fires
//   the export at the cron-scheduled time
//
// Optional capabilities (see usage snippets at the bottom of
// this file):
//
// - `website-metadata::lookup` — host-shared website metadata
//   cache. Resolves a domain to its title, description, and
//   favicon (cached and coalesced across all gadgets). Opt in
//   via `[permissions]\nwebsite-metadata = true` in
//   `manifest.toml`.
// =========================================================

use torchsnap_gadget_sdk::prelude::*;

struct TemplatePlugin;
define_gadget!(TemplatePlugin);

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
        let greeting: String = settings::get_or_else("greeting", || "(unset)".into());
        let verbose: bool = settings::get_or("verbose", false);

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
            "Template gadget enabled",
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
            "Template gadget disabled",
            &[],
            None,
        );
    }

    fn on_setting_changed(key: String, value: String) {
        // The host coalesces rapid same-key writes (e.g. a
        // slider being dragged) before this fires, so it is
        // safe to react to every invocation — there is no
        // need to debounce on the gadget side. Push the
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
// Custom frontend ↔ gadget messaging
//
// Your React frontend calls `sendMessage(method, payload)`
// (from `useGadgetRuntime()`); the host routes the call to
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
// NOT auto-disable the gadget — scheduled tasks are
// best-effort background work.
// =========================================================

impl TasksGuest for TemplatePlugin {
    fn run_task(task_id: String) -> Result<(), String> {
        match task_id.as_str() {
            // The `heartbeat` task fires every 5 minutes per
            // the manifest schedule. It records a row in the
            // SQL table and logs the timestamp so gadget
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
                let greeting: String =
                    settings::get_or_else("greeting", || "(unset)".into());
                messaging::to_response(&serde_json::json!({ "greeting": greeting }))
            }

            // `enable-count` returns the number of rows in
            // `enable_log` — i.e. how many times this gadget
            // has been enabled. Demonstrates composing the
            // SQL API with the messaging API and round-
            // tripping a typed result row across the WIT
            // boundary.
            "enable-count" => {
                let db = sql::connection();
                let row = sql::query_one(&db, "SELECT COUNT(*) FROM enable_log", &[])
                    .map_err(|e| format!("query: {e}"))?;
                let count = row.and_then(|r| r.integer(0)).unwrap_or(0);
                messaging::to_response(&serde_json::json!({ "enable_count": count }))
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
            subtitle: Some("Template WASM gadget demo".into()),
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
        // loading pipeline: WASM → host → torchsnap-gadget://
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

    fn execute(entry: ScoredEntry, _action_id: ActionId) -> Result<PostAction, String> {
        logging::log(
            logging::LogLevel::Info,
            &format!("Executed entry: {}", entry.id),
            &[("entry_id".into(), entry.id)],
            None,
        );
        Ok(PostAction::Dismiss)
    }
}

// =========================================================
// Snippet: website-metadata host import
//
// Reference examples for gadgets that surface websites in
// their results. Not wired into this template's runtime
// search path — copy into your `search()` body when you
// want enriched URL entries.
//
// Manifest opt-in (uncomment in `manifest.toml`):
//
// ```toml
// [permissions]
// website-metadata = true
// ```
//
// Without that flag every lookup logs an error and falls
// back to the supplied default — the host enforces the gate
// before touching the cache.
//
// `entry-icon` is the same Rust type whether it comes from
// `website_metadata::Metadata::Found` or appears on a
// `ScoredEntry`, so the favicon plugs straight into a result
// with no conversion.
//
// Two demos below: `favicon_or` covers the one-line
// "give me a favicon or this fallback" case, and
// `website_metadata_demo` shows the full `Metadata` match
// for gadgets that also want title/description. The raw
// WIT bindings remain available under `website_metadata_host`
// when a gadget needs to distinguish `ReachableNoData` from
// `Unreachable` or surface the underlying error string.
// =========================================================

/// Cache-first favicon-or-fallback — the canonical idiom for a
/// per-keystroke search loop.
#[allow(dead_code)]
fn website_metadata_favicon_demo(domain: &str) -> EntryIcon {
    website_metadata::favicon_or(domain, EntryIcon::HeroIcon("globe-alt".into()))
}

/// Title- and favicon-rich metadata lookup using the wrapper's
/// flattened `Metadata` enum.
///
/// Uses [`website_metadata::lookup_cached`] so the call never
/// blocks the search loop. On a cold cache `Pending` is
/// treated the same as "no data yet" — the result is suppressed
/// for this render, and the next keystroke will see the
/// populated cache. Gadgets that genuinely need to wait for the
/// fetch should swap in [`website_metadata::lookup_blocking`].
#[allow(dead_code)]
fn website_metadata_demo(domain: &str) -> Option<ScoredEntry> {
    use website_metadata::Metadata;

    let (title, icon) = match website_metadata::lookup_cached(domain) {
        Metadata::Found(entry) => {
            let title = entry.title.unwrap_or_else(|| domain.to_string());
            (title, entry.favicon)
        }
        // No cached data yet (cold cache, no metadata, unreachable,
        // or denied permission) — suppress the entry rather than
        // showing a placeholder. `lookup_cached` has already
        // scheduled the background fetch if one is warranted.
        Metadata::NoData | Metadata::Unreachable | Metadata::Pending => return None,
    };

    Some(ScoredEntry {
        id: domain.to_string(),
        title,
        subtitle: Some(format!("Open {domain}")),
        icon: Some(icon),
        score: 500,
        title_highlight_positions: vec![],
        subtitle_highlight_positions: vec![],
        actions: vec![Action {
            id: ActionId::Open,
            label: "Open in Browser".into(),
        }],
        data: None,
    })
}
