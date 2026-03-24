# 6. Use cross-window settings store with event-based sync

Date: 2026-03-24

## Status

Accepted

## Context

Settings changed in the settings window (e.g. global shortcut) must
be reflected in the launcher window and vice versa. Both windows run
as separate Tauri webviews with independent JavaScript runtimes, so
React state cannot be shared directly.

The `tauri-plugin-store` Rust backend is a singleton per file path —
reads from any webview see the latest data — but it provides no
cross-webview notification mechanism on the JS side.

## Decision

Wrap `tauri-plugin-store` in a singleton module (`settingsStore.ts`)
that adds a global Tauri event layer:

- `setSetting(key, value)` writes to the store, saves, and emits a
  `"settings-changed"` event with the key name.
- Each webview installs a listener (once, lazily) that re-reads the
  updated value from the store and notifies local subscribers.
- The `useSetting(key, default)` React hook handles initial load,
  subscription, and cleanup. It returns `[value, setValue, ready]`.

## Consequences

- Settings changes propagate to all windows with eventual
  consistency (one event round-trip).
- The store file (`settings.json`) is the single source of truth;
  the event is only a notification, not a data carrier.
- Adding a new setting requires only adding a default to
  `settingsDefaults.ts` and calling `useSetting()` at the
  consumption site.
