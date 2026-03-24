# 3. Use programmatic window creation

Date: 2026-03-24

## Status

Accepted

## Context

The launcher window requires unusual properties that cannot be fully
expressed in `tauri.conf.json`: transparent background, no
decorations, no shadow, hidden at startup, and platform-specific
post-creation setup (NSPanel conversion on macOS). The settings
window needs an overlay titlebar style on macOS but standard
decorations elsewhere.

Declaring windows in the config would create them before the
`setup()` callback runs, causing a visible flash before platform
initialization can configure them.

## Decision

Set `"windows": []` in `tauri.conf.json` and create all windows
programmatically in the `setup()` callback using
`WebviewWindowBuilder`. Both the launcher and settings windows are
created hidden (`visible(false)`, `focused(false)`) at startup to
preload their webviews and avoid first-show latency.

Window close events are intercepted via `RunEvent::WindowEvent` /
`CloseRequested` to hide instead of destroy, so both windows persist
for the lifetime of the app.

## Consequences

- Full control over window properties and creation order.
- No flash on first show — webviews are warm before the user
  triggers the shortcut.
- Platform-specific builder calls (e.g. `title_bar_style`) can be
  conditionally applied with `#[cfg]` blocks.
- The `setup()` function is longer but all window creation logic
  is co-located.
