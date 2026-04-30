# Plugin Architecture Overview

This document describes the plugin system as of 2026-04-05. It covers the
single unified `Plugin` trait, the three search modes, the lifecycle methods,
and the `PluginContext` available to each plugin.

## Single Plugin Trait

All plugins implement one trait defined in `src-tauri/src/plugins/mod.rs`:

```rust
pub trait Plugin: Send + Sync {
    fn id(&self) -> &str;
    fn initialize_settings(&self, settings: SettingsInit) -> SettingsInit { settings }
    fn enable(&self, _app: &tauri::AppHandle, _ctx: &PluginContext) {}
    fn disable(&self) {}
    fn setting_changed(&self, _key: &str, _value: serde_json::Value) {}
    fn execute(&self, entry_id: &str, action_id: &ActionId,
        app: &tauri::AppHandle) -> anyhow::Result<PostAction>;
    fn shortcuts(&self) -> Vec<PluginShortcut> { vec![] }
    fn handle_shortcut(&self, _shortcut_id: &str,
        _app: &tauri::AppHandle) -> anyhow::Result<PostAction> { ... }
    fn handle_message(&self, _method: &str, _payload: serde_json::Value,
        _channel: tauri::ipc::Channel<serde_json::Value>)
        -> anyhow::Result<serde_json::Value> { ... }

    // Search mode — implement one or both:
    fn search_prefixes(&self) -> &[String] { &[] }
    fn entries(&self) -> Vec<CatalogEntry> { vec![] }
    fn search(&self, query: &str, matched_prefix: Option<&str>)
        -> Option<PluginResponse> { None }
}
```

## Search Modes

A plugin participates in search by implementing `entries()`, `search()`, or
both:

**Catalog mode** — the plugin returns a finite, pre-known list via `entries()`.
The host runs nucleo fuzzy matching on that list; the plugin never sees the
raw query. Suitable for app launchers, system preferences, clipboard history.

**Query mode** — the plugin receives the raw query via `search()` and returns
pre-scored results itself. It can optionally register exclusive-routing prefixes
via `search_prefixes()` (e.g. `":"` for bangs, `"="` for calculator). When a
query starts with a registered prefix, only that plugin's `search()` is called.

**Hybrid** — a plugin may implement both `entries()` and `search()`. This is
uncommon but supported; the host collects results from both paths.

### Current implementors

| Plugin | Mode |
|---|---|
| `AppLauncherPlugin` | Catalog |
| `SystemPreferencesPlugin` | Catalog |
| `ClipboardPlugin` | Catalog |
| `SystemCommandsPlugin` | Catalog |
| `BuiltInCommandsPlugin` | Catalog |
| `EmojiPickerPlugin` | Query |
| `CalculatorPlugin` | Query (prefix `"="`) |
| `WasmPluginBridge` | Query (delegates to WASM guest) |

## Lifecycle

Plugins move through the following phases:

1. **`initialize_settings(settings)`** — called once at startup before any
   plugin is enabled. Declares default values into the settings store so
   the frontend can render controls even before the user has changed anything.

2. **`enable(app, ctx)`** — called when the plugin is activated (at startup
   for enabled plugins, and later if the user toggles the plugin on).
   Receives an `AppHandle` and a `PluginContext`. This is where background
   tasks are started and resources are acquired.

3. **`setting_changed(relative_key, value)`** — called whenever a
   `plugins.<id>.<key>` setting changes. Receives only the relative key (e.g.
   `"retentionDays"`, not the full path). See
   [06-settings-reactivity.md](06-settings-reactivity.md) for the full flow.

4. **`disable()`** — called when the user disables the plugin. Background
   tasks should be stopped here; the plugin may be re-enabled later.

5. **`execute(entry_id, action_id, app)`** — called when the user activates
   a result from this plugin. Returns a `PostAction` telling the host what
   to do next (dismiss, keep open, show custom UI, etc.).

## Enable / Disable Gating

The host wraps each plugin in a `PluginSlot` that owns:

- an `AtomicBool` for the enabled state
- a `CoalescingDispatcher` for serialising `setting_changed` calls

The enabled key lives at `enabled.<plugin-id>` in the top-level settings
namespace — outside the `plugins.<id>.*` prefix and inaccessible to the
plugin itself. The host intercepts changes to that key and calls `enable()`
or `disable()` directly.

## PluginContext

`PluginContext` is handed to a plugin at `enable()` time and provides scoped
access to two subsystems:

- **`settings: PluginSettings`** — read access to the plugin's settings
  namespace (`plugins.<id>.*`).
- **`frecency: PluginFrecency`** — scoped frecency scoring handle for ranking
  results by past user selection.

## Discovery and distribution

Native plugins (`Builtin`) are compiled into the host binary and
registered manually in `lib.rs` during setup. WASM plugins are
discovered at startup from three precedence-ordered search roots
(resource-bundled System, repo-relative Dev in debug builds,
user-installed User), each plugin tagged with its
`PluginSourceKind`. The full rules — including the per-root
archive-over-directory precedence, cross-root collision handling,
install/uninstall flow, and the `plugins/bundled.toml` whitelist —
live in **[ADR 0035](../adr/0035-plugin-distribution-via-bundled-and-user-installable-archives.md)**.

Host-managed plugin state (SQLite databases, future blob storage)
lives under `<app_data_dir>/plugin-home/<plugin-id>/`, separate
from plugin code at `<app_data_dir>/plugins/`. See ADR 0018 and
0019 for the storage shape.
