# Plugin Development Guide

This guide walks you through building a Torchsnap plugin from scratch. It covers
both plugin types, the full lifecycle, every API surface available to plugins,
and progressively more complex examples.

## Contents

- [Concepts](#concepts)
- [Quick Start: Minimal Catalog Plugin](#quick-start-minimal-catalog-plugin)
- [Plugin Types](#plugin-types)
  - [CatalogPlugin](#catalogplugin)
  - [QueryPlugin](#queryplugin)
- [Lifecycle](#lifecycle)
- [Settings](#settings)
  - [Declaring Defaults](#declaring-defaults)
  - [Reading at Runtime](#reading-at-runtime)
  - [Reactive Notifications](#reactive-notifications)
- [Frecency](#frecency)
- [Global Shortcuts](#global-shortcuts)
- [Custom UI](#custom-ui)
  - [Taking Over the Result Area](#taking-over-the-result-area)
  - [Frontend ↔ Backend Messaging](#frontend--backend-messaging)
- [Types Reference](#types-reference)
- [Registering a Plugin](#registering-a-plugin)
- [Full Examples](#full-examples)
  - [Static Command List](#static-command-list)
  - [Prefix-Routed Query Plugin](#prefix-routed-query-plugin)
  - [Reactive Background Plugin](#reactive-background-plugin)

---

## Concepts

Torchsnap's search pipeline is built around plugins. Each plugin is a Rust
struct that implements one of two traits:

| Trait | Search model | When to use |
|---|---|---|
| `CatalogPlugin` | Returns a static list of entries. The host filters with [nucleo](https://github.com/helix-editor/nucleo). | App launchers, command palettes, static lists |
| `QueryPlugin` | Receives the raw query and returns pre-scored results. | Custom matching logic, dynamic results, prefix-gated modes |

Both trait types share the same lifecycle, shortcut API, and message API.
The only difference is how they participate in search.

All plugins are internal (compiled into the binary). There is no dynamic
loading at present — each plugin is registered at startup in `lib.rs`.

---

## Quick Start: Minimal Catalog Plugin

The smallest useful plugin: a hardcoded command list.

```rust
// src-tauri/src/plugins/my_commands.rs

use anyhow::Context as _;
use tauri::Manager;

use crate::plugins::{CatalogPlugin, PluginContext};
use crate::search::types::{Action, ActionId, CatalogEntry, PostAction};

pub struct MyCommandsPlugin;

impl CatalogPlugin for MyCommandsPlugin {
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

### CatalogPlugin

A `CatalogPlugin` owns a set of entries. The host handles all filtering — the
plugin never sees the query string.

```rust
pub trait CatalogPlugin: Send + Sync {
    fn id(&self) -> &str;
    fn is_enabled(&self) -> bool { true }
    fn enabled_settings_key(&self) -> Option<&'static str> { None }
    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit { settings }
    fn setup(&self, _app: &tauri::AppHandle, _ctx: &PluginContext) {}
    fn teardown(&self) {}
    fn entries(&self) -> Vec<CatalogEntry>;
    fn execute(&self, entry_id: &str, action_id: &ActionId, app: &tauri::AppHandle)
        -> anyhow::Result<PostAction>;
    fn shortcuts(&self) -> Vec<PluginShortcut> { vec![] }
    fn handle_shortcut(&self, _shortcut_id: &str, _app: &tauri::AppHandle)
        -> anyhow::Result<PostAction> { Ok(PostAction::Nothing) }
    fn handle_message(&self, _method: &str, _payload: serde_json::Value,
        _channel: tauri::ipc::Channel<serde_json::Value>)
        -> anyhow::Result<serde_json::Value> { bail!("...") }
}
```

**Key points for `entries()`:**

- Called on every search keystroke — keep it fast.
- May be called before `setup()` finishes. Return an empty `Vec` in that case.
- Thread safety: `entries()` takes `&self`, so internal state must use
  `RwLock`, `Mutex`, or atomics.

**`entry_id` in `execute()`:**

The `entry_id` is whatever you put in `CatalogEntry::id`. It is your
responsibility to make these stable and unique within the plugin.

### QueryPlugin

A `QueryPlugin` receives the raw query and scores its own results.

```rust
pub trait QueryPlugin: Send + Sync {
    fn id(&self) -> &str;
    fn is_enabled(&self) -> bool { true }
    fn enabled_settings_key(&self) -> Option<&'static str> { None }
    fn prefixes(&self) -> &[&str] { &[] }
    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit { settings }
    fn setup(&self, _app: &tauri::AppHandle, _ctx: &PluginContext) {}
    fn teardown(&self) {}
    fn search(&self, query: &str, matched_prefix: Option<&str>) -> SearchResponse;
    fn execute(&self, entry_id: &str, action_id: &ActionId, app: &tauri::AppHandle)
        -> anyhow::Result<PostAction>;
    fn shortcuts(&self) -> Vec<PluginShortcut> { vec![] }
    fn handle_shortcut(&self, _shortcut_id: &str, _app: &tauri::AppHandle)
        -> anyhow::Result<PostAction> { Ok(PostAction::Nothing) }
    fn handle_message(&self, _method: &str, _payload: serde_json::Value,
        _channel: tauri::ipc::Channel<serde_json::Value>)
        -> anyhow::Result<serde_json::Value> { bail!("...") }
}
```

**Prefix routing (ADR 0012):**

If your plugin registers prefixes via `prefixes()`, it receives exclusive
routing when the user types a matching prefix. All catalog plugins and
prefix-less query plugins are bypassed entirely.

```rust
fn prefixes(&self) -> &[&str] {
    &[":"]          // Exclusive when query starts with ":"
}
```

When exclusive routing fires:
- The prefix is stripped before `search()` is called.
- `matched_prefix` is `Some(":") ` — useful when a plugin registers multiple
  prefixes and needs to know which triggered.
- Longest prefix wins when multiple prefixes could match.

**Always-on query plugins:**

Return an empty `prefixes()` slice (the default) to run on every query
alongside catalog plugins. Results are merged and sorted by score.

**`search()` return value:**

Return `SearchResponse::Results(vec)` for standard list rendering, or
`SearchResponse::CustomUI(vec)` to take over the result area with a custom
frontend component (see [Custom UI](#custom-ui)).

---

## Lifecycle

Plugin lifecycle has four ordered phases.

### Phase 1 — Settings initialization (synchronous, at registration)

`initialize_settings()` is called synchronously for every plugin before any
`setup()` call. Use it to ensure defaults exist in the settings store.

```rust
fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit {
    settings
        .ensure("enabled", true)
        .ensure("maxResults", 20)
}
```

Values already in the store are never overwritten — `ensure` only fills gaps.
See [Settings → Declaring Defaults](#declaring-defaults) for migrations.

### Phase 2 — Parallel setup (background thread pool)

`setup()` runs on a rayon background thread pool (sized to ~70 % of available
cores). All plugins' `setup()` calls run in parallel.

```rust
fn setup(&self, app: &tauri::AppHandle, ctx: &PluginContext) {
    // Safe to block — this is a background thread, not the Tokio runtime.
    let apps = self.discovery.discover();
    *self.cache.write().expect("cache not poisoned") = apps;
}
```

**What you can do in `setup()`:**
- Block on I/O (filesystem scan, subprocess, HTTP request).
- Spawn background threads that run for the plugin's lifetime.
- Read settings via `ctx.settings`.
- Subscribe to settings changes via `ctx.notifier` to drive background threads.
- Clone and stash `ctx.frecency` for later use in `entries()` or `search()`.

**Important:** `entries()` and `search()` may be called before `setup()`
completes. Guard cached state with `RwLock::try_read()` or return an empty
list.

### Phase 3 — Shortcut registration

After all `setup()` calls complete, the host registers all plugin shortcuts
with the OS. A reactor task watches for settings changes that affect shortcut
key bindings and re-registers when needed.

### Phase 4 — Teardown (on `RunEvent::Exit`)

`teardown()` is called once for every plugin on application exit.

```rust
fn teardown(&self) {
    // Signal the background thread to stop.
    self.stop_signal.store(true, Ordering::SeqCst);
}
```

Teardown must complete quickly. Blocking indefinitely will delay shutdown.

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
        .ensure("enabled", true)
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

`ctx.settings` is a `PluginSettings` handle injected into `setup()`. It
provides scoped read access to `plugins.<id>.*`:

```rust
fn setup(&self, _app: &tauri::AppHandle, ctx: &PluginContext) {
    let interval: u64 = ctx.settings.get("pollingIntervalMs")
        .unwrap_or(500);
    let enabled: bool = ctx.settings.get("enabled")
        .unwrap_or(true);
}
```

`get::<T>(key)` returns `None` only if the key is absent or cannot be
deserialized into `T`. If you initialized defaults correctly, it will always
succeed.

**Stashing the handle:** Clone and stash `ctx.settings` if you need it beyond
`setup()`:

```rust
pub struct MyPlugin {
    settings: std::sync::OnceLock<PluginSettings>,
}

fn setup(&self, _app: &tauri::AppHandle, ctx: &PluginContext) {
    self.settings.set(ctx.settings.clone()).ok();
}
```

### Reactive Notifications

`ctx.notifier` is a `PluginSettingsNotifier` that delivers settings changes
as typed watch channels. Use this to react to user configuration changes at
runtime without polling.

```rust
fn setup(&self, _app: &tauri::AppHandle, ctx: &PluginContext) {
    // Subscribe before spawning the thread so no update is missed.
    let mut enabled_watch: SettingsWatch<bool> = ctx.notifier.watch("enabled");

    let stop = Arc::clone(&self.stop_signal);
    thread::spawn(move || {
        loop {
            // Block until "enabled" changes, or None if app is shutting down.
            match enabled_watch.blocking_changed() {
                Some(true) => { /* start work */ }
                Some(false) => { /* pause work */ }
                None => break,  // Notifier dropped — exit cleanly.
            }
        }
    });
}
```

`SettingsWatch<T>` has three methods:

| Method | Use |
|---|---|
| `get() -> T` | Read the current value synchronously |
| `async changed() -> Option<T>` | Await the next change (async context) |
| `blocking_changed() -> Option<T>` | Wait for next change (blocking thread) |

`watch(key)` reads the initial value from the store automatically, so you do
not need to call `settings.get()` separately for the initial state.

---

## Frecency

Frecency (frequency + recency) boosts results the user selects most often.
The host automatically records a selection event every time `execute()` is
called and applies score bonuses to search results.

You rarely need to interact with frecency directly. The common cases:

**Use `top_items()` for empty-query ordering:**

When there is no query, show the user's most-used items first:

```rust
fn search(&self, query: &str, _prefix: Option<&str>) -> SearchResponse {
    if query.is_empty() {
        let top = self.frecency.read().unwrap().top_items(20);
        let results = top.into_iter()
            .filter_map(|item| self.find_entry(&item.item_id))
            .map(|entry| ScoredEntry { /* fields from entry */ })
            .collect();
        return SearchResponse::Results(results);
    }
    // ... normal search
}
```

**Apply frecency bonuses to your own scored results:**

```rust
fn search(&self, query: &str, _prefix: Option<&str>) -> SearchResponse {
    let mut results = self.score_results(query);
    if let Some(frecency) = self.frecency.read().ok() {
        frecency.apply_scores(&mut results);
    }
    SearchResponse::Results(results)
}
```

**Record custom UI interactions:**

The host records frecency on every `execute()` call. If your custom UI allows
selecting items without going through `execute()`, record them manually:

```rust
// Inside handle_message(), when the user picks an emoji via the grid UI:
self.frecency.record(&emoji_id);
```

**Stashing the handle:** Stash `ctx.frecency` during `setup()` exactly like
settings:

```rust
fn setup(&self, _app: &tauri::AppHandle, ctx: &PluginContext) {
    *self.frecency.write().unwrap() = Some(ctx.frecency.clone());
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

A `QueryPlugin` can return `SearchResponse::CustomUI(results)` to signal that
its React component should replace the result list. The results are passed as
props to the component — use them or ignore them.

```rust
fn search(&self, query: &str, _prefix: Option<&str>) -> SearchResponse {
    let results = self.build_results(query);
    // Returning CustomUI mounts the plugin's React component.
    SearchResponse::CustomUI(results)
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
messages through a `sendMessage` prop. The host routes these to
`handle_message()`.

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

---

## Types Reference

### `CatalogEntry`

Raw entry produced by `CatalogPlugin::entries()`. The host scores and converts
it to `SourcedEntry` before sending to the frontend.

| Field | Type | Description |
|---|---|---|
| `id` | `String` | Stable identifier, echoed back in `execute()` |
| `title` | `String` | Primary display text; the main match target |
| `subtitle` | `Option<String>` | Secondary line shown below the title |
| `icon` | `Option<EntryIcon>` | Icon specification |
| `keywords` | `Vec<String>` | Extra match targets (not highlighted) |
| `actions` | `Vec<Action>` | Ordered actions; first is primary (Enter) |

### `ScoredEntry`

Pre-scored result returned by `QueryPlugin::search()`.

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

### `SearchResponse`

Returned from `QueryPlugin::search()`.

| Variant | Effect |
|---|---|
| `Nothing` | Plugin has nothing to contribute; host skips it entirely |
| `Results(Vec<ScoredEntry>)` | Standard list rendering by the host |
| `CustomUI { view, data, results }` | Mount plugin's custom React component; results passed as props |
| `InlineUI { view, data, results }` | Render plugin component above the result list; results merged into host list |

### `PluginContext`

Injected into `setup()`. All fields are `Clone`.

| Field | Type | Description |
|---|---|---|
| `settings` | `PluginSettings` | Scoped read access to `plugins.<id>.*` |
| `notifier` | `PluginSettingsNotifier` | Subscribe to settings changes |
| `frecency` | `PluginFrecency` | Frecency scores and top-item queries |

---

## Registering a Plugin

All plugins are registered manually in `src-tauri/src/lib.rs` inside the
`setup` closure, before `host.initialize_and_start()`:

```rust
// Catalog plugin
host.register(Box::new(plugins::my_plugin::MyPlugin::new()));

// Query plugin
host.register_query(Box::new(plugins::my_query::MyQueryPlugin::new()));
```

Add your module to `src-tauri/src/plugins/mod.rs`:

```rust
pub mod my_plugin;
```

---

## Full Examples

### Static Command List

A simple `CatalogPlugin` with hardcoded actions and icons.

```rust
use crate::plugins::{CatalogPlugin, PluginContext};
use crate::search::types::{Action, ActionId, ActionKeybinding, CatalogEntry, EntryIcon, PostAction};

pub struct DevToolsPlugin;

impl CatalogPlugin for DevToolsPlugin {
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
                clear_cache(app)?;
                Ok(PostAction::Dismiss)
            }
            ("open-logs", ActionId::Open) => {
                open_log_dir(app)?;
                Ok(PostAction::Dismiss)
            }
            ("open-logs", ActionId::Reveal) => {
                reveal_log_dir(app)?;
                Ok(PostAction::Dismiss)
            }
            _ => anyhow::bail!("unhandled: {entry_id} / {action_id:?}"),
        }
    }
}
```

### Prefix-Routed Query Plugin

A `QueryPlugin` that activates exclusively when the user types `>`:

```rust
use std::sync::RwLock;

use crate::plugins::{CatalogPlugin, PluginContext, QueryPlugin};
use crate::search::types::{Action, ActionId, PostAction, ScoredEntry, SearchResponse};

pub struct ScriptRunnerPlugin {
    scripts: RwLock<Vec<Script>>,
}

impl ScriptRunnerPlugin {
    pub fn new() -> Self {
        Self { scripts: RwLock::new(Vec::new()) }
    }
}

impl QueryPlugin for ScriptRunnerPlugin {
    fn id(&self) -> &str {
        "script-runner"
    }

    // Only activated when the user types ">".
    fn prefixes(&self) -> &[&str] {
        &[">"]
    }

    fn setup(&self, _app: &tauri::AppHandle, _ctx: &PluginContext) {
        // Load scripts from disk on startup.
        let mut scripts = self.scripts.write().expect("scripts not poisoned");
        *scripts = load_scripts_from_disk();
    }

    fn search(&self, query: &str, _matched_prefix: Option<&str>) -> SearchResponse {
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

        SearchResponse::Results(results)
    }

    fn execute(
        &self,
        entry_id: &str,
        _action_id: &ActionId,
        _app: &tauri::AppHandle,
    ) -> anyhow::Result<PostAction> {
        run_script(entry_id)?;
        Ok(PostAction::Dismiss)
    }
}
```

### Reactive Background Plugin

A plugin that polls external data, respects an enable/disable toggle, and
reacts to a configurable interval — all without blocking the main thread.

```rust
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Duration;

use crate::plugins::{CatalogPlugin, PluginContext};
use crate::search::types::{Action, ActionId, CatalogEntry, PostAction};
use crate::settings::{PluginSettings, SettingsInit};

pub struct FeedPlugin {
    items: Arc<RwLock<Vec<FeedItem>>>,
    stop: Arc<AtomicBool>,
}

impl FeedPlugin {
    pub fn new() -> Self {
        Self {
            items: Arc::new(RwLock::new(Vec::new())),
            stop: Arc::new(AtomicBool::new(false)),
        }
    }
}

impl CatalogPlugin for FeedPlugin {
    fn id(&self) -> &str {
        "rss-feed"
    }

    fn enabled_settings_key(&self) -> Option<&'static str> {
        // The host watches this key and re-evaluates `is_enabled()` when it changes.
        Some("enabled")
    }

    fn is_enabled(&self) -> bool {
        // Read directly from the store via the stashed settings handle.
        // `initialize_settings` guarantees "enabled" always exists.
        self.settings
            .get()
            .and_then(|s: &PluginSettings| s.get::<bool>("enabled"))
            .unwrap_or(true)
    }

    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit {
        settings
            .ensure("enabled", true)
            .ensure("feedUrl", "https://example.com/feed.xml")
            .ensure("pollIntervalSecs", 300u64)
    }

    fn setup(&self, _app: &tauri::AppHandle, ctx: &PluginContext) {
        // Read initial config.
        let feed_url: String = ctx.settings.get("feedUrl")
            .unwrap_or_default();
        let interval_secs: u64 = ctx.settings.get("pollIntervalSecs")
            .unwrap_or(300);

        // Subscribe to changes that affect the poller.
        let mut enabled_watch = ctx.notifier.watch::<bool>("enabled");
        let mut interval_watch = ctx.notifier.watch::<u64>("pollIntervalSecs");

        let items = Arc::clone(&self.items);
        let stop = Arc::clone(&self.stop);

        thread::spawn(move || {
            let mut enabled = enabled_watch.get();
            let mut interval = Duration::from_secs(interval_secs);

            loop {
                if stop.load(Ordering::Relaxed) {
                    break;
                }

                if enabled {
                    if let Ok(fetched) = fetch_feed(&feed_url) {
                        *items.write().expect("items not poisoned") = fetched;
                    }
                }

                // Sleep in small ticks so we respond to stop/config changes promptly.
                for _ in 0..interval.as_secs() {
                    thread::sleep(Duration::from_secs(1));
                    if stop.load(Ordering::Relaxed) { return; }

                    // Check for setting changes without blocking.
                    if let Ok(new_enabled) = enabled_watch.rx.try_recv() {
                        enabled = new_enabled;
                    }
                    if let Ok(secs) = interval_watch.rx.try_recv() {
                        interval = Duration::from_secs(secs);
                    }
                }
            }
        });
    }

    fn teardown(&self) {
        self.stop.store(true, Ordering::SeqCst);
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
        // entry_id is the URL we stored above.
        open::that(entry_id)?;
        Ok(PostAction::Dismiss)
    }
}
```

**What this example demonstrates:**

- `enabled_settings_key()` — lets the host gate shortcuts and entries without
  polling `is_enabled()` from outside.
- `initialize_settings()` — declares three typed defaults.
- `setup()` — spawns a long-lived background thread and subscribes to two
  reactive watch channels.
- `teardown()` — signals the background thread to exit cleanly.
- `entries()` — defensively handles a not-yet-initialized `RwLock`.
