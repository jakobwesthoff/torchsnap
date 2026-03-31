# Plugin Architecture Overview

This document describes the plugin communication and update mechanism as of
2026-03-31. It covers the full data flow from plugin computation through to
frontend rendering.

## Two Plugin Traits

The system has two plugin traits defined in `src-tauri/src/plugins/mod.rs`:

### CatalogPlugin

For plugins with a finite, pre-known entry list. The host runs nucleo fuzzy
matching on whatever `entries()` returns — the plugin never sees the query.

Implementors: `AppLauncherPlugin`, `SystemPreferencesPlugin`,
`ClipboardPlugin`, `SystemCommandsPlugin`, `BuiltInCommandsPlugin`.

Key search method:
```rust
fn entries(&self) -> Vec<CatalogEntry>;
```

### QueryPlugin

For plugins that run their own matching. Receives the raw query and returns
pre-scored results. Can register prefixes for exclusive routing (e.g., `":"`,
`"="`).

Implementors: `EmojiPickerPlugin`, `CalculatorPlugin`.

Key search methods:
```rust
fn prefixes(&self) -> &[&str];
fn search(&self, query: &str, matched_prefix: Option<&str>,
          results: &ResultChannel, cancel: &AtomicBool);
```

### Shared Lifecycle

Both traits share ten identical method signatures for lifecycle and
configuration:

- `id()` — unique plugin identifier
- `is_enabled()` / `enabled_settings_key()` — enable/disable gating
- `initialize_settings()` — declare defaults at startup (Phase 1)
- `setup()` — async one-time init (Phase 2)
- `teardown()` — cleanup on exit
- `execute()` — handle action selection, returns `PostAction`
- `shortcuts()` / `handle_shortcut()` — global hotkey system
- `handle_message()` — bidirectional custom frontend IPC

These are currently duplicated across both trait definitions with no common
supertrait.

## PluginContext

Bundled at `setup()` time, gives each plugin scoped access to:

- `PluginSettings` — read access to the plugin's settings namespace
- `PluginSettingsNotifier` — reactive watch subscriptions for settings changes
- `PluginFrecency` — scoped frecency scoring handle
