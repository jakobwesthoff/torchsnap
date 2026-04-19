# Plugin Development Guide

This guide walks you through building a Torchsnap plugin from scratch. It covers
the unified `Plugin` trait, the full lifecycle, every API surface available to
plugins, and progressively more complex examples.

## Contents

- [Concepts](#concepts)
- [Quick Start: Minimal Catalog Plugin](#quick-start-minimal-catalog-plugin)
- [Plugin Types](#plugin-types)
  - [Catalog-only Plugins](#catalog-only-plugins)
  - [Query-only Plugins](#query-only-plugins)
  - [Hybrid Plugins](#hybrid-plugins)
- [Lifecycle](#lifecycle)
- [Settings](#settings)
  - [Declaring Defaults](#declaring-defaults)
  - [Reading at Runtime](#reading-at-runtime)
  - [Reacting to Changes](#reacting-to-changes)
- [Frecency](#frecency)
- [Global Shortcuts](#global-shortcuts)
- [Custom UI](#custom-ui)
  - [Taking Over the Result Area](#taking-over-the-result-area)
  - [Frontend ↔ Backend Messaging](#frontend--backend-messaging)
- [Opener (WASM plugins, ADR 0037)](#opener-wasm-plugins-adr-0037)
- [HTTP (WASM plugins, ADR 0038)](#http-wasm-plugins-adr-0038)
- [How your plugin is discovered (WASM plugins, ADR 0035)](#how-your-plugin-is-discovered-wasm-plugins-adr-0035)
- [Packaging and distributing your plugin (WASM plugins, ADR 0035)](#packaging-and-distributing-your-plugin-wasm-plugins-adr-0035)
- [Types Reference](#types-reference)
- [Registering a Plugin](#registering-a-plugin)
- [Full Examples](#full-examples)
  - [Static Command List](#static-command-list)
  - [Prefix-Routed Query Plugin](#prefix-routed-query-plugin)
  - [Reactive Background Plugin](#reactive-background-plugin)

---

## Concepts

Torchsnap's search pipeline is built around a single unified `Plugin` trait
(ADR 0024). Every plugin — regardless of whether it contributes a static entry
catalog, performs its own query matching, or does both — implements the same
trait. The search mode is determined by which methods you override:

| Search mode | Methods implemented | When to use |
|---|---|---|
| Catalog-only | `entries()` | App launchers, command palettes, static lists |
| Query-only | `search()` | Custom matching logic, dynamic results, prefix-gated modes |
| Hybrid | `entries()` + `search()` | Plugins that do both depending on context |

The host handles filtering for catalog entries (via
[nucleo](https://github.com/helix-editor/nucleo)) and calls `search()` for
query results. Results from both paths are merged and sorted by score before
being sent to the frontend.

All plugins are internal (compiled into the binary). There is no dynamic
loading at present — each plugin is registered at startup in `lib.rs`.

**Enabled state** is managed entirely by the host, not by the plugin. The host
persists `enabled.<plugin-id>` in the top-level settings store and calls
`enable()` / `disable()` as the user toggles plugins. Plugins do not declare
or own an `enabled` key.

---

## Quick Start: Minimal Catalog Plugin

The smallest useful plugin: a hardcoded command list.

```rust
// src-tauri/src/plugins/my_commands.rs

use anyhow::Context as _;
use tauri::Manager;

use crate::plugins::{Plugin, PluginContext};
use crate::search::types::{Action, ActionId, CatalogEntry, PostAction};

pub struct MyCommandsPlugin;

impl Plugin for MyCommandsPlugin {
    fn id(&self) -> &str {
        "my-commands"
    }

    fn entries(&self) -> Vec<CatalogEntry> {
        vec![CatalogEntry {
            id: "greet".to_string(),
            title: "Say Hello".to_string(),
            subtitle: Some("Greets the world".to_string()),
            icon: None,
            keywords: vec![],
            actions: vec![Action {
                id: ActionId::Open,
                label: "Run".to_string(),
                keybinding: None,
            }],
        }]
    }

    fn execute(
        &self,
        entry_id: &str,
        _action_id: &ActionId,
        _app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        match entry_id {
            "greet" => println!("Hello, world!"),
            _ => anyhow::bail!("unknown entry: {entry_id}"),
        }
        Ok(PostAction::Dismiss)
    }
}
```

Register it in `lib.rs`:

```rust
host.register(Box::new(plugins::my_commands::MyCommandsPlugin));
```

That is all it takes. The host calls `entries()` on every keystroke, filters
the list with nucleo, and calls `execute()` when the user activates an action.

---

## Plugin Types

All three modes implement the same `Plugin` trait. The difference is purely
which methods you override.

### Catalog-only Plugins

Override `entries()`. The host handles all matching — the plugin never sees
the query string. This is the right choice for static or semi-static lists
where fuzzy matching over titles and keywords is sufficient.

**Key points for `entries()`:**

- Called on every search keystroke — keep it fast.
- May be called before `enable()` finishes. Return an empty `Vec` in that case.
- Thread safety: `entries()` takes `&self`, so internal state must use
  `RwLock`, `Mutex`, or atomics.

**`entry_id` in `execute()`:**

The `entry_id` is whatever you put in `CatalogEntry::id`. It is your
responsibility to make these stable and unique within the plugin.

### Query-only Plugins

Override `search()`. The plugin receives the raw query and scores its own
results. `entries()` can be left at the default empty implementation.

**Prefix routing (ADR 0012):**

If your plugin registers prefixes via `search_prefixes()`, it receives
exclusive routing when the user types a matching prefix. All catalog plugins
and prefix-less query plugins are bypassed entirely.

```rust
fn search_prefixes(&self) -> &[String] {
    // Exclusive when the user types ">".
    &[">".to_string()]
}
```

When exclusive routing fires:
- The prefix is stripped before `search()` is called.
- `matched_prefix` is `Some(">")` — useful when a plugin registers multiple
  prefixes and needs to know which triggered.
- Longest prefix wins when multiple prefixes could match.

**Always-on query plugins (ADR 0023):**

Return an empty `search_prefixes()` slice (the default) to run on every query
alongside catalog plugins. Results are merged and sorted by score.

**`search()` return value (ADR 0021):**

`search()` returns `Option<PluginResponse>`. Return `None` to indicate the
plugin has nothing to contribute. Return `Some(PluginResponse::Results(vec))`
for standard list rendering, or a `CustomUI` / `InlineUI` variant to involve
a custom frontend component (see [Custom UI](#custom-ui)).

### Hybrid Plugins

Override both `entries()` and `search()`. The host merges catalog results
(filtered by nucleo) with the pre-scored results from `search()`. Use this
when a plugin maintains a static catalog but also wants to inject dynamic
results for certain queries.

---

## Lifecycle

Plugin lifecycle has four ordered phases (ADR 0025).

### Phase 1 — Settings initialization (synchronous, at registration)

`initialize_settings()` is called synchronously for every plugin before any
`enable()` call. Use it to ensure defaults exist in the settings store.

```rust
fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit {
    settings
        .ensure("maxResults", 20)
        .ensure("shortcut.open", "CmdOrCtrl+Shift+H")
}
```

Values already in the store are never overwritten — `ensure` only fills gaps.
See [Settings → Declaring Defaults](#declaring-defaults) for migrations.

Note: do **not** declare an `enabled` key here. The host manages
`enabled.<plugin-id>` at the top level and never looks for it inside a
plugin's own settings namespace.

### Phase 2 — Enable (on startup and when the user re-enables the plugin)

`enable()` is called on a rayon background thread pool (sized to ~70% of
available cores). All plugins' `enable()` calls run in parallel at startup.

Unlike the old `setup()` method, `enable()` can be called multiple times — once
at startup and again every time the user enables the plugin after having
disabled it. Write `enable()` so it is safe to call repeatedly.

```rust
fn enable(&self, app: &tauri::AppHandle, ctx: &PluginContext) {
    // Safe to block — this is a background thread, not the Tokio runtime.
    let items = discover_items(app);
    *self.cache.write().expect("cache not poisoned") = items;
}
```

**What you can do in `enable()`:**
- Block on I/O (filesystem scan, subprocess, HTTP request).
- Spawn background threads that run until `disable()` signals them to stop.
- Read settings via `ctx.settings`.
- Clone and stash `ctx.frecency` for later use in `entries()` or `search()`.

**Important:** `entries()` and `search()` may be called before `enable()`
completes. Guard cached state with `RwLock::try_read()` or return an empty
list.

### Phase 3 — Shortcut registration

After all `enable()` calls complete, the host registers all plugin shortcuts
with the OS. A reactor task watches for settings changes that affect shortcut
key bindings and re-registers when needed.

### Phase 4 — Disable (on user toggle or application exit)

`disable()` is called when the user disables the plugin, and once for every
plugin on application exit. Because `enable()` may be called again later,
`disable()` should only stop active background work — do not drop persistent
state that `enable()` would need to rebuild from scratch.

```rust
fn disable(&self) {
    // Signal the background thread to stop.
    self.stop_signal.store(true, Ordering::SeqCst);
}
```

`disable()` must complete quickly. Blocking indefinitely will delay shutdown
or freeze the settings toggle.

---

## Settings

Plugin settings live under the key prefix `plugins.<plugin-id>.` in the
app-wide settings store (a JSON file backed by `tauri-plugin-store`).

The settings API is split into two types: `SettingsInit` for declaring
defaults at startup, and `PluginSettings` for reading values at runtime.

### Declaring Defaults

Override `initialize_settings()` to define your plugin's settings schema:

```rust
fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit {
    settings
        .ensure("maxHistory", 200)
        .ensure("shortcut.open", "CmdOrCtrl+Shift+V")
}
```

`ensure(key, default)` sets the key only if it does not already exist. Keys
the user has previously configured are left untouched.

**Migrations:** If you renamed a setting key, use `remove()` to carry forward
the old value:

```rust
fn initialize_settings(&self, mut settings: SettingsInit) -> SettingsInit {
    // Rename "pollInterval" → "pollingIntervalMs".
    if let Some(old) = settings.remove("pollInterval") {
        settings = settings.ensure("pollingIntervalMs", old);
    }
    settings.ensure("pollingIntervalMs", 500)
}
```

### Reading at Runtime

`ctx.settings` is a `PluginSettings` handle injected into `enable()`. It
provides scoped read access to `plugins.<id>.*`:

```rust
fn enable(&self, _app: &tauri::AppHandle, ctx: &PluginContext) {
    let interval: u64 = ctx.settings.get("pollingIntervalMs")
        .unwrap_or(500);
}
```

`get::<T>(key)` returns `None` only if the key is absent or cannot be
deserialized into `T`. If you initialized defaults correctly, it will always
succeed.

**Stashing the handle:** Clone and stash `ctx.settings` if you need it beyond
`enable()`:

```rust
pub struct MyPlugin {
    settings: std::sync::OnceLock<PluginSettings>,
}

fn enable(&self, _app: &tauri::AppHandle, ctx: &PluginContext) {
    self.settings.set(ctx.settings.clone()).ok();
}
```

### Reacting to Changes

Instead of watch channels, the host delivers settings changes through
`setting_changed()`. The host uses a `CoalescingDispatcher` internally, so
rapid changes to the same key are deduplicated — only the most recent value is
delivered.

```rust
fn setting_changed(&self, key: &str, value: serde_json::Value) {
    match key {
        "pollingIntervalMs" => {
            if let Ok(secs) = serde_json::from_value::<u64>(value) {
                self.interval_secs.store(secs, Ordering::Relaxed);
            }
        }
        "feedUrl" => {
            if let Ok(url) = serde_json::from_value::<String>(value) {
                *self.feed_url.write().expect("feed_url not poisoned") = url;
            }
        }
        _ => {}
    }
}
```

`setting_changed()` is called on the host's settings-dispatch thread. Keep it
non-blocking: store the new value into an atomic or `RwLock` and let your
background thread pick it up on its next iteration.

---

## Frecency

Frecency (frequency + recency) boosts results the user selects most often.
The host automatically records a selection event every time `execute()` is
called and applies score bonuses to search results.

You rarely need to interact with frecency directly. The common cases:

**Use `top_items()` for empty-query ordering:**

When there is no query, show the user's most-used items first:

```rust
fn search(&self, query: &str, _prefix: Option<&str>) -> Option<PluginResponse> {
    if query.is_empty() {
        let top = self.frecency.read().unwrap().top_items(20);
        let results = top.into_iter()
            .filter_map(|item| self.find_entry(&item.item_id))
            .map(|entry| ScoredEntry { /* fields from entry */ })
            .collect();
        return Some(PluginResponse::Results(results));
    }
    // ... normal search
}
```

**Apply frecency bonuses to your own scored results:**

```rust
fn search(&self, query: &str, _prefix: Option<&str>) -> Option<PluginResponse> {
    let mut results = self.score_results(query);
    if let Some(frecency) = self.frecency.read().ok() {
        frecency.apply_scores(&mut results);
    }
    Some(PluginResponse::Results(results))
}
```

**Record custom UI interactions:**

The host records frecency on every `execute()` call. If your custom UI allows
selecting items without going through `execute()`, record them manually:

```rust
// Inside handle_message(), when the user picks an emoji via the grid UI:
self.frecency.record(&emoji_id);
```

**Stashing the handle:** Stash `ctx.frecency` during `enable()` exactly like
settings:

```rust
fn enable(&self, _app: &tauri::AppHandle, ctx: &PluginContext) {
    *self.frecency.write().expect("frecency not poisoned") = Some(ctx.frecency.clone());
}
```

`PluginFrecency` is `Clone` and bound to your plugin's ID — it cannot access
other plugins' data.

---

## Global Shortcuts

Plugins can register OS-level global shortcuts that work regardless of which
app has focus. Declare them in `shortcuts()`, handle activations in
`handle_shortcut()`.

```rust
fn shortcuts(&self) -> Vec<PluginShortcut> {
    vec![PluginShortcut {
        id: "open-history",          // Stable ID for routing
        label: "Open Clipboard History",
        default_shortcut: "CmdOrCtrl+Shift+V",
        settings_key: "shortcut.open-history",  // Plugin-scoped key
    }]
}

fn handle_shortcut(
    &self,
    shortcut_id: &str,
    _app: &tauri::AppHandle,
) -> anyhow::Result<PostAction> {
    match shortcut_id {
        "open-history" => Ok(PostAction::ShowCustomUI),
        _ => Ok(PostAction::Nothing),
    }
}
```

The actual key binding is read from `plugins.<id>.shortcut.open-history` in
the settings store, falling back to `default_shortcut` if absent. Users can
reconfigure it from the settings UI.

The host re-registers all shortcuts whenever any watched shortcut key changes,
so user customizations take effect immediately.

**`PostAction` from `handle_shortcut()`:**

| Value | Effect |
|---|---|
| `PostAction::Nothing` | No launcher state change |
| `PostAction::ShowCustomUI` | Show launcher + mount this plugin's custom UI |
| `PostAction::Dismiss` | Hide the launcher |
| `PostAction::KeepOpen` | Show launcher without changing its state |

---

## Custom UI

Two mechanisms let plugins go beyond the standard result list.

### Taking Over the Result Area

A plugin can return `Some(PluginResponse::CustomUI { view, data, results })`
from `search()` to signal that its React component should replace the result
list. The `results` are passed as props to the component — use them or ignore
them.

```rust
fn search(&self, query: &str, _prefix: Option<&str>) -> Option<PluginResponse> {
    let results = self.build_results(query);
    // Returning CustomUI mounts the plugin's React component.
    Some(PluginResponse::CustomUI {
        view: "emoji-picker".to_string(),
        data: serde_json::json!({ "query": query }),
        results,
    })
}
```

On the frontend, a React component is registered for the plugin's ID. The host
mounts it (not shows/hides — full mount/unmount) when a `CustomUI` response
arrives and unmounts it when the user dismisses the launcher or a different
plugin takes over.

Returning `PostAction::ShowCustomUI` from `handle_shortcut()` or `execute()`
opens the launcher and immediately mounts the plugin's custom UI:

```rust
fn execute(&self, entry_id: &str, _action: &ActionId, _app: &tauri::AppHandle)
    -> anyhow::Result<PostAction>
{
    match entry_id {
        "open-picker" => Ok(PostAction::ShowCustomUI),
        _ => Ok(PostAction::Dismiss),
    }
}
```

### Frontend ↔ Backend Messaging

When the custom UI component needs live data from the backend, it sends
messages through `sendMessage`, retrieved from the plugin context via
the `usePluginRuntime()` hook (see "Plugin Component Contract" below).
The host routes these to `handle_message()`.

The `channel` parameter enables streaming: the plugin holds the channel and
pushes updates asynchronously. The channel is dropped when the frontend
component unmounts, so you can use that as a stop signal.

```rust
fn handle_message(
    &self,
    method: &str,
    _payload: serde_json::Value,
    channel: tauri::ipc::Channel<serde_json::Value>,
) -> anyhow::Result<serde_json::Value> {
    match method {
        "subscribe-live-data" => {
            let rx = self.data_stream.subscribe();
            // Spawn a task that feeds the channel until it closes.
            tauri::async_runtime::spawn(async move {
                let mut rx = rx;
                loop {
                    match rx.recv().await {
                        Ok(item) => {
                            if channel.send(serde_json::to_value(item).unwrap()).is_err() {
                                break;  // Channel closed — component unmounted.
                            }
                        }
                        Err(_) => break,
                    }
                }
            });
            Ok(serde_json::json!({"ok": true}))
        }
        _ => anyhow::bail!("unknown method: {method}"),
    }
}
```

`handle_message()` returns a `serde_json::Value` as a one-shot response.
Ongoing updates go through the `channel`.

### Plugin Component Contract

Plugin React components (view, inline, settings) receive **only**
per-render data through their props. Identity, runtime capabilities,
launcher actions, and reactive setting accessors all flow through a
React context (ADR 0028).

The contract is exposed via four hooks. For host-tree components
(native plugin React) they live in `src/contexts/`; for WASM plugins
they're available via `@torchsnap/plugin-sdk/hooks`:

```tsx
import {
  usePluginInfo,
  usePluginRuntime,
  useLauncher,
  usePluginSetting,
} from "@torchsnap/plugin-sdk/hooks";

function MyView({ data, query }: PluginViewProps) {
  // Identity (always available)
  const { id, enabled } = usePluginInfo();

  // Runtime capabilities (always available)
  const { sendMessage, logger } = usePluginRuntime();

  // Launcher actions (only inside the launcher tree —
  // throws when called from a settings panel)
  const { dismiss, onExecute, onFooterChange } = useLauncher();

  // Reactive setting accessor — automatically scoped to
  // the active plugin's namespace
  const [retentionDays, setRetentionDays] =
    usePluginSetting<number>("retentionDays");

  // ...
}
```

Settings panels receive an empty props object — every value comes
from hooks:

```tsx
function MySettings() {
  const { enabled } = usePluginInfo();
  const [greeting, setGreeting] = usePluginSetting<string>("greeting");
  return (
    <input
      value={greeting}
      onChange={(e) => setGreeting(e.target.value)}
      disabled={!enabled}
    />
  );
}
```

Per-render data (`results`, `data`, `query`, `matchedPrefix`,
`selected`) stays as props on `PluginViewProps` / `InlineViewProps`
because it changes every keystroke and would invalidate the context
value if hoisted.

Sub-components extracted from a plugin component can call any of the
four hooks directly — no prop threading required.

### Scheduled background tasks (WASM plugins, ADR 0032)

WASM plugins can't spawn their own threads. The host runs a per-plugin
tokio scheduler driven by `[[tasks]]` entries in `manifest.toml`:

```toml
[[tasks]]
id = "retention-cleanup"
schedule = "*/30 * * * *"
```

`schedule` is a 5-field POSIX cron expression
(`minute hour day month weekday`). Sub-minute scheduling is not
supported.

The plugin implements the `tasks::run-task` guest export and
dispatches by `task_id`:

```rust
use exports::torchsnap::plugin::tasks::Guest as TasksGuest;
use torchsnap::plugin::sql;

impl TasksGuest for MyPlugin {
    fn run_task(task_id: String) -> Result<(), String> {
        match task_id.as_str() {
            "retention-cleanup" => {
                let db = sql::connection();
                db.execute(
                    "DELETE FROM history WHERE created_at < datetime('now', '-30 days')",
                    &[],
                ).map_err(|e| format!("delete: {e}"))?;
                Ok(())
            }
            other => Err(format!("unknown task: {other}")),
        }
    }
}
```

Key semantics:

- **First fire is at the next cron match.** No automatic immediate
  fire on enable. Plugins that want startup work should do it in
  their own `enable()`.
- **Failures are logged, not fatal.** Returning `Err(string)` from
  `run-task` is logged at error level by the host. The plugin is
  not auto-disabled. Both wasmtime traps (panics) and plugin-reported
  errors flow through the same logging path.
- **Missed fires across launcher restarts are not made up.** If the
  launcher is closed for 4 hours and a task was supposed to fire 8
  times, it does not "make up" the missed fires on next launch.
- **Sequential execution within the plugin.** The wasmtime store
  mutex serializes every guest call, so two tasks scheduled at the
  same instant run back-to-back in manifest declaration order — not
  concurrently. There's no need for the plugin to coordinate access
  to its own state across tasks.
- **Plugins without `[[tasks]]` pay zero cost.** The host never
  spawns a scheduler loop when the manifest declares no tasks. Every
  plugin still has to implement `TasksGuest::run_task` (it's a WIT
  contract) but a one-line `Ok(())` stub is enough.

Schedule and id validation happens at manifest parse time. A
malformed cron expression or a duplicate task id fails plugin load
with a clear error.

### Per-plugin SQL storage (WASM plugins, ADR 0031)

WASM plugins get an isolated SQLite database scoped to
`<app_data_dir>/plugin-home/<plugin-id>/sql/storage.sqlite3`. Schema
migrations are declared in `manifest.toml` as a list of file paths
relative to the plugin root:

```toml
[storage.sql]
migrations = [
    "migrations/001_init.sql",
]
```

Each `.sql` file is plain SQL — diffable, syntax-highlighted, and
shared with your unit tests via `include_str!`. The host creates the
database file and applies migrations before the guest's `enable()`
runs — by the time the plugin's own code executes, the schema is
ready.

The plugin obtains a connection handle via the WIT host import:

```rust
use torchsnap::plugin::sql;

let db = sql::connection();

db.execute("INSERT INTO log (msg) VALUES (?)", &[
    sql::SqlValue::Text("hello".into()),
])?;

let rows = db.query("SELECT id, msg FROM log ORDER BY id DESC LIMIT 10", &[])?;
for row in rows {
    let sql::SqlValue::Integer(id) = row[0] else { continue };
    let sql::SqlValue::Text(ref msg) = row[1] else { continue };
    println!("{id}: {msg}");
}
```

Key points:

- **Host-managed initialization** — the host creates the database
  file and runs migrations during `enable()`, before the guest's
  `enable()` runs. Plugins that never declare `[storage.sql]` get no
  file on disk.
- **Infallible connection** — `sql::connection()` always succeeds for
  plugins that declare `[storage.sql]`. Each call returns a fresh
  handle backed by the same underlying connection; the connection's
  internal mutex serializes execution.
- **Drop semantics** — letting the handle go out of scope releases
  just that handle. The cached `Arc<SqlStorage>` lives until the
  plugin is disabled.
- **No `IN (?)` expansion** — the WIT `sql-value` variant intentionally
  omits the host's `List` variant. Plugins build their own `IN (?, ?,
  ?)` clauses (one `?` per element) before calling `query`/`execute`.
- **Transactions are not yet exposed.** Calculator-style two-statement
  inserts are fine without; if a plugin genuinely needs save-pointed
  transactions, raise the API gap.

Migration file errors (missing file, malformed SQL, migration apply
failure) surface as a host-side `enable()` error — the guest never
sees them. Per-statement errors come back from `execute` / `query`.

### Opener (WASM plugins, ADR 0037)

WASM plugins can open URLs in the OS default handler (browser, mail client,
etc.) via the `opener` host import. Declare which URL schemes the plugin is
allowed to open under `[permissions.opener]` in `manifest.toml`:

```toml
[permissions.opener]
schemes = ["https", "http"]
```

Omitting `[permissions.opener]` entirely means the plugin has no opener
access. The host enforces the scheme allowlist at call time — any scheme not
listed returns an error without invoking the OS opener.

Call `open_url` from any guest entry point:

```rust
use torchsnap::plugin::opener;

fn open_result_url(url: &str) -> Result<(), String> {
    opener::open_url(url)
}
```

Key points:

- **Deny by default** — no `[permissions.opener]` section = no `open-url`
  access, regardless of what the plugin calls.
- **Error string** — on scheme rejection the host returns
  `"scheme not permitted: {scheme}"`. On OS opener failure the host returns
  the backend's error message.
- **`reveal-path` is not yet exposed.** Opening a directory in Finder /
  Explorer is deferred until the app-launcher plugin conversion.
- **WASM-only.** Native plugins call Tauri's `tauri-plugin-opener` directly.

### HTTP (WASM plugins, ADR 0038)

WASM plugins can make synchronous HTTP requests via the `http` host import.
Declare which origins the plugin is allowed to reach under
`[permissions.http]` in `manifest.toml`:

```toml
[permissions.http]
origins = ["https://api.duckduckgo.com"]

# or trust-all:
origins = ["*"]
```

`"*"` opts the plugin into trust-all mode. Omitting `[permissions.http]`
entirely means the plugin has no HTTP access. Declared origins are normalized
to their ASCII-serialized form at manifest parse time, so a misconfigured
origin (invalid URL) fails plugin load with a clear error rather than
silently blocking requests at runtime.

Build a request and call `fetch`:

```rust
use torchsnap::plugin::http::{self, HttpMethod, HttpRequest};

fn lookup(query: &str) -> Result<Vec<u8>, String> {
    let url = format!("https://api.duckduckgo.com/?q={query}&format=json");
    let request = HttpRequest {
        url,
        method: HttpMethod::Get,
        headers: vec![],
        body: None,
        timeout_ms: Some(5_000),
        max_body_size: None,
    };

    let response = http::fetch(&request).map_err(|e| match e {
        http::HttpError::PermissionDenied(msg) => format!("permission denied: {msg}"),
        http::HttpError::Network(msg) => format!("network error: {msg}"),
        http::HttpError::Timeout => "request timed out".to_string(),
    })?;

    Ok(response.body)
}
```

Key points:

- **Deny by default** — no `[permissions.http]` section = no HTTP access.
- **Three error variants** — `permission-denied(string)` means the request's
  origin is not in the allowlist (check the manifest); `network(string)` means
  any transport failure (DNS, TLS, connection refused); `timeout` means the
  request exceeded the configured or host-default deadline.
- **`timeout-ms` and `max-body-size` are optional guards.** `none` delegates
  to the host default. Override `timeout-ms` for latency-sensitive calls;
  override `max-body-size` when the response body is expected to be large.
- **HTTP status codes are not errors.** A 4xx or 5xx response arrives as a
  normal `HttpResponse` — the plugin inspects `response.status` and decides
  what to do.
- **Blocking call.** The host runs the request on a thread-pool thread via
  `tokio::task::block_in_place`. The guest blocks for the full round-trip.
- **WASM-only.** Native plugins call `reqwest` or `ureq` directly.

---

## How your plugin is discovered (WASM plugins, ADR 0035)

Torchsnap scans three roots at startup, in precedence order:

1. **System** — `<resource_dir>/plugins/`. Shipped inside the
   application bundle. Populated at build time from plugins listed
   in `plugins/bundled.toml`.
2. **Dev** — `<CARGO_MANIFEST_DIR>/../plugins/`. Scanned only in
   debug builds; release builds never touch this root. Your plugin
   source checkout lives here during development, and the loader
   picks it up automatically.
3. **User** — `<app_data_dir>/plugins/`. Where user-installed
   `.torchsnap` archives land when users install via the Plugins
   settings panel.

A plugin id can only be registered once; the first root to claim a
given id wins. The install flow rejects colliding ids up front, so
in practice cross-root collisions only come up in development (e.g.
a user plugin with the same id as a dev checkout).

Each root accepts both `.torchsnap` archives and directory-form
plugins (`<id>/` containing `manifest.toml` at its top). Inside a
single root, an archive (`foo.torchsnap`) wins over a sibling
directory (`foo/`) — handy when you want to keep a working-tree
copy next to a published artifact.

The host tags each loaded plugin with a `PluginSourceKind`
(`system`, `dev`, `user`, or the built-in `builtin` for native
Rust plugins). The Plugins settings panel renders this as a badge
next to each plugin and gates the uninstall action to `user` only.

---

## Packaging and distributing your plugin (WASM plugins, ADR 0035)

A `.torchsnap` file is a plain zip archive containing your plugin's
`manifest.toml`, compiled `.wasm`, bundled frontend assets, and any
SQL migration files. Produce one with:

```bash
just build-plugin <your-plugin>
# or, if the build step has already run:
just package-plugin <your-plugin>
```

The output lands at `plugins/<your-plugin>.torchsnap`.

**Getting your plugin into a Torchsnap build:**

- *Ship with the app* (first-party only): add your plugin id to
  `plugins/bundled.toml` and rebuild. Tauri picks up the archive as
  a resource through the `stage-bundled-plugins` Just recipe and
  embeds it in `Contents/Resources/plugins/` (macOS) or the
  equivalent on other platforms.

- *Share with users* (third-party): send them the
  `.torchsnap` file. They drop it onto the Plugins settings panel
  (file picker or drag-drop); the app copies it to
  `<app_data_dir>/plugins/<id>.torchsnap` and prompts for a restart.

**Manifest path rules.** Every path your manifest references
(`plugin.wasm`, asset-form `icon`, `frontend.*-bundle`,
`frontend.*-css`, `storage.sql.migrations[*]`) must be
plugin-relative: no `..` segments, no leading `/`, no backslashes,
no Windows drive letters. A manifest that violates any of these
fails to parse and the plugin never loads. Plan your asset layout
inside the archive accordingly — everything stays under the
archive root.

**Trust model.** There is no archive signing today. The sandbox is
the WIT capability surface (no network imports, no arbitrary file
reads, write-only clipboard). Users trust the source they got the
archive from; the host limits what the plugin can reach beyond
that. See ADR 0036 for the full rationale and the triggers for
revisiting.

---

## Types Reference

### `CatalogEntry`

Raw entry produced by `Plugin::entries()`. The host scores and converts it to
a `SourcedEntry` before sending to the frontend.

| Field | Type | Description |
|---|---|---|
| `id` | `String` | Stable identifier, echoed back in `execute()` |
| `title` | `String` | Primary display text; the main match target |
| `subtitle` | `Option<String>` | Secondary line shown below the title |
| `icon` | `Option<EntryIcon>` | Icon specification |
| `keywords` | `Vec<String>` | Extra match targets (not highlighted) |
| `actions` | `Vec<Action>` | Ordered actions; first is primary (Enter) |

### `ScoredEntry`

Pre-scored result returned by `Plugin::search()`.

| Field | Type | Description |
|---|---|---|
| `id` | `String` | Stable identifier |
| `title` | `String` | Primary display text |
| `subtitle` | `Option<String>` | Secondary line |
| `icon` | `Option<EntryIcon>` | Icon specification |
| `score` | `u32` | Sort key; higher scores rank higher |
| `title_positions` | `Vec<u32>` | Character offsets to highlight in `title` |
| `subtitle_positions` | `Vec<u32>` | Character offsets to highlight in `subtitle` |
| `actions` | `Vec<Action>` | Ordered actions |

### `EntryIcon`

```rust
pub enum EntryIcon {
    HeroIcon(String),    // Heroicon name, e.g. "x-circle"
    DataUrl(String),     // Base64 data URL
    AssetIcon(String),   // Absolute path to a cached file (WebP, PNG, etc.)
    Emoji(String),       // A single Unicode emoji character
}
```

### `Action` and `ActionId`

```rust
pub struct Action {
    pub id: ActionId,
    pub label: String,
    pub keybinding: Option<ActionKeybinding>,
}

pub enum ActionId {
    Open,
    Copy,
    Reveal,
    OpenWith,
    Delete,
    Custom(String),   // Plugin-defined action ID
}
```

Use well-known `ActionId` variants for standard actions — the frontend maps
these to default keybindings and icons automatically. Use `Custom("my-action")`
for anything else.

`ActionKeybinding` declares the key hint displayed in the action palette:

```rust
pub struct ActionKeybinding {
    pub modifiers: Vec<String>,  // e.g. ["Meta", "Shift"]
    pub key: String,             // e.g. "Enter", "c"
}
```

### `PostAction`

Returned from `execute()`, `handle_shortcut()`.

| Variant | Effect |
|---|---|
| `Nothing` | No launcher state change |
| `Dismiss` | Hide the launcher |
| `KeepOpen` | Keep the launcher visible |
| `ShowCustomUI` | Mount this plugin's custom UI component |

### `PluginResponse`

Returned from `Plugin::search()` as `Option<PluginResponse>`. Return `None`
to indicate the plugin has nothing to contribute (ADR 0021).

| Variant | Effect |
|---|---|
| `Results(Vec<ScoredEntry>)` | Standard list rendering by the host |
| `CustomUI { view, data, results }` | Mount plugin's custom React component; results passed as props |
| `InlineUI { view, data, results }` | Render plugin component above the result list; results merged into host list |

### `PluginContext`

Injected into `enable()`. All fields are `Clone`.

| Field | Type | Description |
|---|---|---|
| `settings` | `PluginSettings` | Scoped read access to `plugins.<id>.*` |
| `frecency` | `PluginFrecency` | Frecency scores and top-item queries |

---

## Registering a Plugin

All plugins are registered manually in `src-tauri/src/lib.rs` inside the
`setup` closure, before `host.initialize_and_start()`:

```rust
host.register(Box::new(plugins::my_plugin::MyPlugin::new()));
```

Add your module to `src-tauri/src/plugins/mod.rs`:

```rust
pub mod my_plugin;
```

---

## Full Examples

### Static Command List

A simple catalog plugin with hardcoded actions and icons.

```rust
use crate::plugins::{Plugin, PluginContext};
use crate::search::types::{Action, ActionId, ActionKeybinding, CatalogEntry, EntryIcon, PostAction};

pub struct DevToolsPlugin;

impl Plugin for DevToolsPlugin {
    fn id(&self) -> &str {
        "dev-tools"
    }

    fn entries(&self) -> Vec<CatalogEntry> {
        vec![
            CatalogEntry {
                id: "clear-cache".to_string(),
                title: "Clear Application Cache".to_string(),
                subtitle: Some("Removes all cached data".to_string()),
                icon: Some(EntryIcon::HeroIcon("trash".to_string())),
                keywords: vec!["purge".to_string(), "reset".to_string()],
                actions: vec![Action {
                    id: ActionId::Open,
                    label: "Clear".to_string(),
                    keybinding: Some(ActionKeybinding {
                        modifiers: vec![],
                        key: "Enter".to_string(),
                    }),
                }],
            },
            CatalogEntry {
                id: "open-logs".to_string(),
                title: "Open Log Directory".to_string(),
                subtitle: None,
                icon: Some(EntryIcon::HeroIcon("folder-open".to_string())),
                keywords: vec!["logs".to_string(), "debug".to_string()],
                actions: vec![
                    Action {
                        id: ActionId::Open,
                        label: "Open".to_string(),
                        keybinding: None,
                    },
                    Action {
                        id: ActionId::Reveal,
                        label: "Reveal in Finder".to_string(),
                        keybinding: None,
                    },
                ],
            },
        ]
    }

    fn execute(
        &self,
        entry_id: &str,
        action_id: &ActionId,
        app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        match (entry_id, action_id) {
            ("clear-cache", ActionId::Open) => {
                clear_cache(app).context("clearing application cache")?;
                Ok(PostAction::Dismiss)
            }
            ("open-logs", ActionId::Open) => {
                open_log_dir(app).context("opening log directory")?;
                Ok(PostAction::Dismiss)
            }
            ("open-logs", ActionId::Reveal) => {
                reveal_log_dir(app).context("revealing log directory")?;
                Ok(PostAction::Dismiss)
            }
            _ => anyhow::bail!("unhandled: {entry_id} / {action_id:?}"),
        }
    }
}
```

### Prefix-Routed Query Plugin

A query-only plugin that activates exclusively when the user types `>`:

```rust
use std::sync::RwLock;

use anyhow::Context as _;

use crate::plugins::{Plugin, PluginContext};
use crate::search::types::{Action, ActionId, PostAction, PluginResponse, ScoredEntry};

pub struct ScriptRunnerPlugin {
    scripts: RwLock<Vec<Script>>,
}

impl ScriptRunnerPlugin {
    pub fn new() -> Self {
        Self { scripts: RwLock::new(Vec::new()) }
    }
}

impl Plugin for ScriptRunnerPlugin {
    fn id(&self) -> &str {
        "script-runner"
    }

    // Only activated when the user types ">".
    fn search_prefixes(&self) -> &[String] {
        &[">".to_string()]
    }

    fn enable(&self, _app: &tauri::AppHandle, _ctx: &PluginContext) {
        // Load scripts from disk on startup.
        let mut scripts = self.scripts.write().expect("scripts not poisoned");
        *scripts = load_scripts_from_disk();
    }

    fn search(&self, query: &str, _matched_prefix: Option<&str>) -> Option<PluginResponse> {
        let scripts = self.scripts.read().expect("scripts not poisoned");
        let results = scripts.iter()
            .filter(|s| s.name.to_lowercase().contains(&query.to_lowercase()))
            .map(|s| ScoredEntry {
                id: s.id.clone(),
                title: s.name.clone(),
                subtitle: Some(s.description.clone()),
                icon: None,
                score: 100,
                title_positions: vec![],
                subtitle_positions: vec![],
                actions: vec![Action {
                    id: ActionId::Open,
                    label: "Run".to_string(),
                    keybinding: None,
                }],
            })
            .collect();

        Some(PluginResponse::Results(results))
    }

    fn execute(
        &self,
        entry_id: &str,
        _action_id: &ActionId,
        _app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        run_script(entry_id).context("running script")?;
        Ok(PostAction::Dismiss)
    }
}
```

### Reactive Background Plugin

A plugin that polls external data, respects the host-managed enable/disable
toggle, and reacts to a configurable interval — all without blocking the main
thread.

```rust
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use anyhow::Context as _;

use crate::plugins::{Plugin, PluginContext};
use crate::search::types::{Action, ActionId, CatalogEntry, PostAction};
use crate::settings::SettingsInit;

pub struct FeedPlugin {
    items: Arc<RwLock<Vec<FeedItem>>>,
    // Shared with the background thread so disable() can signal it to exit.
    stop: Arc<AtomicBool>,
    // Updated by setting_changed() and read by the background thread.
    poll_interval_secs: Arc<AtomicU64>,
    feed_url: Arc<RwLock<String>>,
}

impl FeedPlugin {
    pub fn new() -> Self {
        Self {
            items: Arc::new(RwLock::new(Vec::new())),
            stop: Arc::new(AtomicBool::new(false)),
            poll_interval_secs: Arc::new(AtomicU64::new(300)),
            feed_url: Arc::new(RwLock::new(String::new())),
        }
    }
}

impl Plugin for FeedPlugin {
    fn id(&self) -> &str {
        "rss-feed"
    }

    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit {
        settings
            .ensure("feedUrl", "https://example.com/feed.xml")
            .ensure("pollIntervalSecs", 300u64)
    }

    fn enable(&self, _app: &tauri::AppHandle, ctx: &PluginContext) {
        // Load initial config into shared state so the background thread and
        // setting_changed() both operate on the same data.
        let feed_url: String = ctx.settings.get("feedUrl").unwrap_or_default();
        let interval_secs: u64 = ctx.settings.get("pollIntervalSecs").unwrap_or(300);

        *self.feed_url.write().expect("feed_url not poisoned") = feed_url.clone();
        self.poll_interval_secs.store(interval_secs, Ordering::Relaxed);

        // Reset the stop flag in case this is a re-enable after a disable.
        self.stop.store(false, Ordering::Relaxed);

        let items = Arc::clone(&self.items);
        let stop = Arc::clone(&self.stop);
        let poll_interval_secs = Arc::clone(&self.poll_interval_secs);
        let feed_url = Arc::clone(&self.feed_url);

        thread::spawn(move || {
            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }

                let url = feed_url.read().expect("feed_url not poisoned").clone();
                if let Ok(fetched) = fetch_feed(&url) {
                    *items.write().expect("items not poisoned") = fetched;
                }

                // Sleep in small ticks so we respond to stop signals and
                // interval changes promptly, without holding any locks.
                let interval = poll_interval_secs.load(Ordering::Relaxed);
                for _ in 0..interval {
                    thread::sleep(Duration::from_secs(1));
                    if stop.load(Ordering::Relaxed) {
                        return;
                    }
                }
            }
        });
    }

    fn disable(&self) {
        // Signal the background thread spawned in enable() to exit.
        self.stop.store(true, Ordering::SeqCst);
    }

    fn setting_changed(&self, key: &str, value: serde_json::Value) {
        // The host coalesces rapid changes to the same key, so each call here
        // carries the latest value. Store it; the background thread picks it
        // up on its next iteration.
        match key {
            "pollIntervalSecs" => {
                if let Ok(secs) = serde_json::from_value::<u64>(value) {
                    self.poll_interval_secs.store(secs, Ordering::Relaxed);
                }
            }
            "feedUrl" => {
                if let Ok(url) = serde_json::from_value::<String>(value) {
                    *self.feed_url.write().expect("feed_url not poisoned") = url;
                }
            }
            _ => {}
        }
    }

    fn entries(&self) -> Vec<CatalogEntry> {
        let Ok(items) = self.items.read() else { return vec![] };
        items.iter().map(|item| CatalogEntry {
            id: item.url.clone(),
            title: item.title.clone(),
            subtitle: item.description.clone(),
            icon: None,
            keywords: vec![],
            actions: vec![Action {
                id: ActionId::Open,
                label: "Open".to_string(),
                keybinding: None,
            }],
        }).collect()
    }

    fn execute(
        &self,
        entry_id: &str,
        _action_id: &ActionId,
        _app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        // entry_id is the URL stored in CatalogEntry::id above.
        open::that(entry_id).context("opening feed item URL")?;
        Ok(PostAction::Dismiss)
    }
}
```

**What this example demonstrates:**

- `initialize_settings()` — declares typed defaults without an `enabled` key
  (the host manages that).
- `enable()` — loads initial config, resets the stop flag so re-enable works
  correctly, then spawns a long-lived background thread.
- `disable()` — signals the background thread to exit cleanly.
- `setting_changed()` — reacts to user configuration changes dispatched by the
  host's `CoalescingDispatcher`; stores new values into atomics / `RwLock`s
  so the background thread picks them up on its next iteration.
- `entries()` — defensively handles a not-yet-populated `RwLock`.
